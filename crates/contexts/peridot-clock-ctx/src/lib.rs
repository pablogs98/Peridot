use std::process::{Command, Stdio};
use std::time::Duration;
use async_trait::async_trait;
use log::info;
use wasmtime_wasi::p1::types::{Errno, Error, CiovecArray, Clockid, Fd, Filesize, IovecArray, Size, Timestamp};
use wiggle::GuestMemory;
use peridot::context::DelegatingWasiCtx;
use peridot::plugin::{BoxedContextFuture, ContextConfig};


pub struct PeridotClockCtx {
    next: Box<dyn DelegatingWasiCtx>,
    clock: Duration,
    logs: Vec<String>,
}

/// Registry entry point. Registered under the name `clock`.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, _config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move { Ok(Box::new(PeridotClockCtx::new(next)) as Box<dyn DelegatingWasiCtx>) })
}

impl PeridotClockCtx {
    pub fn new(next: Box<dyn DelegatingWasiCtx>) -> Self {
        Self { next, clock: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap(), logs: Vec::new() }
    }

    pub fn drop_cache(&mut self) {
        let echo_process = Command::new("sudo")
            .arg("echo")
            .arg("3")
            .stdout(Stdio::piped())
            .spawn()
            .expect("Failed to start echo process");

        let mut tee_process = Command::new("sudo")
            .arg("tee")
            .arg("/proc/sys/vm/drop_caches")
            .stdin(echo_process.stdout.unwrap()) // Pasamos la salida de echo como entrada para tee
            .spawn()
            .expect("Failed to start tee process");

        let _ = tee_process.wait().expect("Failed to wait for tee process");
    }

    fn elapsed(&self) -> u128 {
        self.clock.as_nanos()
    }

    fn update_clock(&mut self) {
        self.clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
    }

    pub fn add_log(&mut self, log: String) {
        self.logs.push(log);
        if self.logs.len() % 100 >= 99 {
            info!("Writing logs to file");
            self.to_file("logs_default.txt").unwrap();
        }
    }

    pub fn to_file(&self, path: &str) -> std::io::Result<()> {
        std::fs::write(path, self.logs.join("\n"))
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotClockCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }
    
    fn clock_time_get(&mut self, _mem: &mut GuestMemory<'_>, id: Clockid, _precision: Timestamp) -> Result<Timestamp, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        match id {
            Clockid::Realtime => {
                let now = self.elapsed();
                let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
                self.add_log(format!("clock_time_get,{}", end_clock.as_nanos() - start_clock.as_nanos()));
                Ok(Timestamp::from(now as u64))
            }
            Clockid::Monotonic => {
                let now = self.elapsed();
                let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
                self.add_log(format!("clock_time_get,{}", end_clock.as_nanos() - start_clock.as_nanos()));
                Ok(Timestamp::from(now as u64))
            }
            Clockid::ProcessCputimeId | Clockid::ThreadCputimeId => {
                Err(Error::from(Errno::Noent))
            }
        }
        /*let res = self.inner.clock_time_get(mem, id, precision).await;
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("clock_time_get,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
        */
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.next.fd_datasync(mem, fd).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_datasync,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }

    async fn fd_pread(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.next.fd_pread(mem, fd, iovs, offset).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_pread,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }
    
    async fn fd_pwrite(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res= self.next.fd_pwrite(mem, fd, iovs, offset).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_pwrite,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }

    async fn fd_read(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.next.fd_read(mem, fd, iovs).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_read,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }
    
    async fn fd_write(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.next.fd_write(mem, fd, iovs).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        unsafe {
            if fd.inner() > 3 {
                self.add_log(format!("fd_write,{}", end_clock.as_nanos() - start_clock.as_nanos()));
            }
        }
        res
    }
}
