use peridot::metrics::{DiskIOMetricsProducer, MetricsSubscriber};
use clap::Parser;

use std::path::Path;
use std::sync::Arc;
use std::{env, process};
use wasmtime::{Config, Engine, Linker, Module, Result, Store};

use peridot::conf::PeridotConfig;
use peridot::context;
use peridot::context::{PeridotContext, WasiWrapper};
use peridot::metrics::MetricsPublisher;

#[cfg(feature = "token")]
use peridot::token::TokenBucket;
use tokio::sync::Mutex;

#[cfg(feature = "counter")]
use peridot::counter::PeridotCounter;
#[cfg(feature = "clock")]
use peridot_clock_ctx::PeridotClockCtx;
#[cfg(feature = "counter")]
use peridot_counter_ctx::PeridotCounterCtx;
#[cfg(feature = "s3")]
use peridot_s3_ctx::PeridotS3Ctx;
#[cfg(feature = "token")]
use peridot_token_ctx::PeridotTokenCtx;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

/// Peridot - Transparent Integration of new logic in legacy Wasm modules
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the WebAssembly module to run
    #[arg(required = true)]
    module_path: Option<String>,

    /// Path to the YAML configuration file (e.g. config.yaml)
    #[arg(required = true)]
    config_path: String,

    /// Log level (e.g. debug, info, warn, error). Defaults to "info".
    /// Overridden by RUST_LOG env var if set.
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

async fn run_module(
    module_path: String,
    config: PeridotConfig,
) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::new(Config::new().async_support(true))?;
    let mut linker = Linker::new(&engine);

    // Set up WASI
    let args: Vec<String> = config.args.clone();
    println!("Module path: {}", module_path);
    let module_dir = Path::new(&module_path).parent().unwrap();
    println!("Module dir: {}",module_dir.display());

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .preopened_dir(
        module_dir,
        ".",
        DirPerms::all(),
        FilePerms::all(),
    )?.args(&*args).build_p1();

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
        PeridotContext::new(wasi),
        Arc::clone(&token_bucket),
    );

    #[cfg(feature = "counter")]
    let counter = Arc::new(std::sync::Mutex::new(PeridotCounter::new()));

    #[cfg(feature = "geds")]
    let peridot_ctx = PeridotGEDSCtx::new(PeridotContext::new(wasi));
    #[cfg(feature = "clock")]
    let peridot_ctx = PeridotClockCtx::new(PeridotContext::new(wasi));
    #[cfg(feature = "counter")]
    let peridot_ctx = PeridotCounterCtx::new(PeridotContext::new(wasi), counter.clone());
    #[cfg(feature = "s3")]
    let peridot_ctx = PeridotS3Ctx::new(PeridotContext::new(wasi)).await;
    #[cfg(feature = "token")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotTokenCtx>| wasi_ctx )?;
    #[cfg(feature = "geds")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotGEDSCtx>| wasi_ctx )?;
    #[cfg(feature = "clock")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotClockCtx>| wasi_ctx )?;
    #[cfg(feature = "counter")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotCounterCtx>| wasi_ctx )?;
    #[cfg(feature = "s3")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotS3Ctx>| wasi_ctx )?;

    let wrapped_ctx = WasiWrapper::new(peridot_ctx);
    let mut store = Store::new(&engine, wrapped_ctx);
    linker.allow_shadowing(true);

    let metrics_publisher = Some(Arc::new(Mutex::new(MetricsPublisher::new(vec![], vec![]))));

    // Subscribe metrics producers and start metrics update thread
    if let Some(mp) = &metrics_publisher {
        #[cfg(feature = "token")]
        mp.lock()
            .await
            .subscribe(Arc::clone(&(token_bucket as Arc<std::sync::Mutex<dyn MetricsSubscriber + Send + Sync>>)));

        #[cfg(feature = "counter")]
        mp.lock()
            .await
            .subscribe_producer(Box::new(counter));

        mp.lock()
            .await
            .subscribe_producer(Box::new(DiskIOMetricsProducer {}));

        mp.lock()
            .await
            .spawn_metrics_update_thread(&config)
            .await?;
    }

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_path)?;
    linker.module(&mut store, "", &module)?;

    let instance = linker.instantiate_async(&mut store, &module).await?;
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    start.call_async(&mut store, ()).await?;

    #[cfg(feature="s3")]
    store.data_mut().ctx.drain_uploads().await;

    #[cfg(feature = "counter")]
    if let Some(mp) = metrics_publisher {
        mp.lock().await.stop_metrics_update_thread().await;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", args.log_level);
    }
    env_logger::init();

    let module_path = match &args.module_path {
        Some(path) => path.clone(),
        None => {
            eprintln!("Error: Module path is required.");
            process::exit(1);
        }
    };

    let config = match PeridotConfig::new(&args.config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error reading \"{}\": {}", &args.config_path, e);
            process::exit(1);
        }
    };

    run_module(module_path, config).await?;

    Ok(())
}