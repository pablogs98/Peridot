use peridot_custom_ctx::ctx::PeridotCtx;
use std::{env, fs, process, thread};
use wasi_common::sync::{Dir, WasiCtxBuilder};
use wasmtime::*;

fn run_module(module_name: &str, args: &[String]) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);

    // Set up WASI
    let wasi = WasiCtxBuilder::new()
    .inherit_stdio()
    // TODO CHANGE THIS TO THE CORRECT PATH (FROM YAML?)
    .preopened_dir(Dir::from_std_file(fs::File::open("wasm/io_test")?), ".")?
    .args(args)?
    .build();

    let peridot_ctx = PeridotCtx::new(wasi);
    peridot_custom_ctx::ctx::add_to_linker(&mut linker, |cx| cx)?;
    let mut store = Store::new(&engine, peridot_ctx);

    linker.allow_shadowing(true);

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_name)?;
    linker.module(&mut store, "", &module)?;

    // Run the module
    linker
    .get_default(&mut store, "")?
    .typed::<(), ()>(&store)?
    .call(&mut store, ())?;

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path/to/config.yaml>", &args[0]);
        process::exit(1);
    }

    let config = match peridot::new_config(&args[1]) {
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

    let mut handles = vec![];

    for (module_name, module_config) in config.into_iter() {
         let handle = thread::spawn(move || {
            if let Err(e) = run_module(&module_name, &module_config.args) {
                eprintln!("Error running module \"{}\": {}", &module_name, e);
            }
        });
        handles.push(handle);
    }



    for handle in handles {
        if let Err(e) = handle.join() {
            eprintln!("Thread panicked: {:?}", e);
        }
    }

    Ok(())
}