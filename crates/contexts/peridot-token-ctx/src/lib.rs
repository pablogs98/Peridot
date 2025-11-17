use log::{debug};
use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use peridot::token::TokenBucket;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, IovecArray, Size};
use wasmtime_wasi::p1::{WasiP1Ctx};
use wiggle::{GuestMemory};
use peridot::context::{DelegatingWasiCtx, PeridotContext};

pub struct PeridotTokenCtx {
    inner: PeridotContext,
    bucket: Arc<Mutex<TokenBucket>>
}

impl PeridotTokenCtx {
    pub fn new(inner: PeridotContext, bucket: Arc<Mutex<TokenBucket>>) -> Self {
        bucket.lock().unwrap().start_refill_thread();
        Self { inner, bucket }
    }

    pub fn get_bucket(&self) -> &Arc<Mutex<TokenBucket>> {
        &self.bucket
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotTokenCtx {
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.inner.inner()
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
        self.inner.fd_pread(mem, fd, iovs, offset).await
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
        self.inner.fd_pwrite(mem, fd, iovs, offset).await
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

        self.inner.fd_read(mem, fd, iovs).await
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
        self.inner.fd_write(mem, fd, iovs).await
    }
}