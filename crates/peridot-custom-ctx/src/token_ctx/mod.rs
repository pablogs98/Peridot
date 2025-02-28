use std::ops::Deref;
use log::info;
use log::debug;
use std::sync::Arc;
use peridot::token::TokenBucket;
use wasi_common::snapshots::preview_1::types::{Advice, CiovecArray, Clockid, Dircookie, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta, Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights, Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence};
use wasi_common::snapshots::preview_1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasi_common::{Error, ErrorExt, WasiCtx};
use wiggle::{GuestMemory, GuestPtr};
pub struct PeridotTokenCtx {
    inner: WasiCtx,
    bucket: Arc<TokenBucket>,
}

impl PeridotTokenCtx {
    pub fn new(inner: WasiCtx, bucket: Arc<TokenBucket>) -> Self {
        Self { inner, bucket }
    }

    pub fn get_inner(&self) -> &WasiCtx {
        &self.inner
    }

    pub fn get_inner_mut(&mut self) -> &mut WasiCtx {
        &mut self.inner
    }

    pub fn get_bucket(&self) -> &TokenBucket {
        &self.bucket
    }
}

#[async_trait::async_trait]
impl WasiSnapshotPreview1 for PeridotTokenCtx {
    async fn args_get(&mut self, mem: &mut GuestMemory<'_>, argv: GuestPtr<GuestPtr<u8>>, argv_buf: GuestPtr<u8>) -> Result<(), Error> {
        WasiSnapshotPreview1::args_get(&mut self.inner, mem, argv, argv_buf).await
    }

