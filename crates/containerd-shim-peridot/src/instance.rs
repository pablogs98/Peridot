use anyhow::{bail, Context, Result};
use containerd_shim_wasm::sandbox::context::{
    Entrypoint, RuntimeContext, WasmBinaryType, WasmLayer,
};
use containerd_shim_wasm::sandbox::Sandbox;
use containerd_shim_wasm::shim::{version, Compiler, Shim, Version};
use peridot::token::TokenBucket;
use peridot_overseer_grpc::client::OverseerGrpcClient;
use std::collections::HashMap;
use std::hash::Hash;
use std::path::Path;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, LazyLock, Mutex};
use std::{fs, thread};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;
use wasmtime::component::types::ComponentItem;
use wasmtime::component::{self, Component, ResourceTable};
use wasmtime::{Config, Engine, Linker, Module, Precompiled, Store};
use wasmtime_wasi::p1::WasiP1Ctx;
use wasmtime_wasi::WasiCtxBuilder;
use peridot::context;
use peridot::context::WasiWrapper;
use peridot::metrics::{DiskIOMetricsProducer, MetricsPublisher};
use peridot_token_ctx::PeridotTokenCtx;

pub struct PeridotShim;

pub struct PeridotCompiler(Engine);

pub struct PeridotSandbox {
    engine: Engine,
    metrics_publisher: Option<MetricsPublisher>,
    cancel: CancellationToken,
}

impl Default for PeridotSandbox {
    fn default() -> Self {
        let mut config = Config::new();
        config.async_support(true);

        Self {
            engine: Engine::new(&config)
                .context("Failed to create wasmtime engine")
                .unwrap(),
            cancel: CancellationToken::new(),
            metrics_publisher: Some(MetricsPublisher::new(vec![], vec![])),
        }
    }
}

impl Shim for PeridotShim {
    fn name() -> &'static str {
        "peridot"
    }

    fn version() -> Version {
        version!()
    }

    type Sandbox = PeridotSandbox;

    #[allow(refining_impl_trait)]
    async fn compiler() -> Option<PeridotCompiler> {
        let mut config = Config::new();

        config.async_support(true); // must be on

        let engine = Engine::new(&config)
            .expect("failed to create wasmtime precompilation engine");

        Some(PeridotCompiler(engine))
    }
}

impl Sandbox for PeridotSandbox {
    async fn run_wasi(&self, ctx: &impl RuntimeContext) -> Result<i32> {
        log::info!("Setting up wasi");

        let peridot_config =
            peridot::conf::PeridotConfig::new("/peridot_config.yaml").into_error_code();

        // Subscribe metrics producers and start metrics update thread
        self.metrics_publisher.unwrap().subscribe_producer(DiskIOMetricsProducer{});

        self.metrics_publisher
            .unwrap()
            .spawn_metrics_update_thread(peridot_config)
            .await;

        let Entrypoint {
            source,
            func,
            arg0: _,
            name: _,
        } = ctx.entrypoint();

        let wasm_bytes = &source.as_bytes()?;
        let result = self.execute(ctx, peridot_config, wasm_bytes, func).await.into_error_code();

        self.metrics_publisher.unwrap().stop_metrics_update_thread();
        result
    }
}

impl Compiler for PeridotCompiler {
    fn cache_key(&self) -> impl Hash {
        self.0.precompile_compatibility_hash()
    }

    async fn compile(&self, layers: &[WasmLayer]) -> Result<Vec<Option<Vec<u8>>>> {
        let mut compiled_layers = Vec::<Option<Vec<u8>>>::with_capacity(layers.len());

        for layer in layers {
            if Engine::detect_precompiled(&layer.layer).is_some() {
                log::info!("Already precompiled");
                compiled_layers.push(None);
                continue;
            }

            let compiled_layer = match WasmBinaryType::from_bytes(&layer.layer) {
                Some(WasmBinaryType::Module) => self.0.precompile_module(&layer.layer)?,
                Some(WasmBinaryType::Component) => {
                    bail!("Peridot does not support precompilation of components yet")
                }
                None => {
                    log::warn!("Unknown WASM binary type");
                    continue;
                }
            };

            compiled_layers.push(Some(compiled_layer));
        }

        Ok(compiled_layers)
    }
}

