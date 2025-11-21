use anyhow::{anyhow, bail, Context, Result};
use containerd_shim_wasm::sandbox::context::{Entrypoint, RuntimeContext, Source, WasmBinaryType, WasmLayer};
use containerd_shim_wasm::sandbox::Sandbox;
use containerd_shim_wasm::shim::{version, Compiler, Shim, Version};
use log::{debug, error, info};
use oci_spec::image::MediaType;
use peridot::conf::PeridotConfig;
use peridot::context;
use peridot::context::WasiWrapper;
use peridot::metrics::{DiskIOMetricsProducer, MetricsPublisher, MetricsSubscriber};
#[cfg(feature = "token")]
use peridot::token::TokenBucket;
use std::fs::File;
use std::hash::Hash;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use wasmtime::{Config, Engine, Linker, Module, Precompiled, Store};
use wasmtime_wasi::WasiCtxBuilder;

use crate::constants;
#[cfg(feature = "token")]
use peridot_token_ctx::PeridotTokenCtx;

pub struct PeridotShim;

pub struct PeridotCompiler(Engine);

pub struct PeridotSandbox {
    engine: Engine,
    metrics_publisher: Option<Arc<Mutex<MetricsPublisher>>>,
}

impl Default for PeridotSandbox {
    fn default() -> Self {
        let mut config = Config::new();
        config.async_support(true);

        Self {
            engine: Engine::new(&config)
                .context("Failed to create wasmtime engine")
                .unwrap(),
            metrics_publisher: Some(Arc::new(Mutex::new(MetricsPublisher::new(vec![], vec![])))),
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

        let engine = Engine::new(&config).expect("failed to create wasmtime precompilation engine");

        Some(PeridotCompiler(engine))
    }

    fn supported_layers_types() -> &'static [&'static str] {
        &[
            constants::OCI_LAYER_MEDIA_TYPE_WASM,
            "application/wasm",
            constants::OCI_LAYER_MEDIA_TYPE_PERIDOT_CONFIG,
        ]
    }
}

impl Sandbox for PeridotSandbox {
    async fn run_wasi(&self, ctx: &impl RuntimeContext) -> Result<i32> {
        info!("Setting up wasi");

        let peridot_config = load_peridot_config(ctx).await?;

        // Subscribe metrics producers and start metrics update thread
        if let Some(mp) = self.metrics_publisher.as_ref() {
            mp.lock()
                .await
                .subscribe_producer(Arc::new(DiskIOMetricsProducer {}));

            mp.lock()
                .await
                .spawn_metrics_update_thread(&peridot_config)
                .await?;
        }

        let Entrypoint {
            source,
            func,
            arg0: _,
            name: _,
        } = ctx.entrypoint();

        let wasm_bytes = &source.as_bytes()?;
        let result = self
            .execute(ctx, peridot_config, wasm_bytes, func)
            .await
            .into_error_code();

        if let Some(mp) = self.metrics_publisher.as_ref() {
            mp.lock().await.stop_metrics_update_thread().await;
        }

        result
    }

    async fn can_handle(&self, _ctx: &impl RuntimeContext) -> Result<()> {
        Ok(())
    }
}

impl Compiler for PeridotCompiler {
    fn cache_key(&self) -> impl Hash {
        self.0.precompile_compatibility_hash()
    }

    async fn compile(&self, layers: &[WasmLayer]) -> Result<Vec<Option<Vec<u8>>>> {
        let precompiled_layers = layers
            .iter()
            .map(|layer| match is_wasm_content(layer) {
                Some(wasm_layer) => {
                    info!(
                        "Precompiling wasm layer {:?}",
                        wasm_layer.config.digest()
                    );
                    if Engine::detect_precompiled(&wasm_layer.layer).is_some() {
                        info!("Layer already precompiled {:?}", wasm_layer.config.digest());
                        Ok(Some(wasm_layer.layer))
                    } else {
                        let precompiled: Option<Vec<u8>> = match WasmBinaryType::from_bytes(&wasm_layer.layer) {
                            Some(WasmBinaryType::Module) => Some(self.0.precompile_module(&wasm_layer.layer)?),
                            Some(WasmBinaryType::Component) => {
                                error!("Peridot does not support precompilation of components yet");
                                None
                            }
                            None => {
                                error!("Unknown WASM binary type");
                                None
                            }
                        };
                        Ok(precompiled)
                    }
                }
                None => Ok(None),
            })
            .collect::<Result<_>>()?;
        Ok(precompiled_layers)
    }
}

