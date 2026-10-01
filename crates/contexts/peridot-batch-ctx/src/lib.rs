//! Accumulates the files a guest writes and uploads them to S3 in groups, as one parquet object
//! per batch.
//!
//! Writes to stdio pass straight through. Everything else is buffered: `fd_write` reports success
//! to the guest as soon as the bytes are queued, and the upload happens once `batch_size` files
//! have accumulated. Whatever is still queued when the guest exits is flushed by [`shutdown`].
//!
//! [`shutdown`]: DelegatingWasiCtx::shutdown

use anyhow::{Context as _, Result};
use arrow::array::{ArrayRef, BinaryArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use async_trait::async_trait;
use aws_sdk_s3::error::SdkError;
use aws_sdk_s3::operation::put_object::{PutObjectError, PutObjectOutput};
use aws_sdk_s3::primitives::ByteStream;
use log::{debug, error, info, warn};
use parquet::arrow::ArrowWriter;
use parquet::file::properties::WriterProperties;
use peridot::context::DelegatingWasiCtx;
use peridot::memory;
use peridot::plugin::{BoxedContextFuture, ContextConfig};
use serde::Deserialize;
use std::io::Cursor;
use std::sync::Arc;
use tokio::task::JoinHandle;
use uuid::Uuid;
use wasmtime_wasi::p1::types::{CiovecArray, Error, Fd, Size};
use wiggle::GuestMemory;

/// Builds an S3 client, honouring `AWS_S3_FORCE_PATH_STYLE`.
///
/// The AWS SDK addresses buckets virtual-hosted-style by default, turning an
/// endpoint of `http://minio:9000` into `http://<bucket>.minio:9000`. Amazon
/// S3 resolves that; a self-hosted S3-compatible store (MinIO, Ceph,
/// SeaweedFS) addressed by hostname does not, and every request fails with a
/// dispatch error. Setting `AWS_S3_FORCE_PATH_STYLE=true` keeps the bucket in
/// the path instead. It is off by default, so behaviour against Amazon S3 is
/// unchanged.
async fn build_s3_client() -> aws_sdk_s3::Client {
    let config = aws_config::load_from_env().await;
    let force_path_style = std::env::var("AWS_S3_FORCE_PATH_STYLE")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if force_path_style {
        let builder = aws_sdk_s3::config::Builder::from(&config).force_path_style(true);
        aws_sdk_s3::Client::from_conf(builder.build())
    } else {
        aws_sdk_s3::Client::new(&config)
    }
}

/// Settings for the `batch` context.
#[derive(Debug, Deserialize)]
pub struct BatchSettings {
    /// How many guest files to accumulate before uploading them as one parquet object.
    pub batch_size: usize,
    /// Destination S3 bucket.
    pub bucket: String,
}

impl Default for BatchSettings {
    fn default() -> Self {
        Self {
            batch_size: 100,
            bucket: String::new(),
        }
    }
}

/// Registry entry point. Registered under the name `batch`.
pub fn factory(next: Box<dyn DelegatingWasiCtx>, config: ContextConfig<'_>) -> BoxedContextFuture<'_> {
    Box::pin(async move {
        let settings: BatchSettings = config.parse()?;
        if settings.bucket.is_empty() {
            anyhow::bail!("the `batch` context requires a non-empty `bucket` in its config block");
        }
        Ok(
            Box::new(PeridotBatchCtx::new(next, settings.batch_size, &settings.bucket).await)
                as Box<dyn DelegatingWasiCtx>,
        )
    })
}

pub struct PeridotBatchCtx {
    next: Box<dyn DelegatingWasiCtx>,
    batch_size: usize,
    bucket: String,
    s3: aws_sdk_s3::Client,
    /// Files written by the guest but not yet part of an upload.
    pending_batches: Vec<Vec<u8>>,
    /// Uploads in flight. Spawned rather than awaited, so they have to be joined before the
    /// process exits — see [`PeridotBatchCtx::drain_uploads`].
    uploads: Vec<JoinHandle<Result<PutObjectOutput, SdkError<PutObjectError>>>>,
}

impl PeridotBatchCtx {
    pub async fn new(next: Box<dyn DelegatingWasiCtx>, batch_size: usize, bucket: &str) -> Self {
        let client = build_s3_client().await;
        Self {
            next,
            batch_size,
            bucket: bucket.to_string(),
            s3: client,
            pending_batches: Vec::new(),
            uploads: Vec::new(),
        }
    }

    /// Encodes `files` as one parquet object and starts uploading it.
    ///
    /// Shared by the full-batch path and the partial-batch flush in `shutdown`.
    fn spawn_batch_upload(&mut self, files: Vec<Vec<u8>>) {
        if files.is_empty() {
            return;
        }

        let count = files.len();
        let body = match create_parquet_from_binary_chunks(&files) {
            Ok(body) => body,
            // Encoding failure is deterministic, so re-queueing would only reproduce it. Report
            // the loss loudly instead of panicking inside the guest's `fd_write`.
            Err(e) => {
                error!("dropping a batch of {count} files: could not encode it as parquet: {e:#}");
                return;
            }
        };

        let key = format!("batch_{}.parquet", Uuid::new_v4());
        info!("uploading {count} files to s3://{}/{key}", self.bucket);

        let client = self.s3.clone();
        let bucket = self.bucket.clone();
        self.uploads.push(tokio::spawn(async move {
            client
                .put_object()
                .bucket(bucket)
                .key(key)
                .body(ByteStream::from(body))
                .send()
                .await
        }));
    }

    /// Queues one guest file, uploading the batch once it is full.
    fn write_file_to_batch(&mut self, content: Vec<u8>) {
        self.pending_batches.push(content);
        debug!(
            "queued a file, batch is now {}/{}",
            self.pending_batches.len(),
            self.batch_size
        );

        if self.pending_batches.len() >= self.batch_size {
            let files = self.pending_batches.drain(..).collect::<Vec<_>>();
            self.spawn_batch_upload(files);
        }
    }

    /// Awaits every upload started so far.
    pub async fn drain_uploads(&mut self) {
        if self.uploads.is_empty() {
            return;
        }

        info!("waiting for {} pending batch uploads...", self.uploads.len());
        for handle in self.uploads.drain(..) {
            match handle.await {
                Ok(Ok(_)) => info!("batch upload complete"),
                Ok(Err(e)) => error!("batch upload failed: {e}"),
                Err(e) => error!("batch upload task did not finish: {e}"),
            }
        }
    }
}

impl Drop for PeridotBatchCtx {
    fn drop(&mut self) {
        // `shutdown` is what flushes; reaching here with work outstanding means it was never
        // called, and a `Drop` impl cannot await the uploads to rescue it.
        if !self.pending_batches.is_empty() || !self.uploads.is_empty() {
            warn!(
                "dropping the batch context with {} unflushed files and {} uploads in flight; \
                 shutdown() was never called",
                self.pending_batches.len(),
                self.uploads.len()
            );
        }
    }
}

/// Encodes each chunk as one row of a single-column binary parquet file.
pub fn create_parquet_from_binary_chunks(chunks: &[Vec<u8>]) -> Result<Vec<u8>> {
    let schema = Arc::new(Schema::new(vec![Field::new(
        "binary_data",
        DataType::Binary,
        false,
    )]));

    let binary_refs: Vec<&[u8]> = chunks.iter().map(|v| v.as_slice()).collect();
    let array = Arc::new(BinaryArray::from(binary_refs)) as ArrayRef;

    let batch = RecordBatch::try_new(schema.clone(), vec![array])
        .context("could not build the record batch")?;

    let mut buffer = Cursor::new(Vec::new());
    let props = WriterProperties::builder().build();
    let mut writer = ArrowWriter::try_new(&mut buffer, schema, Some(props))
        .context("could not create the parquet writer")?;
    writer
        .write(&batch)
        .context("could not write the record batch")?;
    writer.close().context("could not finish the parquet file")?;

    Ok(buffer.into_inner())
}

#[async_trait]
impl DelegatingWasiCtx for PeridotBatchCtx {
    fn next(&mut self) -> Option<&mut dyn DelegatingWasiCtx> {
        Some(&mut *self.next)
    }

    /// Uploads the final partial batch, then joins every upload, before the process exits.
    async fn shutdown(&mut self) {
        if !self.pending_batches.is_empty() {
            let files = self.pending_batches.drain(..).collect::<Vec<_>>();
            info!("flushing a final partial batch of {} files", files.len());
            self.spawn_batch_upload(files);
        }
        self.drain_uploads().await;
        self.next.shutdown().await;
    }

    async fn fd_write(
        &mut self,
        mem: &mut GuestMemory<'_>,
        fd: Fd,
        iovs: CiovecArray,
    ) -> Result<Size, Error> {
        if u32::from(fd) < 3 {
            return self.next.fd_write(mem, fd, iovs).await;
        }

        // Buffered until the batch is full, so the bytes must be owned.
        let buf = memory::payload(mem, iovs)?.into_owned();
        let len = buf.len();
        self.write_file_to_batch(buf);
        debug!("batched {len} bytes written to fd {}", u32::from(fd));
        Ok(u32::try_from(len)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Array, BinaryArray};
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

    #[test]
    fn parquet_round_trips_binary_chunks() {
        let chunks = vec![b"hello".to_vec(), b"world!".to_vec(), Vec::new()];

        let encoded =
            create_parquet_from_binary_chunks(&chunks).expect("chunks should encode as parquet");

        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(encoded))
            .expect("the output should be a readable parquet file")
            .build()
            .expect("the reader should build");

        let mut rows: Vec<Vec<u8>> = Vec::new();
        for batch in reader {
            let batch = batch.expect("each record batch should decode");
            let column = batch
                .column(0)
                .as_any()
                .downcast_ref::<BinaryArray>()
                .expect("the single column should be binary");
            for i in 0..column.len() {
                rows.push(column.value(i).to_vec());
            }
        }

        assert_eq!(rows, chunks, "every chunk should survive the round trip");
    }

    #[test]
    fn encoding_no_chunks_produces_a_readable_file() {
        let encoded = create_parquet_from_binary_chunks(&[]).expect("an empty batch should encode");
        let reader = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(encoded))
            .expect("the output should still be a readable parquet file");
        assert_eq!(reader.metadata().file_metadata().num_rows(), 0);
    }
}
