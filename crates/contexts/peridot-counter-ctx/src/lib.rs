use async_trait::async_trait;
use log::debug;
use peridot::context::DelegatingWasiCtx;
use peridot::plugin::{BoxedContextFuture, ContextConfig};
use peridot::metrics::MetricsProducer;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, Size};
use wiggle::GuestMemory;

/// Counts the bytes the guest writes, and reports the running total as a metric.
///
/// This is the reference implementation of a Peridot context: it holds the next link in the
/// chain, overrides the two hostcalls it cares about, and lets the [`DelegatingWasiCtx`]
/// defaults forward the other forty-odd untouched.
pub struct PeridotCounterCtx {
    next: Box<dyn DelegatingWasiCtx>,
    counter: Arc<PeridotCounter>,
}

impl PeridotCounterCtx {
    pub fn new(next: Box<dyn DelegatingWasiCtx>, counter: Arc<PeridotCounter>) -> Self {
        Self { next, counter }
    }

    pub fn get_counter(&self) -> &Arc<PeridotCounter> {
        &self.counter
    }
}

/// Registry entry point. Registered under the name `counter`.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, _config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move {
        Ok(Box::new(PeridotCounterCtx::new(next, Arc::new(PeridotCounter::new())))
            as Box<dyn DelegatingWasiCtx>)
    })
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

impl Default for PeridotCounter {
    fn default() -> Self {
        Self::new()
    }
}

impl MetricsProducer for PeridotCounter {
    fn produce(&self, metrics: &mut HashMap<String, f64>) {
        metrics.insert("written_bytes".into(), self.get_counter() as f64);
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotCounterCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    fn metrics_producers(&self) -> Vec<Arc<dyn MetricsProducer + Send + Sync>> {
        vec![self.counter.clone() as Arc<dyn MetricsProducer + Send + Sync>]
    }

    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        let written_bytes = self.next.fd_pwrite(mem, fd, iovs, offset).await?;
        self.counter.increment_counter(written_bytes as u64);
        debug!(
            "fd_pwrite wrote {} bytes, total {}",
            written_bytes,
            self.counter.get_counter()
        );
        Ok(written_bytes)
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        let written_bytes = self.next.fd_write(mem, fd, iovs).await?;
        self.counter.increment_counter(written_bytes as u64);
        debug!(
            "fd_write wrote {} bytes, total {}",
            written_bytes,
            self.counter.get_counter()
        );
        Ok(written_bytes)
    }
}

impl Drop for PeridotCounterCtx {
    fn drop(&mut self) {
        log::info!("Total bytes written: {}", self.counter.get_counter());
    }
}