    async fn args_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::args_sizes_get(&mut self.inner, mem).await
    }

    async fn environ_get(&mut self, mem: &mut GuestMemory<'_>, environ: GuestPtr<GuestPtr<u8>>, environ_buf: GuestPtr<u8>) -> Result<(), Error> {
        WasiSnapshotPreview1::environ_get(&mut self.inner, mem, environ, environ_buf).await
    }

    async fn environ_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::environ_sizes_get(&mut self.inner, mem).await
    }

    async fn clock_res_get(&mut self, mem: &mut GuestMemory<'_>, id: Clockid) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_res_get(&mut self.inner, mem, id).await
    }

    async fn clock_time_get(&mut self, mem: &mut GuestMemory<'_>, id: Clockid, precision: Timestamp) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_time_get(&mut self.inner, mem, id, precision).await
    }

    async fn fd_advise(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filesize, len: Filesize, advice: Advice) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_advise(&mut self.inner, mem, fd, offset, len, advice).await
    }

    async fn fd_allocate(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filesize, len: Filesize) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_allocate(&mut self.inner, mem, fd, offset, len).await
    }

    async fn fd_close(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_close(&mut self.inner, mem, fd).await
    }

    async fn fd_datasync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_datasync(&mut self.inner, mem, fd).await
    }

    async fn fd_fdstat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Fdstat, Error> {
        WasiSnapshotPreview1::fd_fdstat_get(&mut self.inner, mem, fd).await
    }

    async fn fd_fdstat_set_flags(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Fdflags) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_flags(&mut self.inner, mem, fd, flags).await
    }

    async fn fd_fdstat_set_rights(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, fs_rights_base: Rights, fs_rights_inheriting: Rights) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_rights(&mut self.inner, mem, fd, fs_rights_base, fs_rights_inheriting).await
    }

    async fn fd_filestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::fd_filestat_get(&mut self.inner, mem, fd).await
    }

    async fn fd_filestat_set_size(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, size: Filesize) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_size(&mut self.inner, mem, fd, size).await
    }

    async fn fd_filestat_set_times(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_times(&mut self.inner, mem, fd, atim, mtim, fst_flags).await
    }

    async fn fd_pread(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray, offset: Filesize) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_pread(&mut self.inner, mem, fd, iovs, offset).await
    }

    async fn fd_prestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Prestat, Error> {
        WasiSnapshotPreview1::fd_prestat_get(&mut self.inner, mem, fd).await
    }

    async fn fd_prestat_dir_name(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<u8>, path_len: Size) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_prestat_dir_name(&mut self.inner, mem, fd, path, path_len).await
    }

    async fn fd_pwrite(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray, offset: Filesize) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_pwrite(&mut self.inner, mem, fd, iovs, offset).await
    }

    async fn fd_read(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: IovecArray) -> Result<Size, Error> {
        let ctx = self.get_inner();

        let mut bytes_to_read = 0;

        for i in 0..iovs.len() {
            let iov = iovs.get(i).unwrap(); // Convierte Option en Result
            let iovec = mem.read(iov)?; // Lee el Iovec desde memoria

            let buf_len = iovec.buf_len as usize;

            if buf_len == 0 {
                continue;
            }

            bytes_to_read += buf_len as i32;
        }

        self.bucket.consume(bytes_to_read as u32);

        debug!("Tokens consumed in fd_read: {}", bytes_to_read);

        WasiSnapshotPreview1::fd_read(&mut self.inner, mem, fd, iovs).await
    }

    async fn fd_readdir(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, buf: GuestPtr<u8>, buf_len: Size, cookie: Dircookie) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_readdir(&mut self.inner, mem, fd, buf, buf_len, cookie).await
    }

    async fn fd_renumber(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, to: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_renumber(&mut self.inner, mem, fd, to).await
    }

    async fn fd_seek(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, offset: Filedelta, whence: Whence) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_seek(&mut self.inner, mem, fd, offset, whence).await
    }

    async fn fd_sync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_sync(&mut self.inner, mem, fd).await
    }

    async fn fd_tell(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_tell(&mut self.inner, mem, fd).await
    }

    async fn fd_write(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, iovs: CiovecArray) -> Result<Size, Error> {
        let ctx = self.get_inner();

        let mut bytes_to_write = 0;

        for i in 0..iovs.len() {
            let iov = iovs.get(i).unwrap(); // Convierte Option en Result
            let ciovec = mem.read(iov)?; // Lee el Ciovec desde memoria

            let buf_len = ciovec.buf_len as usize;

            if buf_len == 0 {
                continue;
            }

            bytes_to_write += buf_len as i32;
        }

        self.bucket.consume(bytes_to_write as u32);
        println!("Tokens consumed in fd_write: {}", bytes_to_write);

        WasiSnapshotPreview1::fd_write(&mut self.inner, mem, fd, iovs).await
    }


    async fn path_create_directory(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_create_directory(&mut self.inner, mem, fd, path).await
    }

    async fn path_filestat_get(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Lookupflags, path: GuestPtr<str>) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::path_filestat_get(&mut self.inner, mem, fd, flags, path).await
    }

    async fn path_filestat_set_times(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Lookupflags, path: GuestPtr<str>, atim: Timestamp, mtim: Timestamp, fst_flags: Fstflags) -> Result<(), Error> {
        WasiSnapshotPreview1::path_filestat_set_times(&mut self.inner, mem, fd, flags, path, atim, mtim, fst_flags).await
    }

    async fn path_link(&mut self, mem: &mut GuestMemory<'_>, old_fd: Fd, old_flags: Lookupflags, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_link(&mut self.inner, mem, old_fd, old_flags, old_path, new_fd, new_path).await
    }

    async fn path_open(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, dirflags: Lookupflags, path: GuestPtr<str>, oflags: Oflags, fs_rights_base: Rights, fs_rights_inheriting: Rights, fdflags: Fdflags) -> Result<Fd, Error> {
        WasiSnapshotPreview1::path_open(&mut self.inner, mem, fd, dirflags, path, oflags, fs_rights_base, fs_rights_inheriting, fdflags).await
    }

    async fn path_readlink(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>, buf: GuestPtr<u8>, buf_len: Size) -> Result<Size, Error> {
        WasiSnapshotPreview1::path_readlink(&mut self.inner, mem, fd, path, buf, buf_len).await
    }

    async fn path_remove_directory(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_remove_directory(&mut self.inner, mem, fd, path).await
    }

    async fn path_rename(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, old_path: GuestPtr<str>, new_fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_rename(&mut self.inner, mem, fd, old_path, new_fd, new_path).await
    }

    async fn path_symlink(&mut self, mem: &mut GuestMemory<'_>, old_path: GuestPtr<str>, fd: Fd, new_path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_symlink(&mut self.inner, mem, old_path, fd, new_path).await
    }

    async fn path_unlink_file(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, path: GuestPtr<str>) -> Result<(), Error> {
        WasiSnapshotPreview1::path_unlink_file(&mut self.inner, mem, fd, path).await
    }

    async fn poll_oneoff(&mut self, mem: &mut GuestMemory<'_>, in_: GuestPtr<Subscription>, out: GuestPtr<Event>, nsubscriptions: Size) -> Result<Size, Error> {
        WasiSnapshotPreview1::poll_oneoff(&mut self.inner, mem, in_, out, nsubscriptions).await
    }

    async fn proc_exit(&mut self, mem: &mut GuestMemory<'_>, rval: Exitcode) -> anyhow::Error {
        WasiSnapshotPreview1::proc_exit(&mut self.inner, mem, rval).await
    }

    async fn proc_raise(&mut self, mem: &mut GuestMemory<'_>, sig: Signal) -> Result<(), Error> {
        WasiSnapshotPreview1::proc_raise(&mut self.inner, mem, sig).await
    }

    async fn sched_yield(&mut self, mem: &mut GuestMemory<'_>) -> Result<(), Error> {
        WasiSnapshotPreview1::sched_yield(&mut self.inner, mem).await
    }

    async fn random_get(&mut self, mem: &mut GuestMemory<'_>, buf: GuestPtr<u8>, buf_len: Size) -> Result<(), Error> {
        WasiSnapshotPreview1::random_get(&mut self.inner, mem, buf, buf_len).await
    }

    async fn sock_accept(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, flags: Fdflags) -> Result<Fd, Error> {
        WasiSnapshotPreview1::sock_accept(&mut self.inner, mem, fd, flags).await
    }

    async fn sock_recv(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, ri_data: IovecArray, ri_flags: Riflags) -> Result<(Size, Roflags), Error> {
        WasiSnapshotPreview1::sock_recv(&mut self.inner, mem, fd, ri_data, ri_flags).await
    }

    async fn sock_send(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, si_data: CiovecArray, si_flags: Siflags) -> Result<Size, Error> {
        WasiSnapshotPreview1::sock_send(&mut self.inner, mem, fd, si_data, si_flags).await
    }

    async fn sock_shutdown(&mut self, mem: &mut GuestMemory<'_>, fd: Fd, how: Sdflags) -> Result<(), Error> {
        WasiSnapshotPreview1::sock_shutdown(&mut self.inner, mem, fd, how).await
    }
}

impl Deref for PeridotTokenCtx {
    type Target = WasiCtx;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

super::define_wasi!(block_on);