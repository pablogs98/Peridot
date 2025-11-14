use async_trait::async_trait;
use std::ops::Deref;
use wasmtime::Linker;
use wasmtime_wasi::p1::types::{Advice, CiovecArray, Clockid, Dircookie, Error, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta, Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights, Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence};
use wasmtime_wasi::p1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasmtime_wasi::p1::{wasi_snapshot_preview1, WasiP1Ctx};
use wiggle::{anyhow, GuestMemory, GuestPtr};

#[async_trait]
pub trait DelegatingWasiCtx: Send {
    fn inner(&mut self) -> &mut WasiP1Ctx;

    fn args_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        argv: GuestPtr<GuestPtr<u8>>,
        argv_buf: GuestPtr<u8>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::args_get(self.inner(), mem, argv, argv_buf)
    }

    fn args_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::args_sizes_get(self.inner(), mem)
    }

    fn environ_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        environ: GuestPtr<GuestPtr<u8>>,
        environ_buf: GuestPtr<u8>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::environ_get(self.inner(), mem, environ, environ_buf)
    }

    fn environ_sizes_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
    ) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::environ_sizes_get(self.inner(), mem)
    }

    fn clock_res_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        id: Clockid,
    ) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_res_get(self.inner(), mem, id)
    }

    fn clock_time_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        id: Clockid,
        precision: Timestamp,
    ) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_time_get(self.inner(), mem, id, precision)
    }

    async fn fd_advise(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filesize,
        len: Filesize,
        advice: Advice,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_advise(self.inner(), mem, fd, offset, len, advice).await
    }

    fn fd_allocate(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filesize,
        len: Filesize,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_allocate(self.inner(), mem, fd, offset, len)
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_close(self.inner(), mem, fd).await
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_datasync(self.inner(), mem, fd).await
    }

    async fn fd_fdstat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Fdstat, Error> {
        WasiSnapshotPreview1::fd_fdstat_get(self.inner(), mem, fd).await
    }

    fn fd_fdstat_set_flags(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Fdflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_flags(self.inner(), mem, fd, flags)
    }

    fn fd_fdstat_set_rights(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        fs_rights_base: Rights,
        fs_rights_inheriting: Rights,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_rights(self.inner(), mem, fd, fs_rights_base, fs_rights_inheriting)
    }

    async fn fd_filestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
    ) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::fd_filestat_get(self.inner(), mem, fd).await
    }

    async fn fd_filestat_set_size(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        size: Filesize,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_size(self.inner(), mem, fd, size).await
    }

    async fn fd_filestat_set_times(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        atim: Timestamp,
        mtim: Timestamp,
        fst_flags: Fstflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_times(self.inner(), mem, fd, atim, mtim, fst_flags)
            .await
    }

    async fn fd_pread(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_pread(self.inner(), mem, fd, iovs, offset).await
    }

    fn fd_prestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
    ) -> Result<Prestat, Error> {
        WasiSnapshotPreview1::fd_prestat_get(self.inner(), mem, fd)
    }

    fn fd_prestat_dir_name(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<u8>,
        path_len: Size,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_prestat_dir_name(self.inner(), mem, fd, path, path_len)
    }

    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_pwrite(self.inner(), mem, fd, iovs, offset).await
    }

    async fn fd_read(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_read(self.inner(), mem, fd, iovs).await
    }

    async fn fd_readdir(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        buf: GuestPtr<u8>,
        buf_len: Size,
        cookie: Dircookie,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_readdir(self.inner(), mem, fd, buf, buf_len, cookie).await
    }

    fn fd_renumber(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        to: Fd,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_renumber(self.inner(), mem, fd, to)
    }

    async fn fd_seek(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filedelta,
        whence: Whence,
    ) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_seek(self.inner(), mem, fd, offset, whence).await
    }

    async fn fd_sync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_sync(self.inner(), mem, fd).await
    }

    fn fd_tell(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_tell(self.inner(), mem, fd)
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_write(self.inner(), mem, fd, iovs).await
    }

    async fn path_create_directory(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_create_directory(self.inner(), mem, fd, path).await
    }

    async fn path_filestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Lookupflags,
        path: GuestPtr<str>,
    ) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::path_filestat_get(self.inner(), mem, fd, flags, path).await
    }

    async fn path_filestat_set_times(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Lookupflags,
        path: GuestPtr<str>,
        atim: Timestamp,
        mtim: Timestamp,
        fst_flags: Fstflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_filestat_set_times(self.inner(), mem, fd, flags, path, atim, mtim, fst_flags)
            .await
    }

    async fn path_link(
        &mut self,
        mem: &mut GuestMemory<'_>,
        old_fd: Fd,
        old_flags: Lookupflags,
        old_path: GuestPtr<str>,
        new_fd: Fd,
        new_path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_link(self.inner(), mem, old_fd, old_flags, old_path, new_fd, new_path)
            .await
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
        WasiSnapshotPreview1::path_open(self.inner(), mem, fd, dirflags, path, oflags, fs_rights_base, fs_rights_inheriting, fdflags)
            .await
    }

    async fn path_readlink(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
        buf: GuestPtr<u8>,
        buf_len: Size,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::path_readlink(self.inner(), mem, fd, path, buf, buf_len).await
    }

    async fn path_remove_directory(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_remove_directory(self.inner(), mem, fd, path).await
    }

    async fn path_rename(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        old_path: GuestPtr<str>,
        new_fd: Fd,
        new_path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_rename(self.inner(), mem, fd, old_path, new_fd, new_path)
            .await
    }

    async fn path_symlink(
        &mut self,
        mem: &mut GuestMemory<'_>,
        old_path: GuestPtr<str>,
        fd: Fd,
        new_path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_symlink(self.inner(), mem, old_path, fd, new_path).await
    }

    async fn path_unlink_file(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_unlink_file(self.inner(), mem, fd, path).await
    }

    async fn poll_oneoff(
        &mut self,
        mem: &mut GuestMemory<'_>,
        in_: GuestPtr<Subscription>,
        out: GuestPtr<Event>,
        nsubscriptions: Size,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::poll_oneoff(self.inner(), mem, in_, out, nsubscriptions).await
    }

    fn proc_exit(&mut self, mem: &mut GuestMemory<'_>, rval: Exitcode) -> anyhow::Error {
        WasiSnapshotPreview1::proc_exit(self.inner(), mem, rval)
    }

    fn proc_raise(&mut self, mem: &mut GuestMemory<'_>, sig: Signal) -> Result<(), Error> {
        WasiSnapshotPreview1::proc_raise(self.inner(), mem, sig)
    }

    fn sched_yield(&mut self, mem: &mut GuestMemory<'_>) -> Result<(), Error> {
        WasiSnapshotPreview1::sched_yield(self.inner(), mem)
    }

    fn random_get(&mut self, mem: &mut GuestMemory<'_>, buf: GuestPtr<u8>, buf_len: Size) -> Result<(), Error> {
        WasiSnapshotPreview1::random_get(self.inner(), mem, buf, buf_len)
    }

    fn sock_accept(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Fdflags) -> Result<Fd, Error> {
        WasiSnapshotPreview1::sock_accept(self.inner(), mem, fd, flags)
    }

    fn sock_recv(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, ri_data: IovecArray, ri_flags: Riflags) -> Result<(Size, Roflags), Error> {
        WasiSnapshotPreview1::sock_recv(self.inner(), mem, fd, ri_data, ri_flags)
    }

    fn sock_send(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, si_data: CiovecArray, si_flags: Siflags) -> Result<Size, Error> {
        WasiSnapshotPreview1::sock_send(self.inner(), mem, fd, si_data, si_flags)
    }

    fn sock_shutdown(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, how: Sdflags) -> Result<(), Error> {
        WasiSnapshotPreview1::sock_shutdown(self.inner(), mem, fd, how)
    }
}

