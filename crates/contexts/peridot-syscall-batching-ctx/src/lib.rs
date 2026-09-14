//! Coalesces many small guest writes into fewer host writes.
//!
//! Guest `fd_write` calls are buffered and flushed once `num_writes` have accumulated for a
//! file, turning N small `write(2)` syscalls into one.
//!
//! To do that the context has to own the file. It cannot borrow the one wasmtime opened: the p1
//! descriptor table is private to `wasmtime-wasi`, so the underlying handle is unreachable.
//! Instead this intercepts `path_open` for write-only files, opens them itself with [`std::fs`],
//! and serves the write-path hostcalls from its own descriptor table — the same shape
//! `peridot-s3-ctx` and `peridot-geds-ctx` use for their remote objects.
//!
//! Files opened for reading are not intercepted; they pass straight down the chain.

use async_trait::async_trait;
use log::debug;
use peridot::context::DelegatingWasiCtx;
use peridot::memory;
use peridot::plugin::{BoxedContextFuture, ContextConfig, Preopen};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use wasmtime_wasi::p1::types::{
    CiovecArray, Errno, Error, Fd, Fdflags, Filedelta, Filesize, Lookupflags, Oflags, Rights, Size,
    Whence,
};
use wiggle::{GuestMemory, GuestPtr};

/// Shadow descriptors start well above anything wasmtime hands out, so they cannot collide with
/// a real fd (or with stdio).
const SHADOW_FD_BASE: u32 = 1 << 30;

/// Settings for the `syscall-batching` context.
#[derive(Debug, Deserialize)]
pub struct SyscallBatchingSettings {
    /// Buffered writes per file before a flush. `1` disables batching.
    pub num_writes: usize,
}

impl Default for SyscallBatchingSettings {
    fn default() -> Self {
        Self { num_writes: 1 }
    }
}

struct BatchedFile {
    file: File,
    buf: Vec<u8>,
    /// Writes buffered since the last flush.
    pending: usize,
}

impl BatchedFile {
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.buf.is_empty() {
            self.file.write_all(&self.buf)?;
            self.buf.clear();
        }
        self.pending = 0;
        Ok(())
    }
}

pub struct PeridotSyscallBatchingCtx {
    next: Box<dyn DelegatingWasiCtx>,
    num_writes: usize,
    files: BTreeMap<u32, BatchedFile>,
    next_fd: u32,
    preopens: Vec<Preopen>,
}

/// Registry entry point. Registered under the name `syscall-batching`.
pub fn factory(
    next: Box<dyn DelegatingWasiCtx>,
    config: ContextConfig<'_>,
) -> BoxedContextFuture<'_> {
    Box::pin(async move {
        let settings: SyscallBatchingSettings = config.parse()?;
        Ok(
            Box::new(PeridotSyscallBatchingCtx::new(next, settings.num_writes, config))
                as Box<dyn DelegatingWasiCtx>,
        )
    })
}

impl PeridotSyscallBatchingCtx {
    pub fn new(
        next: Box<dyn DelegatingWasiCtx>,
        num_writes: usize,
        config: ContextConfig<'_>,
    ) -> Self {
        Self {
            next,
            num_writes,
            files: BTreeMap::new(),
            next_fd: SHADOW_FD_BASE,
            preopens: config.preopens.to_vec(),
        }
    }

    fn owns(&self, fd: Fd) -> bool {
        self.files.contains_key(&u32::from(fd))
    }

    /// Buffers `bytes` for `fd`, flushing once `num_writes` have accumulated.
    fn buffer(&mut self, fd: Fd, bytes: &[u8]) -> Result<Size, Error> {
        let num_writes = self.num_writes;
        let file = self.files.get_mut(&u32::from(fd)).ok_or(Errno::Badf)?;
        file.buf.extend_from_slice(bytes);
        file.pending += 1;
        if file.pending >= num_writes {
            file.flush().map_err(|_| Errno::Io)?;
            debug!("flushed {num_writes} buffered write(s) for fd {}", u32::from(fd));
        }
        Ok(u32::try_from(bytes.len())?)
    }

