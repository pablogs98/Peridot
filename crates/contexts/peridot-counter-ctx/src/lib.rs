use async_trait::async_trait;
use peridot::context::{DelegatingWasiCtx, PeridotContext};
use peridot::metrics::MetricsProducer;
use std::collections::HashMap;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, Size};
use wasmtime_wasi::p1::WasiP1Ctx;
use wiggle::GuestMemory;

pub struct PeridotCounterCtx {
    inner: PeridotContext,
    pub counter: Arc<PeridotCounter>
}
impl PeridotCounterCtx {
    pub fn new(inner: PeridotContext, counter: Arc<PeridotCounter>) -> Self {
        std::env::set_var("WASMTIME_LOG", "wasmtime_wasi=trace");
        Self { inner, counter }
    }
    pub fn get_counter(&self) -> &Arc<PeridotCounter> {
        &self.counter
    }
}

pub struct PeridotCounter {
    counter: AtomicU64,
}

impl PeridotCounter {
    pub fn new() -> Self {
        Self {
            counter: AtomicU64::new(0),
        }
    }

    pub fn increment_counter(&self, bytes: u64) {
        self.counter.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn get_counter(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }
}

impl MetricsProducer for PeridotCounter {
    fn produce(&self, metrics: &mut HashMap<String, f64>) {
        metrics.insert("written_bytes".into(), self.get_counter() as f64);
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
        self.counter.increment_counter(written_bytes as u64);
        println!("Counter after write: {}", self.counter.get_counter());
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
        self.counter.increment_counter(written_bytes as u64);
        println!("Counter after write: {}", self.counter.get_counter());
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
            println!("Count {}", self.counter.get_counter()); 
    }
}