pub struct PeridotContext {
    pub inner: WasiP1Ctx,
}

impl PeridotContext {
    pub fn new(inner: WasiP1Ctx) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl DelegatingWasiCtx for PeridotContext {
    fn inner(&mut self) -> &mut WasiP1Ctx {
        &mut self.inner
    }
}

pub struct WasiWrapper<T: DelegatingWasiCtx> {
    pub ctx: T,
}

impl<T: DelegatingWasiCtx> WasiWrapper<T> {
    pub fn new(ctx: T) -> Self {
        Self { ctx }
    }
}

impl <T: DelegatingWasiCtx> Deref for WasiWrapper<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

#[async_trait::async_trait]
impl<T: DelegatingWasiCtx + Send> WasiSnapshotPreview1 for WasiWrapper<T> {
    fn args_get(&mut self, mem: &mut GuestMemory<'_>, argv: GuestPtr<GuestPtr<u8>>, argv_buf: GuestPtr<u8>) -> Result<(), Error> {
        self.ctx.args_get(mem, argv, argv_buf)
    }

    fn args_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        self.ctx.args_sizes_get(mem)
    }

    fn environ_get(&mut self, mem: &mut GuestMemory<'_>, environ: GuestPtr<GuestPtr<u8>>, environ_buf: GuestPtr<u8>) -> Result<(), Error> {
        self.ctx.environ_get(mem, environ, environ_buf)
    }

