use std::collections::HashMap;
use async_trait::async_trait;
use peridot::context::{DelegatingWasiCtx, PeridotContext};
use std::ops::Deref;
use std::sync::{Arc, Mutex};
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, Size};
use wasmtime_wasi::p1::WasiP1Ctx;
use wiggle::GuestMemory;
use peridot::counter::PeridotCounter;

pub struct PeridotCounterCtx {
    inner: PeridotContext,
    pub counter: Arc<Mutex<PeridotCounter>>
}
impl PeridotCounterCtx {
    pub fn new(inner: PeridotContext, counter: Arc<Mutex<PeridotCounter>>) -> Self {
        std::env::set_var("WASMTIME_LOG", "wasmtime_wasi=trace");
        Self { inner, counter }
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
        println!("Written bytes: {}", written_bytes);
        self.counter.lock().unwrap().increment_counter(written_bytes as u64);
        println!("Counter after write: {}", self.counter.lock().unwrap().get_counter());
        Ok(written_bytes)
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        let written_bytes = self.inner.fd_write(mem, fd, iovs).await?;
        println!("Written bytes: {}", written_bytes);
        self.counter.lock().unwrap().increment_counter(written_bytes as u64);
        println!("Counter after write: {}", self.counter.lock().unwrap().get_counter());
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
            println!("Count {}", self.counter.lock().unwrap().get_counter());}
}