impl PeridotSandbox {
    async fn execute_module(
        &self,
        ctx: &impl RuntimeContext,
        config: peridot::conf::PeridotConfig,
        module: Module,
        func: &String,
    ) -> Result<i32> {
        log::debug!("execute module");

        let ctx_p1 = wasi_builder(ctx)?.build_p1();
        let mut module_linker = Linker::new(&self.engine);
        log::debug!("init linker");

        #[cfg(feature = "token")]
        let max_bandwidth: u64 = config.io.max_bandwidth as u64;
        #[cfg(feature = "token")]
        let token_bucket = Arc::new(Mutex::new(TokenBucket::new(
            max_bandwidth,
            max_bandwidth,
            max_bandwidth,
        )));

        #[cfg(feature = "token")]
        let peridot_ctx = PeridotTokenCtx::new(
            context::PeridotContext::new(ctx_p1),
            Arc::clone(&token_bucket),
        );

        context::add_to_linker_async(&mut module_linker, |wasi_ctx: &mut WasiWrapper<PeridotTokenCtx>| wasi_ctx )?;
        let wrapped_ctx = WasiWrapper::new(peridot_ctx);

        let mut store = Store::new(&self.engine, wrapped_ctx);

        self.metrics_publisher
            .unwrap()
            .subscribe(Arc::clone(&token_bucket));

        log::info!("instantiating instance");
        let instance: wasmtime::Instance =
            module_linker.instantiate_async(&mut store, &module).await?;

        log::debug!("getting start function");
        let start_func = instance
            .get_func(&mut store, func)
            .context("module does not have a WASI start function")?;

        log::info!("running start function {func:?}");

        start_func
            .call_async(&mut store, &[], &mut [])
            .await
            .into_error_code()
    }

    async fn execute(
        &self,
        ctx: &impl RuntimeContext,
        config: peridot::conf::PeridotConfig,
        wasm_binary: &[u8],
        func: String,
    ) -> Result<i32> {
        match WasmBinaryType::from_bytes(wasm_binary) {
            Some(WasmBinaryType::Module) => {
                log::debug!("loading wasm module");
                let module = Module::from_binary(&self.engine, wasm_binary)?;
                self.execute_module(ctx, config, module, &func).await
            }
            Some(WasmBinaryType::Component) => {
                bail!("Peridot does not support direct execution of components yet");
            }
            None => match Engine::detect_precompiled(wasm_binary) {
                Some(Precompiled::Module) => {
                    log::info!("using precompiled module");
                    let module = unsafe { Module::deserialize(&self.engine, wasm_binary) }?;
                    self.execute_module(ctx, config, module, &func).await
                }
                Some(Precompiled::Component) => {
                    bail!("Peridot does not support direct execution of components yet");
                }
                None => {
                    bail!("invalid precompiled module")
                }
            },
        }
    }
}

pub(crate) fn envs_from_ctx(ctx: &impl RuntimeContext) -> Vec<(String, String)> {
    ctx.envs()
        .iter()
        .map(|v| {
            let (key, value) = v.split_once('=').unwrap_or((v.as_str(), ""));
            (key.to_string(), value.to_string())
        })
        .collect()
}

fn wasi_builder(ctx: &impl RuntimeContext) -> Result<WasiCtxBuilder, anyhow::Error> {
    log::debug!("building WASI context");

    let file_perms = wasmtime_wasi::FilePerms::all();
    let dir_perms = wasmtime_wasi::DirPerms::all();
    let envs = envs_from_ctx(ctx);

    let mut builder = WasiCtxBuilder::new();
    builder
        .args(ctx.args())
        .envs(&envs)
        .inherit_stdio()
        .preopened_dir("/", "/", dir_perms, file_perms)?;

    log::debug!("WASI context built successfully");
    Ok(builder)
}

pub trait IntoErrorCode {
    fn into_error_code(self) -> Result<i32>;
}

impl IntoErrorCode for Result<i32> {
    fn into_error_code(self) -> Result<i32> {
        self.or_else(|err| match err.downcast_ref::<wasmtime_wasi::I32Exit>() {
            Some(exit) => Ok(exit.0),
            _ => Err(err),
        })
    }
}

impl IntoErrorCode for Result<()> {
    fn into_error_code(self) -> Result<i32> {
        self.map(|_| 0).into_error_code()
    }
}
