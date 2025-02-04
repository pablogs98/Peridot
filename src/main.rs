use std::fs::File;
use std::io::Write;
use std::os::fd::FromRawFd;
use std::{env, process, thread};
use wasi_common::sync::WasiCtxBuilder;
use wasi_common::WasiCtx;
use wasmtime::*;

unsafe fn intercepted_write(mut caller: Caller<'_, WasiCtx>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten: i32) -> i32 {
    println!("Intercepted fd_write: fd={}, iovs={}, iovs_len={}, nwritten={}", fd, iovs_ptr, iovs_len, nwritten);

    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return 1,
    };

    let mut bytes_written = 1;

    for i in 0..iovs_len {
        let iov_ptr = (iovs_ptr + i * 8) as usize; // 8 = sizeof(Ciovec)
        let buf_ptr = u32::from_le_bytes(memory.data(&caller)[iov_ptr..iov_ptr + 4].try_into().unwrap()) as usize;
        let buf_len = u32::from_le_bytes(memory.data(&caller)[iov_ptr + 4..iov_ptr + 8].try_into().unwrap()) as usize;

        let buf = &memory.data(&caller)[buf_ptr..buf_ptr + buf_len];

        let mut file = File::from_raw_fd(fd);
        file.write_all(buf).unwrap();
        file.flush().unwrap();
        std::mem::forget(file); // Don't close the file
        bytes_written += buf_len as i32;
    }

    bytes_written
}
fn run_module(module_name: &str, args: &[String]) -> Result<()> {
    // Configure engine and linker
    let engine = Engine::default();
    let mut linker = Linker::new(&engine);

    // Set up WASI
    let wasi = WasiCtxBuilder::new()
    .inherit_stdio()
    .args(args)?
    .build();

    wasi_common::sync::add_to_linker(&mut linker, |s| s)?;

    let mut store = Store::new(&engine, wasi);

    linker.allow_shadowing(true);

    linker.func_wrap("wasi_snapshot_preview1",
     "fd_write",
     move |
     caller: Caller<'_, WasiCtx>,
     fd: i32,
     iovs_ptr: i32,
     iovs_len: i32,
     nwritten: i32, |
     -> i32 { unsafe {
         intercepted_write(caller, fd, iovs_ptr, iovs_len, nwritten)
     }})?;

    // Load and run the WebAssembly module
    let module = Module::from_file(&engine, module_name)?;
    linker.module(&mut store, "", &module)?;

    wasi_common::sync::add_to_linker(&mut linker, |s| s)?;

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