use log::{error, info};
use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use arrow::datatypes::{DataType, Field, Schema};
use aws_sdk_s3::primitives::ByteStream;
use tokio::runtime::Runtime;
use wasi_common::snapshots::preview_1::types;
use wasi_common::snapshots::preview_1::types::{
    Advice, CiovecArray, Clockid, Dircookie, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta,
    Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights,
    Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence,
};
use wasi_common::snapshots::preview_1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasi_common::{Error, WasiCtx};
use wiggle::{tracing, GuestError, GuestMemory, GuestPtr};

use arrow::array::{BinaryArray, ArrayRef};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::io::Cursor;

pub struct PeridotBatchCtx {
    inner: WasiCtx,
    batch_size: usize,
    bucket: String,
    s3: aws_sdk_s3::Client,
    tokio_runtime: Runtime,
    pending_batches: Vec<Vec<u8>>,
    futures: Vec<tokio::task::JoinHandle<Result<aws_sdk_s3::operation::put_object::PutObjectOutput,  aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>>>>
}
impl PeridotBatchCtx {
    pub fn new(inner: WasiCtx, batch_size: usize, bucket: &str) -> Self {
        std::env::set_var("WASMTIME_LOG", "wasmtime_wasi=trace");
        let config = Runtime::new()
            .unwrap()
            .block_on(aws_config::load_from_env());
        let client = aws_sdk_s3::Client::new(&config);
        Self {
            inner,
            s3: client,
            batch_size,
            bucket: bucket.to_string(),
            pending_batches: Vec::new(),
            tokio_runtime: Runtime::new().unwrap(),
            futures: Vec::new(),
        }
    }

    pub fn get_inner(&self) -> &WasiCtx {
        &self.inner
    }

    pub fn get_inner_mut(&mut self) -> &mut WasiCtx {
        &mut self.inner
    }

    pub fn flush_pending_uploads(&mut self) {
        while let Some(fut) = self.futures.pop() {
            match self.tokio_runtime.block_on(fut) {
                Ok(_) => tracing::info!("Upload successful"),
                Err(e) => tracing::error!("Upload failed: {:?}", e),
            }
        }
    }

    /// Escribe un fichero a un lote. Cuando se acumulan `batch_size`, los sube juntos.
    pub fn write_file_to_batch(&mut self, content: Vec<u8>) {
        self.pending_batches.push(content);

        tracing::info!("Added file to batch, current batch size: {}", self.pending_batches.len());

        if  self.pending_batches.len() >= self.batch_size {
            let files_to_upload = self.pending_batches.drain(..).collect::<Vec<_>>();

            let client = self.s3.clone();
            // generate random key (not uuid)
            let key = format!("batch_{}", rand::random::<u16>());

            let compressed = create_parquet_from_binary_chunks(&files_to_upload);

            //write to local first:
            let local_file_path = format!("{}.parquet", key);
            std::fs::write(&local_file_path, &compressed)
                .expect("Failed to write local parquet file");

            tracing::info!("Uploading batch with {} files to S3 with key: {}", files_to_upload.len(), key);

            self.futures.push(self.tokio_runtime.spawn(self.s3
                .put_object()
                .bucket(&self.bucket)
                .key(format!("{}.parquet", key))
                .body(aws_sdk_s3::primitives::ByteStream::from(compressed))
                .send()));

            //self.flush_pending_uploads();
        }
    }
}

impl Drop for PeridotBatchCtx {
    fn drop(&mut self) {
        tracing::info!("Flushing uploads before dropping context...");
        self.flush_pending_uploads();
    }
}

pub fn create_parquet_from_binary_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    // Define el schema: una sola columna binaria
    let schema = Arc::new(Schema::new(vec![
        Field::new("binary_data", DataType::Binary, false),
    ]));

    // Convierte Vec<Vec<u8>> -> Vec<&[u8]>
    let binary_refs: Vec<&[u8]> = chunks.iter().map(|v| v.as_slice()).collect();

    // Crea BinaryArray desde Vec<&[u8]>
    let array = Arc::new(BinaryArray::from(binary_refs)) as ArrayRef;

    // Crea el RecordBatch
    let batch = RecordBatch::try_new(schema.clone(), vec![array])
        .expect("fallo al crear RecordBatch");

    // Crea el buffer Parquet
    let mut buffer = Cursor::new(Vec::new());
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(&mut buffer, schema.clone(), Some(props))
        .expect("fallo al crear ArrowWriter");

    writer.write(&batch).expect("fallo al escribir RecordBatch");
    writer.close().expect("fallo al cerrar ArrowWriter");

    buffer.into_inner() // Devuelve el archivo parquet como Vec<u8>
}


