use clap::Parser;
use log::{debug, error, info};
use nix::sys::wait::waitpid;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};
use peridot_overseer_grpc::client::OverseerGrpcClient;

use std::path::Path;
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};
use wasi_common::sync::{Dir, WasiCtxBuilder};
use wasmtime::*;

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

fn run_module(
    module_path: &str,
    args: &[String],
    _i: usize,
    _overseer_address: &Option<String>,
    _demand: f64,
) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);

    info!("Args {:?}", args);

    // Set up WASI
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .preopened_dir(Dir::from_std_file(
            fs::File::open(Path::new(module_path).parent().unwrap())?), Path::new(module_path).parent().unwrap())?
        .args(args)?
        .build();

    let wasi_ctx = peridot_geds_ctx::PeridotGEDSCtx::new(wasi);
    peridot_geds_ctx::add_to_linker(&mut linker, |cx| cx)?;
    let mut store = Store::new(&engine, wasi_ctx);
    linker.allow_shadowing(true);
    let module = Module::from_file(&engine, module_path)?;
    linker.module(&mut store, "", &module)?;
    
    info!("STARTING MODULE>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>>><<<<");
    linker
        .get_default(&mut store, "")?
        .typed::<(), ()>(&store)?
        .call(&mut store, ())?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", args.log_level);
    }
    env_logger::init();

    let overseer_address = &args.overseer_address;

    let config = match peridot::conf::new_config(&args.config_path) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error reading \"{}\": {}", &args.config_path, e);
            process::exit(1);
        }
    };

    if config.is_empty() {
        eprintln!("No configurations found in \"{}\".", &args.config_path);
        process::exit(1);
    }

    let mut children = vec![];
    let mut client = None;
    if let Some(ref address) = overseer_address {
        client = Some(OverseerGrpcClient::new(address));
    }

    // sort config by 'priority' key
    let mut config: Vec<_> = config.into_iter().collect();
    config.sort_by(|a, b| {
        a.1.peridot_config
            .get("priority")
            .unwrap()
            .partial_cmp(b.1.peridot_config.get("priority").unwrap())
            .unwrap()
    });

    // todo: what happens if the parent process is killed? Use UNIX process groups and signal handling
    for (module_path, module_config) in config.into_iter() {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                if let Err(e) = run_module(
                    &module_path,
                    &module_config.args,
                    children.len(),
                    overseer_address,
                    *module_config.peridot_config.get("demand").unwrap(),
                ) {
                    eprintln!("Error running module \"{}\": {}", &module_path, e);
                    process::exit(1);
                }
                process::exit(0);
            }
            Ok(ForkResult::Parent { child }) => {
                info!("Spawned process with PID: {}", child);
                children.push(child);
            }
            Err(e) => {
                error!("Fork failed: {}", e);
            }
        }
    }

    let interval = Duration::from_secs(1);
    let mut next_tick = Instant::now();

    while !children.is_empty() {
        let mut to_remove = vec![];

        for (index, pid) in children.iter().enumerate() {
            info!("Waiting");
            match waitpid(Some(*pid), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, status)) => {
                    info!("End!");
                    if let Some(ref mut client) = client {
                        client.remove_module(pid.as_raw() as u32)?;
                    }
                    info!("Process {} exited with status {}", pid, status);
                    to_remove.push(index);
                }
                Ok(_) => {}
                Err(err) => {
                    error!("Error while waiting for {}: {}", pid, err);
                    to_remove.push(index);
                }
            }
            next_tick += interval;
            let now = Instant::now();
            if next_tick > now {
                thread::sleep(next_tick - now);
            } else {
                next_tick = now;
            }
        }
        for index in to_remove.iter().rev() {
            children.remove(*index);
        }
    }
    Ok(())
}
