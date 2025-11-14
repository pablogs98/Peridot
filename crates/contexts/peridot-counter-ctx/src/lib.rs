use async_trait::async_trait;
use peridot::context::{DelegatingWasiCtx, PeridotContext};
use std::ops::Deref;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, Size};
use wasmtime_wasi::p1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasmtime_wasi::p1::WasiP1Ctx;
use wiggle::GuestMemory;

pub struct PeridotCounterCtx {
    inner: PeridotContext,
    counter: u64
}
impl PeridotCounterCtx {
    pub fn new(inner: PeridotContext) -> Self {
        std::env::set_var("WASMTIME_LOG", "wasmtime_wasi=trace");
        Self { inner, counter: 0 }
    }

    pub fn increment_counter(&mut self, bytes: u64) {
        self.counter += bytes;
    }

    pub fn get_counter(&self) -> u64 {
        self.counter
    }
}


#[async_trait]
impl DelegatingWasiCtx for PeridotCounterCtx {
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.inner.inner()
    }
    
    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        let written_bytes = self.inner.fd_pwrite(mem, fd, iovs, offset).await?;
        self.increment_counter(written_bytes as u64);
        Ok(written_bytes)
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        let written_bytes = self.inner.fd_write(mem, fd, iovs).await?;
        self.increment_counter(written_bytes as u64);
        Ok(written_bytes)
    }
}

impl Deref for PeridotCounterCtx {
    type Target = PeridotContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Drop for PeridotCounterCtx {
    fn drop(&mut self) {
        println!("Count {}", self.get_counter());
    }
}