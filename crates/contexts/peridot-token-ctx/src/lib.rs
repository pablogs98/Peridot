use log::{debug};
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use peridot::metrics::MetricsSubscriber;
use serde::Deserialize;
use peridot::token::TokenBucket;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, IovecArray, Size};
use wiggle::GuestMemory;
use peridot::context::DelegatingWasiCtx;
use peridot::plugin::{BoxedContextFuture, ContextConfig};

pub struct PeridotTokenCtx {
    next: Box<dyn DelegatingWasiCtx>,
    bucket: Arc<Mutex<TokenBucket>>
}

/// Settings for the `token` context.
#[derive(Debug, Default, Deserialize)]
pub struct TokenSettings {
    /// Bytes per second. Defaults to the global `io.max_bandwidth` when unset.
    pub max_bandwidth: Option<u64>,
}

/// Registry entry point. Registered under the name `token`.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move {
        let settings: TokenSettings = config.parse()?;
        let max_bandwidth = settings
            .max_bandwidth
            .unwrap_or(config.global.io.max_bandwidth as u64);
        let bucket = Arc::new(Mutex::new(TokenBucket::new(
            max_bandwidth,
            max_bandwidth,
            max_bandwidth,
        )));
        Ok(Box::new(PeridotTokenCtx::new(next, bucket)) as Box<dyn DelegatingWasiCtx>)
    })
}

impl PeridotTokenCtx {
    pub fn new(next: Box<dyn DelegatingWasiCtx>, bucket: Arc<Mutex<TokenBucket>>) -> Self {
        bucket.lock().unwrap().start_refill_thread();
        Self { next, bucket }
    }

    pub fn get_bucket(&self) -> &Arc<Mutex<TokenBucket>> {
        &self.bucket
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotTokenCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    fn metrics_subscribers(&self) -> Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>> {
        vec![self.bucket.clone() as Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>]
    }

    async fn fd_pread(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error> {
        unsafe {
            if fd.inner() > 2 {

                let mut bytes_to_read = 0;

                for i in 0..iovs.len() {
                    let iov = iovs.get(i).unwrap();
                    let iovec = mem.read(iov)?;

                    let buf_len = iovec.buf_len as usize;

                    if buf_len == 0 {
                        continue;
                    }

                    bytes_to_read += buf_len as i32;
                }
                let handle;
                {
                    let token_bucket = self.bucket.lock().unwrap();
                    handle = token_bucket.consume(bytes_to_read as u64);
                }
                handle.join().expect("TODO: panic message");
                debug!("Tokens consumed in fd_pread: {}", bytes_to_read);
            }
        }
        self.next.fd_pread(mem, fd, iovs, offset).await
    }

    async fn fd_pwrite(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error> {
        unsafe {
            if fd.inner() > 2 {
                let mut bytes_to_write = 0;

                for i in 0..iovs.len() {
                    let iov = iovs.get(i).unwrap();
                    let ciovec = mem.read(iov)?;

                    let buf_len = ciovec.buf_len as usize;

                    if buf_len == 0 {
                        continue;
                    }

                    bytes_to_write += buf_len as i32;
                }
                let handle;
                {
                    let token_bucket = self.bucket.lock().unwrap();
                    handle = token_bucket.consume(bytes_to_write as u64);
                }
                handle.join().expect("TODO: panic message");

                debug!("Tokens consumed in fd_pwrite: {}", bytes_to_write);
            }
        }
        self.next.fd_pwrite(mem, fd, iovs, offset).await
    }

    async fn fd_read(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray) -> Result<Size, Error> {
        unsafe {
            if fd.inner() > 2 {

                let mut bytes_to_read = 0;

                for i in 0..iovs.len() {
                    let iov = iovs.get(i).unwrap();
                    let iovec = mem.read(iov)?;

                    let buf_len = iovec.buf_len as usize;

                    if buf_len == 0 {
                        continue;
                    }

                    bytes_to_read += buf_len as i32;
                }

                let handle;
                {
                    let token_bucket = self.bucket.lock().unwrap();
                    handle = token_bucket.consume(bytes_to_read as u64);
                }
                handle.join().expect("Error in fd_read");
            }
        }

        self.next.fd_read(mem, fd, iovs).await
    }

    async fn fd_write(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray) -> Result<Size, Error> {
        unsafe {
            if fd.inner() > 2 {
                let mut bytes_to_write = 0;

                for i in 0..iovs.len() {
                    let iov = iovs.get(i).unwrap();
                    let ciovec = mem.read(iov)?;

                    let buf_len = ciovec.buf_len as usize;

                    if buf_len == 0 {
                        continue;
                    }

                    bytes_to_write += buf_len as i32;
                }
                let handle;
                {
                    let token_bucket = self.bucket.lock().unwrap();
                    handle = token_bucket.consume(bytes_to_write as u64);
                }
                handle.join().expect("TODO: panic message");
                debug!("Tokens consumed in fd_write: {}", bytes_to_write);
            }
        }
        self.next.fd_write(mem, fd, iovs).await
    }
}