    /// Resolves a guest path against the runtime's preopens.
    fn host_path(&self, guest_path: &str) -> Option<std::path::PathBuf> {
        let path = std::path::Path::new(guest_path);
        if path.is_absolute() {
            return Some(path.to_path_buf());
        }
        let mut candidates: Vec<&Preopen> = self.preopens.iter().collect();
        candidates.sort_by_key(|p| std::cmp::Reverse(p.guest.len()));
        for preopen in &candidates {
            if let Ok(rest) = path.strip_prefix(&preopen.guest) {
                return Some(preopen.host.join(rest));
            }
        }
        self.preopens
            .first()
            .map(|p| p.host.join(path.strip_prefix("./").unwrap_or(path)))
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotSyscallBatchingCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    /// Flush anything still buffered before the process exits.
    async fn shutdown(&mut self) {
        for (fd, file) in self.files.iter_mut() {
            if let Err(e) = file.flush() {
                log::error!("failed to flush fd {fd} on shutdown: {e}");
            }
        }
        self.next.shutdown().await;
    }

    async fn path_open(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        dirflags: Lookupflags,
        path: GuestPtr<str>,
        oflags: Oflags,
        fs_rights_base: Rights,
        fs_rights_inheriting: Rights,
        fdflags: Fdflags,
    ) -> Result<Fd, Error> {
        // Only take over write-only files; anything readable keeps wasmtime's own descriptor.
        let write_only =
            fs_rights_base.contains(Rights::FD_WRITE) && !fs_rights_base.contains(Rights::FD_READ);

        if self.num_writes > 1 && write_only {
            let guest_path = memory::read_path(mem, path)?;
            if let Some(host_path) = self.host_path(&guest_path) {
                let file = OpenOptions::new()
                    .write(true)
                    .create(oflags.contains(Oflags::CREAT))
                    .truncate(oflags.contains(Oflags::TRUNC))
                    .open(&host_path)
                    .map_err(|_| Error::from(Errno::Noent))?;

                let shadow = self.next_fd;
                self.next_fd += 1;
                self.files.insert(
                    shadow,
                    BatchedFile {
                        file,
                        buf: Vec::new(),
                        pending: 0,
                    },
                );
                debug!("batching writes to {} as fd {shadow}", host_path.display());
                return Ok(shadow.into());
            }
        }

        self.next
            .path_open(
                mem,
                fd,
                dirflags,
                path,
                oflags,
                fs_rights_base,
                fs_rights_inheriting,
                fdflags,
            )
            .await
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        if self.owns(fd) {
            // Buffered until the batch is full, so the bytes must be owned.
            let bytes = memory::payload(mem, iovs)?.into_owned();
            return self.buffer(fd, &bytes);
        }
        self.next.fd_write(mem, fd, iovs).await
    }

    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        if self.owns(fd) {
            // A positioned write cannot be reordered behind buffered data, so flush and seek.
            let bytes = memory::payload(mem, iovs)?.into_owned();
            let file = self.files.get_mut(&u32::from(fd)).ok_or(Errno::Badf)?;
            file.flush().map_err(|_| Errno::Io)?;
            let saved = file.file.stream_position().map_err(|_| Errno::Io)?;
            file.file
                .seek(SeekFrom::Start(offset))
                .map_err(|_| Errno::Io)?;
            file.file.write_all(&bytes).map_err(|_| Errno::Io)?;
            file.file
                .seek(SeekFrom::Start(saved))
                .map_err(|_| Errno::Io)?;
            return Ok(u32::try_from(bytes.len())?);
        }
        self.next.fd_pwrite(mem, fd, iovs, offset).await
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if let Some(mut file) = self.files.remove(&u32::from(fd)) {
            file.flush().map_err(|_| Errno::Io)?;
            return Ok(());
        }
        self.next.fd_close(mem, fd).await
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if let Some(file) = self.files.get_mut(&u32::from(fd)) {
            file.flush().map_err(|_| Errno::Io)?;
            file.file.sync_data().map_err(|_| Errno::Io)?;
            return Ok(());
        }
        self.next.fd_datasync(mem, fd).await
    }

    async fn fd_sync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        if let Some(file) = self.files.get_mut(&u32::from(fd)) {
            file.flush().map_err(|_| Errno::Io)?;
            file.file.sync_all().map_err(|_| Errno::Io)?;
            return Ok(());
        }
        self.next.fd_sync(mem, fd).await
    }

    fn fd_tell(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filesize, Error> {
        if let Some(file) = self.files.get_mut(&u32::from(fd)) {
            // Buffered bytes are logically already written, so include them.
            let pos = file.file.stream_position().map_err(|_| Errno::Io)?;
            return Ok(pos + file.buf.len() as u64);
        }
        self.next.fd_tell(mem, fd)
    }

    async fn fd_seek(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filedelta,
        whence: Whence,
    ) -> Result<Filesize, Error> {
        if let Some(file) = self.files.get_mut(&u32::from(fd)) {
            file.flush().map_err(|_| Errno::Io)?;
            let target = match whence {
                Whence::Set => SeekFrom::Start(offset as u64),
                Whence::Cur => SeekFrom::Current(offset),
                Whence::End => SeekFrom::End(offset),
            };
            return file.file.seek(target).map_err(|_| Errno::Io.into());
        }
        self.next.fd_seek(mem, fd, offset, whence).await
    }
}
