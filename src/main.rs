use std::{env, process};
use peridot::PeridotConfig;
use wasmtime::*;
use wasi_common::sync::WasiCtxBuilder;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: {} <path/to/config.yaml>", args[0]);
        process::exit(1);
    }

    let config = match PeridotConfig::new(&args[1]) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Error reading \"{}\": {}", &args[1], e);
            process::exit(1);
        }
    };

    if config.configs.is_empty() {
        eprintln!("No configurations found in \"{}\".", &args[1]);
        process::exit(1);
    }

    let engine = Engine::default();
    let mut linker = Linker::new(&engine);
    wasi_common::sync::add_to_linker(&mut linker, |s| s)?;

    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()?
        .build();
    let mut store = Store::new(&engine, wasi);

    let module = Module::from_file(&engine, config.configs.keys().next().unwrap().to_string())?;
    linker.module(&mut store, "", &module)?;
    linker
        .get_default(&mut store, "")?
        .typed::<(), ()>(&store)?
        .call(&mut store, ())?;

    Ok(())
}