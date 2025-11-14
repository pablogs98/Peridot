use log::{error, info};
use std::collections::{BTreeMap, HashMap};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex};
use arrow::datatypes::{DataType, Field, Schema};
use aws_sdk_s3::primitives::ByteStream;
use tokio::runtime::Runtime;
use wasmtime_wasi::p1::types::{Error, Advice, CiovecArray, Clockid, Dircookie, Event, Exitcode, Fd, Fdflags, Fdstat, Filedelta, Filesize, Filestat, Fstflags, IovecArray, Lookupflags, Oflags, Prestat, Riflags, Rights, Roflags, Sdflags, Siflags, Signal, Size, Subscription, Timestamp, Whence};
use wasmtime_wasi::p1::wasi_snapshot_preview1::WasiSnapshotPreview1;
use wasmtime_wasi::p1::WasiP1Ctx;
use wiggle::{tracing, GuestError, GuestMemory, GuestPtr};
use peridot::context::{DelegatingWasiCtx, PeridotContext};

use arrow::array::{BinaryArray, ArrayRef};
use arrow::record_batch::RecordBatch;
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use std::io::Cursor;
use async_trait::async_trait;

pub struct PeridotBatchCtx {
    inner: PeridotContext,
    batch_size: usize,
    bucket: String,
    s3: aws_sdk_s3::Client,
    tokio_runtime: Runtime,
    pending_batches: Vec<Vec<u8>>,
    futures: Vec<tokio::task::JoinHandle<Result<aws_sdk_s3::operation::put_object::PutObjectOutput,  aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>>>>
}
impl PeridotBatchCtx {
    pub fn new(inner: PeridotContext, batch_size: usize, bucket: &str) -> Self {
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


#[async_trait]
impl DelegatingWasiCtx for PeridotBatchCtx {
    fn inner(&mut self) -> &mut WasiP1Ctx {
        self.inner.inner()
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        unsafe {
            if (fd.inner() < 3 ) {
                tracing::info!("writing to STDOUT/STDERR, FD={}", fd.inner());
                return self.inner.fd_write(mem, fd, iovs).await
            }

            let buf = first_non_empty_ciovec(mem, iovs)?;
            let buf = mem.to_vec(buf)?;
            let len = buf.len();
            self.write_file_to_batch(buf);
            tracing::info!("SKIPPING FD_WRITE, writing {} bytes, FD={}",len, fd.inner());
            Ok(u32::try_from(len)?)
        }
    }
}

fn read_string(memory: &GuestMemory, ptr: GuestPtr<str>) -> Result<String, GuestError> {
    Ok(memory.as_cow_str(ptr)?.into_owned())
}

fn first_non_empty_ciovec(
    memory: &GuestMemory<'_>,
    ciovs: CiovecArray,
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
    type Target = PeridotContext;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}