use log::{debug, error, info, warn};
use nix::sys::wait::waitpid;
use nix::sys::wait::{WaitPidFlag, WaitStatus};
use nix::unistd::{fork, ForkResult};
use peridot::token::TokenBucket;
use peridot_overseer_grpc::client::OverseerGrpcClient;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use std::{env, fs, process, thread};
use std::path::Path;
use wasi_common::sync::{Dir, WasiCtxBuilder};
use wasmtime::*;

use clap::Parser;

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

fn run_module(module_path: &str, args: &[String], i: usize, overseer_address: &Option<String>, demand: f64) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    let end_thread = Arc::new(AtomicBool::new(false));

    info!("Args {:?}", args);

    // Set up WASI
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .preopened_dir(Dir::from_std_file(
            fs::File::open(Path::new(module_path).parent().unwrap())?), Path::new(module_path).parent().unwrap())?
        .args(args)?
        .build();

    let token_bucket = Arc::new(Mutex::new(TokenBucket::new(10000, 10000, 1)));
    let peridot_ctx =
        peridot_custom_ctx::token_ctx::PeridotTokenCtx::new(wasi, token_bucket.clone());
    peridot_custom_ctx::token_ctx::add_to_linker(&mut linker, |cx| cx)?;

    //let peridot_ctx = peridot_custom_ctx::clock_ctx::PeridotClockCtx::new(wasi);
    //peridot_custom_ctx::clock_ctx::add_to_linker(&mut linker, |cx| cx)?;
    let mut store = Store::new(&engine, peridot_ctx);

    linker.allow_shadowing(true);

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_path)?;
    linker.module(&mut store, "", &module)?;

    // Run the module
    let handle = start_update_rate_thread(
        token_bucket,
        end_thread.clone(),
        format!("/tmp/{}_pipe", process::id()),
    );

    info!("Sleeping for {} seconds", 30 * i);
    thread::sleep(Duration::from_secs(30 * i as u64));

    match overseer_address {
        Some(overseer_address) => {
            let mut client = OverseerGrpcClient::new(overseer_address);
            client.register_module(process::id(), demand)?;
        }
        None => (),
    };

    linker
        .get_default(&mut store, "")?
        .typed::<(), ()>(&store)?
        .call(&mut store, ())?;

    end_thread.store(true, Ordering::Relaxed);
    handle.join().unwrap();
    Ok(())
}

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

fn communicate_updates(updated_bandwidth: &HashMap<u32, f64>, pipes: &HashMap<u32, UnixStream>) {
    for (pid, demand) in updated_bandwidth {
        let mut stream = pipes.get(&pid).unwrap();
        let bytes = demand.to_le_bytes();
        match stream.write(&bytes) {
            Ok(_) => (),
            Err(e) => warn!("Error writing to pipe: {}", e)
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
    let mut pipes: HashMap<u32, UnixStream> = HashMap::new();
    let mut client = None;
    if let Some(ref address) = overseer_address {
        client = Some(OverseerGrpcClient::new(address));
    }

    // sort config by 'priority' key
    let mut config: Vec<_> = config.into_iter().collect();
    config.sort_by(|a, b| a.1.peridot_config.get("priority").unwrap().partial_cmp(b.1.peridot_config.get("priority").unwrap()).unwrap());

    // todo: what happens if the parent process is killed? Use UNIX process groups and signal handling
    for (module_path, module_config) in config.into_iter() {
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                if let Err(e) = run_module(&module_path, &module_config.args, children.len(),overseer_address, *module_config.peridot_config.get("demand").unwrap(),) {
                    eprintln!("Error running module \"{}\": {}", &module_path, e);
                    process::exit(1);
                }
                process::exit(0);
            }
            Ok(ForkResult::Parent { child }) => {
                if client.is_some() {
                    // wait until /tmp/{pid}_pipe is created
                    while !fs::metadata(format!("/tmp/{}_pipe", child.as_raw())).is_ok() { thread::sleep(Duration::from_millis(100)); }
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
        if let Some(ref mut client) = client {
            let updated_bandwidth = client
                .update_max_bandwidth(children.iter().map(|pid| pid.as_raw() as u32).collect());
            communicate_updates(&updated_bandwidth, &pipes);
        }

        for (index, pid) in children.iter().enumerate() {
            match waitpid(Some(*pid), Some(WaitPidFlag::WNOHANG)) {
                Ok(WaitStatus::Exited(_, status)) => {
                    if let Some(ref mut client) = client {
                        client.remove_module(pid.as_raw() as u32)?;
                    }
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
