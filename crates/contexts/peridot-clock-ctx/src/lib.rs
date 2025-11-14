use std::ops::Deref;
use std::process::{Command, Stdio};
use std::time::Duration;
use async_trait::async_trait;
use log::info;
use wasmtime_wasi::p1::types::{Error, Advice, CiovecArray, Clockid, Dircookie, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta, Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights, Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence, Errno};
use wasmtime_wasi::p1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasmtime_wasi::p1::WasiP1Ctx;
use wiggle::{GuestMemory, GuestPtr};
use peridot::context::{DelegatingWasiCtx, PeridotContext};


pub struct PeridotClockCtx {
    inner: PeridotContext,
    clock: Duration,
    logs: Vec<String>,
}

impl PeridotClockCtx {
    pub fn new(inner: PeridotContext) -> Self {
        Self { inner, clock: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap(), logs: Vec::new() }
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
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.inner.inner()
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
        let res = self.inner.fd_datasync(mem, fd).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_datasync,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }

    async fn fd_pread(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.inner.fd_pread(mem, fd, iovs, offset).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_pread,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }
    
    async fn fd_pwrite(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res= self.inner.fd_pwrite(mem, fd, iovs, offset).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_pwrite,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }

    async fn fd_read(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.inner.fd_read(mem, fd, iovs).await;
        self.update_clock();
        let end_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        self.add_log(format!("fd_read,{}", end_clock.as_nanos() - start_clock.as_nanos()));
        res
    }
    
    async fn fd_write(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray) -> Result<Size, Error> {
        self.drop_cache();
        let start_clock = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap();
        let res = self.inner.fd_write(mem, fd, iovs).await;
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

impl Deref for PeridotClockCtx {
    type Target = PeridotContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}