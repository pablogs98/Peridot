use peridot::token::TokenBucket;
use peridot_overseer_grpc::client::OverseerGrpcClient;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::{env, fs, process, thread};
use wasi_common::sync::{Dir, WasiCtxBuilder};
use wasmtime::*;
use log::error;
#[cfg(unix)]
use nix::unistd::{fork, ForkResult};
#[cfg(unix)]
use nix::sys::wait::waitpid;
#[cfg(windows)]
use std::process::Command;

fn run_module(module_name: &str, overseer_address: &str, args: &[String]) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    let end_thread = Arc::new(AtomicBool::new(false));

    // Set up WASI
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        // TODO CHANGE THIS TO THE CORRECT PATH (FROM YAML?)
        .preopened_dir(Dir::from_std_file(fs::File::open("wasm/io_test")?), ".")?
        .args(args)?
        .build();

    let peridot_ctx = peridot_custom_ctx::clock_ctx::PeridotClockCtx::new(wasi);
    peridot_custom_ctx::clock_ctx::add_to_linker(&mut linker, |cx| cx)?;
    let mut store = Store::new(&engine, peridot_ctx);

    linker.allow_shadowing(true);

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_name)?;
    linker.module(&mut store, "", &module)?;

    // Run the module
    let token_bucket = Arc::new(TokenBucket::new(1024, 1024, 1));
    let mut client: Arc<OverseerGrpcClient> = Arc::new(OverseerGrpcClient::new(overseer_address));
    client.register_module(process::id());
    let handle = start_update_rate_thread(token_bucket.clone(), end_thread.clone(), client.clone());

    linker
        .get_default(&mut store, "")?
        .typed::<(), ()>(&store)?
        .call(&mut store, ())?;

    end_thread.store(false, Ordering::Relaxed);
    handle.join();
    client.remove_module(process::id());

    Ok(())
}

fn start_update_rate_thread(
    token_bucket: Arc<TokenBucket>,
    end_thread: Arc<AtomicBool>,
    client: Arc<OverseerGrpcClient>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !*end_thread.load(Ordering::Relaxed) {
            let new_refill_freq = 0;
            //  todo: grpc stuff
            token_bucket.refill_freq = new_refill_freq;
            thread::sleep(std::time::Duration::from_secs(1));
        }
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::var_os("RUST_LOG").is_none() {
        env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!(
            "Usage: {} <path/to/config.yaml> <overseer_address>",
            &args[0]
        );
        process::exit(1);
    }

    let overseer_address = &args[2];

    let config = match peridot::conf::new_config(&args[1]) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error reading \"{}\": {}", &args[1], e);
            process::exit(1);
        }
    };

    if config.is_empty() {
        eprintln!("No configurations found in \"{}\".", &args[1]);
        process::exit(1);
    }

    // print the PID of the main process
    println!("MAIN");
    println!("PID: {}", process::id());

    let mut children = vec![];

    for (module_name, module_config) in config.into_iter() {
        #[cfg(unix)]
        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                if let Err(e) = run_module(&module_name, overseer_address, &module_config.args) {
                    error!("Error running module \"{}\": {}", &module_name, e);
                }
                process::exit(0);
            }
            Ok(ForkResult::Parent { child }) => {
                println!("Spawned process with PID: {}", child);
                children.push(child);
            }
            Err(e) => {
                eprintln!("Fork failed: {}", e);
            }
        }

        #[cfg(windows)]
        match Command::new(env::current_exe()?)
            .arg(&module_name)
            .args(&module_config.args)
            .spawn()
        {
            Ok(child) => {
                println!("Spawned process with PID: {}", child.id());
                children.push(child);
            }
            Err(e) => {
                error!("Failed to spawn process: {}", e);
            }
        }
    }

    #[cfg(unix)]
    for child in children {
        let _ = waitpid(child, None);
    }

    #[cfg(windows)]
    for mut child in children {
        let _ = child.wait();
    }

    Ok(())
}
