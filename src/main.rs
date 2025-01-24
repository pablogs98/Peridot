use std::{env, process, thread};
use wasmtime::*;
use wasi_common::sync::WasiCtxBuilder;

fn run_module(module_name: &str, args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);

    wasi_common::sync::add_to_linker(&mut linker, |s| s)?;

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .args(args)?
        .build();

    let mut store = Store::new(&engine, wasi);

    let module = Module::from_file(&engine, module_name)?;
    linker.module(&mut store, "", &module)?;
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