#[async_trait::async_trait]
impl WasiSnapshotPreview1 for PeridotBatchCtx {
    async fn args_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        argv: GuestPtr<GuestPtr<u8>>,
        argv_buf: GuestPtr<u8>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::args_get(&mut self.inner, mem, argv, argv_buf).await
    }

    async fn args_sizes_get(&mut self, mem: &mut GuestMemory<'_>) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::args_sizes_get(&mut self.inner, mem).await
    }

    async fn environ_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        environ: GuestPtr<GuestPtr<u8>>,
        environ_buf: GuestPtr<u8>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::environ_get(&mut self.inner, mem, environ, environ_buf).await
    }

    async fn environ_sizes_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
    ) -> Result<(Size, Size), Error> {
        WasiSnapshotPreview1::environ_sizes_get(&mut self.inner, mem).await
    }

    async fn clock_res_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        id: Clockid,
    ) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_res_get(&mut self.inner, mem, id).await
    }

    async fn clock_time_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        id: Clockid,
        precision: Timestamp,
    ) -> Result<Timestamp, Error> {
        WasiSnapshotPreview1::clock_time_get(&mut self.inner, mem, id, precision).await
    }

    async fn fd_advise(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filesize,
        len: Filesize,
        advice: Advice,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_advise(&mut self.inner, mem, fd, offset, len, advice).await
    }

    async fn fd_allocate(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filesize,
        len: Filesize,
    ) -> Result<(), Error> {
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

    async fn fd_fdstat_set_flags(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Fdflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_flags(&mut self.inner, mem, fd, flags).await
    }

    async fn fd_fdstat_set_rights(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        fs_rights_base: Rights,
        fs_rights_inheriting: Rights,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_fdstat_set_rights(
            &mut self.inner,
            mem,
            fd,
            fs_rights_base,
            fs_rights_inheriting,
        )
        .await
    }

    async fn fd_filestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
    ) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::fd_filestat_get(&mut self.inner, mem, fd).await
    }

    async fn fd_filestat_set_size(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        size: Filesize,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_size(&mut self.inner, mem, fd, size).await
    }

    async fn fd_filestat_set_times(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        atim: Timestamp,
        mtim: Timestamp,
        fst_flags: Fstflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_filestat_set_times(&mut self.inner, mem, fd, atim, mtim, fst_flags)
            .await
    }

    async fn fd_pread(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_pread(&mut self.inner, mem, fd, iovs, offset).await
    }

    async fn fd_prestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
    ) -> Result<Prestat, Error> {
        WasiSnapshotPreview1::fd_prestat_get(&mut self.inner, mem, fd).await
    }

    async fn fd_prestat_dir_name(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<u8>,
        path_len: Size,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_prestat_dir_name(&mut self.inner, mem, fd, path, path_len).await
    }

    async fn fd_pwrite(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
        offset: Filesize,
    ) -> Result<Size, Error> {
        tracing::info!("PRWITINGNGNGNGNGNGNG");
        println!("PRINFINFIDSNFDS");
        WasiSnapshotPreview1::fd_pwrite(&mut self.inner, mem, fd, iovs, offset).await
    }

    async fn fd_read(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: IovecArray,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_read(&mut self.inner, mem, fd, iovs).await
    }

    async fn fd_readdir(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        buf: GuestPtr<u8>,
        buf_len: Size,
        cookie: Dircookie,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::fd_readdir(&mut self.inner, mem, fd, buf, buf_len, cookie).await
    }

    async fn fd_renumber(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        to: Fd,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_renumber(&mut self.inner, mem, fd, to).await
    }

    async fn fd_seek(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        offset: Filedelta,
        whence: Whence,
    ) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_seek(&mut self.inner, mem, fd, offset, whence).await
    }

    async fn fd_sync(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<(), Error> {
        WasiSnapshotPreview1::fd_sync(&mut self.inner, mem, fd).await
    }

    async fn fd_tell(&mut self, mem: &mut GuestMemory<'_>, fd: Fd) -> Result<Filesize, Error> {
        WasiSnapshotPreview1::fd_tell(&mut self.inner, mem, fd).await
    }

    async fn fd_write(
        &mut self,
        mem: &mut wiggle::GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        unsafe {
            if (fd.inner() < 3 ) {
                tracing::info!("writing to STDOUT/STDERR, FD={}", fd.inner());
                return WasiSnapshotPreview1::fd_write(&mut self.inner, mem, fd, iovs).await
            }

        let buf = first_non_empty_ciovec(mem, iovs)?;
        let buf = mem.to_vec(buf)?;
        let len = buf.len();
        self.write_file_to_batch(buf);
        tracing::info!("SKIPPING FD_WRITE, writing {} bytes, FD={}",len, fd.inner());
        Ok(u32::try_from(len)?)
    }
    }

    async fn path_create_directory(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_create_directory(&mut self.inner, mem, fd, path).await
    }

    async fn path_filestat_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Lookupflags,
        path: GuestPtr<str>,
    ) -> Result<Filestat, Error> {
        WasiSnapshotPreview1::path_filestat_get(&mut self.inner, mem, fd, flags, path).await
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
        WasiSnapshotPreview1::path_filestat_set_times(
            &mut self.inner,
            mem,
            fd,
            flags,
            path,
            atim,
            mtim,
            fst_flags,
        )
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
        WasiSnapshotPreview1::path_link(
            &mut self.inner,
            mem,
            old_fd,
            old_flags,
            old_path,
            new_fd,
            new_path,
        )
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
        WasiSnapshotPreview1::path_open(
            &mut self.inner,
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

    async fn path_readlink(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
        buf: GuestPtr<u8>,
        buf_len: Size,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::path_readlink(&mut self.inner, mem, fd, path, buf, buf_len).await
    }

    async fn path_remove_directory(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_remove_directory(&mut self.inner, mem, fd, path).await
    }

    async fn path_rename(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        old_path: GuestPtr<str>,
        new_fd: Fd,
        new_path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_rename(&mut self.inner, mem, fd, old_path, new_fd, new_path)
            .await
    }

    async fn path_symlink(
        &mut self,
        mem: &mut GuestMemory<'_>,
        old_path: GuestPtr<str>,
        fd: Fd,
        new_path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_symlink(&mut self.inner, mem, old_path, fd, new_path).await
    }

    async fn path_unlink_file(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        path: GuestPtr<str>,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::path_unlink_file(&mut self.inner, mem, fd, path).await
    }

    async fn poll_oneoff(
        &mut self,
        mem: &mut GuestMemory<'_>,
        in_: GuestPtr<Subscription>,
        out: GuestPtr<Event>,
        nsubscriptions: Size,
    ) -> Result<Size, Error> {
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

    async fn random_get(
        &mut self,
        mem: &mut GuestMemory<'_>,
        buf: GuestPtr<u8>,
        buf_len: Size,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::random_get(&mut self.inner, mem, buf, buf_len).await
    }

    async fn sock_accept(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        flags: Fdflags,
    ) -> Result<Fd, Error> {
        WasiSnapshotPreview1::sock_accept(&mut self.inner, mem, fd, flags).await
    }

    async fn sock_recv(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        ri_data: IovecArray,
        ri_flags: Riflags,
    ) -> Result<(Size, Roflags), Error> {
        WasiSnapshotPreview1::sock_recv(&mut self.inner, mem, fd, ri_data, ri_flags).await
    }

    async fn sock_send(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        si_data: CiovecArray,
        si_flags: Siflags,
    ) -> Result<Size, Error> {
        WasiSnapshotPreview1::sock_send(&mut self.inner, mem, fd, si_data, si_flags).await
    }

    async fn sock_shutdown(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        how: Sdflags,
    ) -> Result<(), Error> {
        WasiSnapshotPreview1::sock_shutdown(&mut self.inner, mem, fd, how).await
    }
}

fn read_string<'a>(memory: &'a GuestMemory<'_>, ptr: GuestPtr<str>) -> Result<String, GuestError> {
    Ok(memory.as_cow_str(ptr)?.into_owned())
}

fn first_non_empty_ciovec(
    memory: &GuestMemory<'_>,
    ciovs: types::CiovecArray,
) -> Result<GuestPtr<[u8]>, GuestError> {
    for iov in ciovs.iter() {
        let iov = memory.read(iov?)?;
        if iov.buf_len == 0 {
            continue;
        }
        return Ok(iov.buf.as_array(iov.buf_len));
    }
    Ok(GuestPtr::new((0, 0)))
}

impl Deref for PeridotBatchCtx {
    type Target = WasiCtx;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

#[macro_export]
macro_rules! define_wasi {
    ($async_mode:tt $($bounds:tt)*) => {

    use wasmtime::Linker;

    pub fn add_to_linker<T, U>(
        linker: &mut Linker<T>,
        get_cx: impl Fn(&mut T) -> &mut U + Send + Sync + Copy + 'static,
    ) -> anyhow::Result<()>
        where U: Send
                    + wasi_common::snapshots::preview_1::wasi_snapshot_preview1::WasiSnapshotPreview1,
            $($bounds)*
    {
        snapshots::preview_1::add_wasi_snapshot_preview1_to_linker(linker, get_cx)?;
        Ok(())
    }

    pub mod snapshots {
        pub mod preview_1 {
            wiggle::wasmtime_integration!({
                // The wiggle code to integrate with lives here:
                target: wasi_common::snapshots::preview_1,
                witx: ["$CARGO_MANIFEST_DIR/witx/wasi_snapshot_preview1.witx"],
                errors: { errno => trappable Error },
                $async_mode: *
            });
        }
    }
}}

define_wasi!(block_on);