    fn environ_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        self.ctx.environ_sizes_get(mem)
    }

    fn clock_res_get(&mut self, mem: &mut GuestMemory<'_>, id: Clockid) -> Result<Timestamp, Error> {
        self.ctx.clock_res_get(mem, id)
    }

    fn clock_time_get(&mut self, mem: &mut GuestMemory<'_>, id: Clockid, precision: Timestamp) -> Result<Timestamp, Error> {
        self.ctx.clock_time_get(mem, id, precision)
    }

    async fn fd_advise(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filesize, len: Filesize, advice: Advice) -> Result<(), Error> {
        self.ctx.fd_advise(mem, fd, offset, len, advice).await
    }

    fn fd_allocate(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filesize, len: Filesize) -> Result<(), Error> {
        self.ctx.fd_allocate(mem, fd, offset, len)
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        self.ctx.fd_close(mem, fd).await
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        self.ctx.fd_datasync(mem, fd).await
    }

    async fn fd_fdstat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Fdstat, Error> {
        self.ctx.fd_fdstat_get(mem, fd).await
    }

    fn fd_fdstat_set_flags(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Fdflags) -> Result<(), Error> {
        self.ctx.fd_fdstat_set_flags(mem, fd, flags)
    }

    fn fd_fdstat_set_rights(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, fs_rights_base: Rights, fs_rights_inheriting: Rights) -> Result<(), Error> {
        self.ctx.fd_fdstat_set_rights(mem, fd, fs_rights_base, fs_rights_inheriting)
    }

    async fn fd_filestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filestat, Error> {
        self.ctx.fd_filestat_get(mem, fd).await
    }

    async fn fd_filestat_set_size(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, size: Filesize) -> Result<(), Error> {
        self.ctx.fd_filestat_set_size(mem, fd, size).await
    }

    async fn fd_filestat_set_times(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error> {
        self.ctx.fd_filestat_set_times(mem, fd, atim, mtim, fst_flags).await
    }

    async fn fd_pread(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error> {
        self.ctx.fd_pread(mem, fd, iovs, offset).await
    }

    fn fd_prestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Prestat, Error> {
        self.ctx.fd_prestat_get(mem, fd)
    }

    fn fd_prestat_dir_name(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<u8>, path_len: Size) -> Result<(), Error> {
        self.ctx.fd_prestat_dir_name(mem, fd, path, path_len)
    }

    async fn fd_pwrite(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error> {
        self.ctx.fd_pwrite(mem, fd, iovs, offset).await
    }

    async fn fd_read(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray) -> Result<Size, Error> {
        self.ctx.fd_read(mem, fd, iovs).await
    }

    async fn fd_readdir(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, buf: GuestPtr<u8>, buf_len: Size, cookie: Dircookie) -> Result<Size, Error> {
        self.ctx.fd_readdir(mem, fd, buf, buf_len, cookie).await
    }

    fn fd_renumber(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, to: Fd) -> Result<(), Error> {
        self.ctx.fd_renumber(mem, fd, to)
    }

    async fn fd_seek(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filedelta, whence: Whence) -> Result<Filesize, Error> {
        self.ctx.fd_seek(mem, fd, offset, whence).await
    }

    async fn fd_sync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        self.ctx.fd_sync(mem, fd).await
    }

    fn fd_tell(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filesize, Error> {
        self.ctx.fd_tell(mem, fd)
    }

    async fn fd_write(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray) -> Result<Size, Error> {
        self.ctx.fd_write(mem, fd, iovs).await
    }

    async fn path_create_directory(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_create_directory(mem, fd, path).await
    }

    async fn path_filestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Lookupflags, path: GuestPtr<str>) -> Result<Filestat, Error> {
        self.ctx.path_filestat_get(mem, fd, flags, path).await
    }

    async fn path_filestat_set_times(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Lookupflags, path: GuestPtr<str>, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error> {
        self.ctx.path_filestat_set_times(mem, fd, flags, path, atim, mtim, fst_flags).await
    }

    async fn path_link(&mut self, mem: &mut GuestMemory<'_>, old_fd: Fd, old_flags: Lookupflags, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_link(mem, old_fd, old_flags, old_path, new_fd, new_path).await
    }

    async fn path_open(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, dirflags: Lookupflags, path: GuestPtr<str>, oflags: Oflags, fs_rights_base: Rights, fs_rights_inheriting: Rights, fdflags: Fdflags) -> Result<Fd, Error> {
        self.ctx.path_open(mem, fd, dirflags, path, oflags, fs_rights_base, fs_rights_inheriting, fdflags).await
    }

    async fn path_readlink(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>, buf: GuestPtr<u8>, buf_len: Size) -> Result<Size, Error> {
        self.ctx.path_readlink(mem, fd, path, buf, buf_len).await
    }

    async fn path_remove_directory(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_remove_directory(mem, fd, path).await
    }

    async fn path_rename(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_rename(mem, fd, old_path, new_fd, new_path).await
    }

    async fn path_symlink(&mut self, mem: &mut GuestMemory<'_>, old_path: GuestPtr<str>, fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_symlink(mem, old_path, fd, new_path).await
    }

    async fn path_unlink_file(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        self.ctx.path_unlink_file(mem, fd, path).await
    }

    async fn poll_oneoff(&mut self, mem: &mut GuestMemory<'_>, in_: GuestPtr<Subscription>, out: GuestPtr<Event>, nsubscriptions: Size) -> Result<Size, Error> {
        self.ctx.poll_oneoff(mem, in_, out, nsubscriptions).await
    }

    fn proc_exit(&mut self, mem: &mut GuestMemory<'_>, rval: Exitcode) -> anyhow::Error {
        self.ctx.proc_exit(mem, rval)
    }

    fn proc_raise(&mut self, mem: &mut GuestMemory<'_>, sig: Signal) -> Result<(), Error> {
        self.ctx.proc_raise(mem, sig)
    }

    fn sched_yield(&mut self, mem: &mut GuestMemory<'_>) -> Result<(), Error> {
        self.ctx.sched_yield(mem)
    }

    fn random_get(&mut self, mem: &mut GuestMemory<'_>, buf: GuestPtr<u8>, buf_len: Size) -> Result<(), Error> {
        self.ctx.random_get(mem, buf, buf_len)
    }

    fn sock_accept(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Fdflags) -> Result<Fd, Error> {
        self.ctx.sock_accept(mem, fd, flags)
    }

    fn sock_recv(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, ri_data: IovecArray, ri_flags: Riflags) -> Result<(Size, Roflags), Error> {
        self.ctx.sock_recv(mem, fd, ri_data, ri_flags)
    }

    fn sock_send(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, si_data: CiovecArray, si_flags: Siflags) -> Result<Size, Error> {
        self.ctx.sock_send(mem, fd, si_data, si_flags)
    }

    fn sock_shutdown(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, how: Sdflags) -> Result<(), Error> {
        self.ctx.sock_shutdown(mem, fd, how)
    }
}

pub fn add_to_linker_async<T: Send + 'static + WasiSnapshotPreview1>(
    linker: &mut Linker<T>,
    f: impl Fn(&mut T) -> &mut T + Copy + Send + Sync + 'static,
) -> anyhow::Result<()> {
    wasi_snapshot_preview1::add_to_linker(linker, f)
}