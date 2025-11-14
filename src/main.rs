use clap::Parser;
use log::{debug, error};
#[cfg(feature = "token")]
use log::warn;
use nix::sys::wait::waitpid;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};

use std::path::Path;
use std::time::{Duration, Instant};
use std::{env, process, thread};
use wasmtime::{Engine, Linker, Module, Store, Result, Config};

#[cfg(feature = "token")]
use std::collections::HashMap;
#[cfg(feature = "token")]
use std::io::{Read, Write};
#[cfg(feature = "token")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "token")]
use std::sync::{Arc, Mutex};
#[cfg(feature = "token")]
use std::os::unix::net::{UnixListener, UnixStream};
use wasmtime_wasi::{DirPerms, FilePerms, WasiCtx, WasiCtxBuilder};
use peridot::conf::PeridotConfig;
use peridot::context;
use peridot::context::{PeridotContext, WasiWrapper};
#[cfg(feature = "token")]
use peridot::token::TokenBucket;
#[cfg(feature = "counter")]
use peridot_counter_ctx::PeridotCounterCtx;
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

    #[cfg(feature = "token")]
    {
        let end_thread = Arc::new(AtomicBool::new(false));
        let token_bucket = Arc::new(Mutex::new(TokenBucket::new(10000, 10000, 1)));
        let peridot_ctx = peridot_token_ctx::PeridotTokenCtx::new(wasi, token_bucket.clone());
    }
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
    #[cfg(feature = "token")]
    context::add_to_linker_async(&mut linker, |wasi_ctx: &mut WasiWrapper<PeridotTokenCtx>| wasi_ctx )?;
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

    // Run the module
    #[cfg(feature = "token")]
    {
        let handle = start_update_rate_thread(
            token_bucket,
            end_thread.clone(),
            format!("/tmp/{}_pipe", process::id()),
        );

        info!("Sleeping for {} seconds", 30 * _i);
        thread::sleep(Duration::from_secs(30 * _i as u64));

        match _overseer_address {
            Some(overseer_address) => {
                let mut client = OverseerGrpcClient::new(overseer_address);
                client.register_module(process::id(), _demand)?;
            }
            None => (),
        };
    }

    #[cfg(feature = "token")]
    {
        end_thread.store(true, Ordering::Relaxed);
        handle.join().unwrap();
    }
    let instance = linker.instantiate_async(&mut store, &module).await?;
    let start = instance.get_typed_func::<(), ()>(&mut store, "_start")?;
    start.call_async(&mut store, ()).await?;

    #[cfg(feature="s3")]
    store.data_mut().ctx.drain_uploads().await;
    Ok(())
}

#[cfg(feature = "token")]
fn start_update_rate_thread(
    token_bucket: Arc<Mutex<TokenBucket>>,
    end_thread: Arc<AtomicBool>,
    pipe_path: String,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let listener = UnixListener::bind(pipe_path).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        while !end_thread.load(Ordering::Relaxed) {
            let mut buffer = [0u8; 8]; // Buffer for f64 (8 bytes)
            match stream.read_exact(&mut buffer) {
                Ok(_) => {
                    let received_value = f64::from_le_bytes(buffer);
                    {
                        debug!("Received new rate: {}", received_value);
                        let mut token_bucket = token_bucket.lock().unwrap();
                        token_bucket.set_max_capacity(received_value as u64);
                    }
                }
                Err(_) => {
                    info!(
                        "Reached EOF in PID {}. Exiting update rate thread.",
                        process::id()
                    );
                    break;
                }
            }
            thread::sleep(Duration::from_secs(1));
        }
    })
}

#[cfg(feature = "token")]
fn communicate_updates(updated_bandwidth: &HashMap<u32, f64>, pipes: &HashMap<u32, UnixStream>) {
    for (pid, demand) in updated_bandwidth {
        let mut stream = pipes.get(&pid).unwrap();
        let bytes = demand.to_le_bytes();
        match stream.write(&bytes) {
            Ok(_) => (),
            Err(e) => warn!("Error writing to pipe: {}", e),
        };
    }
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

    let mut children = vec![];
    #[cfg(feature = "token")]
    let mut pipes: HashMap<u32, UnixStream> = HashMap::new();

    // sort config by 'priority' key
    let mut config: Vec<_> = config.into_iter().collect();
    config.sort_by(|a, b| {
        a.1.peridot_config
            .priority
            .partial_cmp(&b.1.peridot_config.priority)
            .unwrap()
    });

    for (module_path, module_config) in config.into_iter() {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(4)
                    .enable_all()
                    .build()
                    .unwrap();

                rt.block_on(async {
                    run_module(&module_path,
                               &module_config.args,
                               children.len(),
                               overseer_address,
                               module_config.peridot_config.demand, ).await.unwrap();
                });
                process::exit(0);
            }
            Ok(ForkResult::Parent { child }) => {
                #[cfg(feature = "token")]
                if client.is_some() {
                    // wait until /tmp/{pid}_pipe is created
                    while !fs::metadata(format!("/tmp/{}_pipe", child.as_raw())).is_ok() {
                        thread::sleep(Duration::from_millis(100));
                    }

                    pipes.insert(
                        child.as_raw() as u32,
                        UnixStream::connect(format!("/tmp/{}_pipe", child.as_raw()))?,
                    );
                }
                debug!("Spawned process with PID: {}", child);
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
        #[cfg(feature = "token")]
        if let Some(ref mut client) = client {
            let updated_bandwidth = client
                .update_max_bandwidth(children.iter().map(|pid| pid.as_raw() as u32).collect());
            communicate_updates(&updated_bandwidth, &pipes);
        }

        for (index, pid) in children.iter().enumerate() {
            match waitpid(Some(*pid), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, status)) => {
                    debug!("Process {} exited with status {}", pid, status);
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