pub(crate) fn is_wasm_content(layer: &WasmLayer) -> Option<WasmLayer> {
    if let MediaType::Other(name) = layer.config.media_type() {
        if name == constants::OCI_LAYER_MEDIA_TYPE_WASM {
            info!("WASM layer {:?} is Wasm, Type={:?}", layer.config.digest(), name);
            return Some(layer.clone());
        }
    }
    None
}

impl PeridotSandbox {
    async fn execute_module(
        &self,
        ctx: &impl RuntimeContext,
        config: PeridotConfig,
        module: Module,
        func: &String,
    ) -> Result<i32> {
        debug!("execute module");

        let ctx_p1 = wasi_builder(ctx)?.build_p1();
        let mut module_linker = Linker::new(&self.engine);
        debug!("init linker");

        #[cfg(feature = "token")]
        let max_bandwidth: u64 = config.io.max_bandwidth as u64;
        #[cfg(feature = "token")]
        let token_bucket = Arc::new(std::sync::Mutex::new(TokenBucket::new(
            max_bandwidth,
            max_bandwidth,
            max_bandwidth,
        )));

        #[cfg(feature = "token")]
        let peridot_ctx = PeridotTokenCtx::new(
            context::PeridotContext::new(ctx_p1),
            Arc::clone(&token_bucket),
        );
        #[cfg(feature = "token")]
        context::add_to_linker_async(
            &mut module_linker,
            |wasi_ctx: &mut WasiWrapper<PeridotTokenCtx>| wasi_ctx,
        )?;

        let wrapped_ctx = WasiWrapper::new(peridot_ctx);

        let mut store = Store::new(&self.engine, wrapped_ctx);

        let token_bucket =
            token_bucket as Arc<std::sync::Mutex<dyn MetricsSubscriber + Send + Sync>>;

        #[cfg(feature = "token")]
        if let Some(metrics_publisher) = &self.metrics_publisher {
            metrics_publisher
                .lock()
                .await
                .subscribe(Arc::clone(&token_bucket));
        }

        info!("instantiating instance");
        let instance: wasmtime::Instance =
            module_linker.instantiate_async(&mut store, &module).await?;

        debug!("getting start function");
        let start_func = instance
            .get_func(&mut store, func)
            .context("module does not have a WASI start function")?;

        info!("running start function {func:?}");

        start_func
            .call_async(&mut store, &[], &mut [])
            .await
            .into_error_code()
    }

    async fn execute(
        &self,
        ctx: &impl RuntimeContext,
        config: PeridotConfig,
        wasm_binary: &[u8],
        func: String,
    ) -> Result<i32> {
        match WasmBinaryType::from_bytes(wasm_binary) {
            Some(WasmBinaryType::Module) => {
                debug!("loading wasm module");
                let module = Module::from_binary(&self.engine, wasm_binary)?;
                self.execute_module(ctx, config, module, &func).await
            }
            Some(WasmBinaryType::Component) => {
                bail!("Peridot does not support direct execution of components yet");
            }
            None => match Engine::detect_precompiled(wasm_binary) {
                Some(Precompiled::Module) => {
                    info!("using precompiled module");
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
    debug!("building WASI context");

    let file_perms = wasmtime_wasi::FilePerms::all();
    let dir_perms = wasmtime_wasi::DirPerms::all();
    let envs = envs_from_ctx(ctx);

    let mut builder = WasiCtxBuilder::new();
    builder
        .args(ctx.args())
        .envs(&envs)
        .inherit_stdio()
        .preopened_dir("/", "/", dir_perms, file_perms)?;

    debug!("WASI context built successfully");
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

pub async fn load_peridot_config(ctx: &impl RuntimeContext) -> Result<PeridotConfig> {
    match ctx.entrypoint().source {
        Source::File(_) => Err(anyhow!("Not implemented")),

        Source::Oci(layers) => {
            for artifact in layers {
                match artifact.config.media_type() {
                    MediaType::Other(name)
                        if name == constants::OCI_LAYER_MEDIA_TYPE_PERIDOT_CONFIG =>
                    {
                        let path = PathBuf::from("/peridot_conf.yaml");
                        info!("Writing Peridot OCI config to {path:?}");
                        File::create(&path)
                            .context("failed to create peridot config files")?
                            .write_all(&artifact.layer)
                            .context("failed to write peridot config file")?;
                        return Ok(PeridotConfig::new("/peridot_conf.yaml")?);
                    }
                    MediaType::Other(name)
                        if name
                            == constants::OCI_LAYER_MEDIA_TYPE_WASM =>
                    {
                        debug!("This is the WASM layer! Size = {:?}", artifact.layer.len(),);
                    }
                    _ => {
                        debug!("<<< unknown media type {:?}", artifact.config.media_type());
                    }
                }
            }

            Err(anyhow::anyhow!("Peridot OCI config layer not found."))
        }
    }
}
