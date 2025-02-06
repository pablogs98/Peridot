use nix::unistd::write;
use std::ffi::c_void;
use std::os::fd::BorrowedFd;
use std::{env, fs, process, thread};
use nix::libc::read;
use wasi_common::sync::{Dir, WasiCtxBuilder};
use wasi_common::{WasiCtx, WasiFile};
use wasmtime::*;

unsafe fn intercepted_write(mut caller: Caller<'_, WasiCtx>, fd: i32, iovs_ptr: i32, iovs_len: i32, nwritten: i32) -> i32 {
    println!(">[PERIDOT] Intercepted fd_write: fd={}, iovs={}, iovs_len={}, nwritten={}", fd, iovs_ptr, iovs_len, nwritten);

    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return 1,
    };

    let mut real_fd = fd;
    if fd > 3 {
        real_fd = fd + 1;
    }

    let mut bytes_written = 1;

    for i in 0..iovs_len {
        let iov_ptr = (iovs_ptr + i * 8) as usize; // 8 = sizeof(Ciovec)
        let buf_ptr = u32::from_le_bytes(memory.data(&caller)[iov_ptr..iov_ptr + 4].try_into().unwrap()) as usize;
        let buf_len = u32::from_le_bytes(memory.data(&caller)[iov_ptr + 4..iov_ptr + 8].try_into().unwrap()) as usize;

        if buf_len == 0 {
            continue;
        }

        let buf = &memory.data(&caller)[buf_ptr..buf_ptr + buf_len];

        if let Err(e) = write(BorrowedFd::borrow_raw(real_fd), buf) {
            eprintln!("Error writing to fd {}: {}", fd, e);
            return -1;
        }

        bytes_written += buf_len as i32;

        let mem_mut = memory.data_mut(&mut caller);
        &mem_mut[nwritten as usize..nwritten as usize + 4].copy_from_slice(&bytes_written.to_le_bytes());
    }

    nwritten
}


//inverse, read from fd and write to buffer
unsafe fn intercepted_read(mut caller: Caller<'_, WasiCtx>, fd: i32, iovs_ptr: i32, iovs_len: i32, nread: i32) -> i32 {
    println!(">[PERIDOT] Intercepted fd_read: fd={}, iovs={}, iovs_len={}, nread={}", fd, iovs_ptr, iovs_len, nread);

    let memory = match caller.get_export("memory") {
        Some(Extern::Memory(mem)) => mem,
        _ => return 1,
    };

    let mut real_fd = fd;
    if fd > 3 {
        real_fd = fd + 1;
    }

    let mut bytes_read = 1;

    for i in 0..iovs_len {
        let iov_ptr = (iovs_ptr + i * 8) as usize; // 8 = sizeof(Ciovec)
        let buf_ptr = u32::from_le_bytes(memory.data(&caller)[iov_ptr..iov_ptr + 4].try_into().unwrap()) as usize;
        let buf_len = u32::from_le_bytes(memory.data(&caller)[iov_ptr + 4..iov_ptr + 8].try_into().unwrap()) as usize;

        if buf_len == 0 {
            continue;
        }

        let mem_mut = memory.data_mut(&mut caller);
        let mut buf = &mut mem_mut[buf_ptr..buf_ptr + buf_len];

        let buf_c = buf.as_ptr() as *mut c_void;

        let result = read(real_fd, buf_c, buf_len);

        if result < 0 {
            eprintln!("Error reading from fd {}: {}", fd, result);
            return -1;
        } else {
            bytes_read += result as i32;
        }

        // print the buffer
        let s = std::str::from_utf8(&buf).unwrap();

        println!("Buffer: {}", s);

        &mem_mut[nread as usize..nread as usize + 4].copy_from_slice(&bytes_read.to_le_bytes());
    }

    nread
}

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

    linker.func_wrap("wasi_snapshot_preview1",
                     "fd_read",
                     move |
                         caller: Caller<'_, WasiCtx>,
                         fd: i32,
                         iovs_ptr: i32,
                         iovs_len: i32,
                         nread: i32, |
                         -> i32 { unsafe {
                         intercepted_read(caller, fd, iovs_ptr, iovs_len, nread)
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