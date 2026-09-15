use peridot::metrics::DiskIOMetricsProducer;
use clap::Parser;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{env, process};
use wasmtime::{Config, Engine, Linker, Module, Result, Store};

use peridot::conf::PeridotConfig;
use peridot::context;
use peridot::context::{PeridotContext, WasiWrapper};
use peridot::metrics::MetricsPublisher;
use peridot::plugin::{Preopen, PluginRegistry};

use tokio::sync::Mutex;
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

/// Contexts available to this binary, keyed by the name used in `config.yaml`.
///
/// Which of these actually run, and in what order, is decided at run time by the `contexts`
/// list in the configuration file — building Peridot no longer requires choosing.
fn build_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    registry.register("counter", peridot_counter_ctx::factory);
    registry.register("clock", peridot_clock_ctx::factory);
    registry.register("token", peridot_token_ctx::factory);
    registry.register("s3", peridot_s3_ctx::factory);
    registry.register("batch", peridot_batch_ctx::factory);
    registry.register("syscall-batching", peridot_syscall_batching_ctx::factory);
    registry
}

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
    // `Path::parent` yields `""` for a bare filename, which `preopened_dir` rejects, so fall
    // back to the working directory. Canonicalizing keeps the preopen an absolute host path,
    // which is what contexts resolving guest paths against it expect.
    let module_dir = match Path::new(&module_path).parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let module_dir = module_dir.canonicalize().unwrap_or(module_dir);
    println!("Module dir: {}", module_dir.display());

    // Recorded so contexts that open host files themselves can resolve guest paths; the
    // fd-to-directory mapping inside WasiP1Ctx is not reachable from a context.
    let preopens = vec![Preopen {
        host: module_dir.clone(),
        guest: ".".to_string(),
    }];

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .preopened_dir(
        &module_dir,
        ".",
        DirPerms::all(),
        FilePerms::all(),
    )?.args(&*args).build_p1();

    // Load any plugin libraries first, so the contexts they provide can be named below.
    let mut registry = build_registry();
    for path in &config.plugins {
        // SAFETY: the operator named this library in their own configuration file, and
        // `load_library` refuses anything not built against this exact runtime ABI.
        let added = unsafe { registry.load_library(path) }?;
        log::info!("loaded plugin {} providing {:?}", path.display(), added);
    }
    let registry = registry;
    let mut chain = registry
        .build_chain(PeridotContext::boxed(wasi), &config, &preopens)
        .await?;
    log::info!("Context chain: {:?}", config.context_names());

    // Metrics have to be collected from the chain before it is moved into the store.
    let (producers, subscribers) = context::collect_metrics(&mut *chain);

    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper| wasi_ctx)?;

    let mut store = Store::new(&engine, WasiWrapper::new(chain));
    linker.allow_shadowing(true);

    let metrics_publisher = Arc::new(Mutex::new(MetricsPublisher::new(vec![], vec![])));

    // Subscribe metrics producers and start metrics update thread
    {
        let mut mp = metrics_publisher.lock().await;
        for producer in producers {
            mp.subscribe_producer(producer);
        }
        for subscriber in subscribers {
            mp.subscribe(subscriber);
        }
        mp.subscribe_producer(Arc::new(DiskIOMetricsProducer {}));

        // The metrics thread reports to the overseer, so it is only useful with one configured.
        if config.overseer_address.is_some() {
            mp.spawn_metrics_update_thread(&config).await?;
        } else {
            log::info!("No overseer_address configured; not starting the metrics thread.");
        }
    }

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_path)?;
    linker.module(&mut store, "", &module)?;

    let instance = linker.instantiate_async(&mut store, &module).await?;
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    start.call_async(&mut store, ()).await?;

    // Let every link flush whatever work it deferred (uploads, batches, log files).
    store.data_mut().ctx.shutdown().await;

    if config.overseer_address.is_some() {
        metrics_publisher.lock().await.stop_metrics_update_thread().await;
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