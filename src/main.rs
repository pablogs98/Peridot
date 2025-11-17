use clap::Parser;
use log::{debug, error};

use nix::sys::wait::waitpid;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};

use std::path::Path;
use std::time::{Duration, Instant};
use std::{env, process, thread};
use wasmtime::{Engine, Linker, Module, Store, Result, Config};

use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};
use peridot::conf::PeridotConfig;
use peridot::context;
use peridot::context::{PeridotContext, WasiWrapper};
#[cfg(feature = "clock")]
use peridot_clock_ctx::PeridotClockCtx;
#[cfg(feature = "counter")]
use peridot_counter_ctx::PeridotCounterCtx;

#[cfg(feature = "s3")]
use peridot_s3_ctx::PeridotS3Ctx;


/// Peridot - Transparent Integration of new logic in legacy Wasm modules
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the YAML configuration file (e.g. config.yaml)
    #[arg(required = true)]
    config_path: String,

    /// Overseer Unix Domain Socket address (e.g. /tmp/peridot.sock)
    #[arg(short = 'o', long, required = false)]
    overseer_address: Option<String>,

    /// Log level (e.g. debug, info, warn, error). Defaults to "info".
    /// Overridden by RUST_LOG env var if set.
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

async fn run_module(
    module_path: &str,
    args: &Vec<String>,
    _i: usize,
    _overseer_address: &Option<String>,
    _demand: f64,
) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::new(Config::new().async_support(true))?;
    let mut linker = Linker::new(&engine);

    // Set up WASI
    println!("Module path: {}", module_path);
    let module_dir = Path::new(module_path).parent().unwrap();
    println!("Module dir: {}",module_dir.display());

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .preopened_dir(
        module_dir,
        ".",
        DirPerms::all(),
        FilePerms::all(),
    )?.args(args).build_p1();


    #[cfg(feature = "geds")]
    let peridot_ctx = PeridotGEDSCtx::new(PeridotContext::new(wasi));
    #[cfg(feature = "clock")]
    let peridot_ctx = PeridotClockCtx::new(PeridotContext::new(wasi));
    #[cfg(feature = "counter")]
    let peridot_ctx = PeridotCounterCtx::new(PeridotContext::new(wasi));
    #[cfg(feature = "s3")]
    let peridot_ctx = PeridotS3Ctx::new(PeridotContext::new(wasi)).await;
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
    
    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_path)?;
    linker.module(&mut store, "", &module)?;

    let instance = linker.instantiate_async(&mut store, &module).await?;
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    start.call_async(&mut store, ()).await?;

    #[cfg(feature="s3")]
    store.data_mut().ctx.drain_uploads().await;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", args.log_level);
    }
    env_logger::init();

    let overseer_address = &args.overseer_address;

    let config = match PeridotConfig::new(&args.config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error reading \"{}\": {}", &args.config_path, e);
            process::exit(1);
        }
    };


    Ok(())
}