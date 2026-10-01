//! An out-of-tree Peridot context, built as a `cdylib` and loaded at run time.
//!
//! Demonstrates that a context needs no change to the runtime: build this crate, point
//! `plugins:` at the resulting library, and name `trace` in `contexts:`.

use async_trait::async_trait;
use peridot::context::DelegatingWasiCtx;
use peridot::plugin::{BoxedContextFuture, ContextConfig};
use serde::Deserialize;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Filesize, IovecArray, Size};
use wiggle::GuestMemory;

/// Settings for the `trace` context.
#[derive(Debug, Deserialize)]
pub struct TraceSettings {
    /// Prefix for each traced line, so several instances stay distinguishable.
    pub prefix: String,
}

impl Default for TraceSettings {
    fn default() -> Self {
        Self {
            prefix: "trace".to_string(),
        }
    }
}

pub struct TraceCtx {
    next: Box<dyn DelegatingWasiCtx>,
    prefix: String,
}

#[async_trait]
impl DelegatingWasiCtx for TraceCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        let written = self.next.fd_write(mem, fd, iovs).await?;
        eprintln!("[{}] fd_write fd={} -> {written} bytes", self.prefix, u32::from(fd));
        Ok(written)
    }

    async fn fd_read(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
    ) -> Result<Size, Error> {
        let read = self.next.fd_read(mem, fd, iovs).await?;
        eprintln!("[{}] fd_read fd={} -> {read} bytes", self.prefix, u32::from(fd));
        Ok(read)
    }

    async fn fd_seek(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: wasmtime_wasi::p1::types::Filedelta,
        whence: wasmtime_wasi::p1::types::Whence,
    ) -> Result<Filesize, Error> {
        let pos = self.next.fd_seek(mem, fd, offset, whence).await?;
        eprintln!("[{}] fd_seek fd={} -> {pos}", self.prefix, u32::from(fd));
        Ok(pos)
    }
}

/// Registry entry point.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move {
        let settings: TraceSettings = config.parse()?;
        Ok(Box::new(TraceCtx {
            next,
            prefix: settings.prefix,
        }) as Box<dyn DelegatingWasiCtx>)
    })
}

peridot::export_peridot_plugin! {
    "trace" => crate::factory,
}
