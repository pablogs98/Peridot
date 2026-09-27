use std::{collections::HashSet, fs, sync::Arc, time::Duration};

use arrow::{
    array::{ArrayRef, BinaryArray},
    datatypes::{DataType, Field, Schema},
    record_batch::RecordBatch,
};
use aws_sdk_s3::{Client};
use parquet::{arrow::ArrowWriter, file::properties::WriterProperties};
use uuid::Uuid;
use tokio::{runtime::Runtime, task::JoinHandle, time::sleep};
use tracing::{info, Level};
use tracing_subscriber::FmtSubscriber;

use std::io::Cursor;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// Nombre del bucket S3
    #[arg(long)]
    bucket: String,

    /// Tamaño del batch (# de archivos)
    #[arg(long, default_value_t = 10)]
    batch_size: usize,

    /// Directorio a observar
    #[arg(long, default_value = "./data")]
    input_dir: String,
}

struct Batcher {
    pending_batches: Vec<Vec<u8>>,
    batch_size: usize,
    seen_files: HashSet<String>,
    s3: Client,
    bucket: String,
    tokio_runtime: Arc<Runtime>,
    futures: Vec<JoinHandle<()>>,
}

impl Batcher {
    pub fn new(batch_size: usize, s3: Client, bucket: String, runtime: Arc<Runtime>) -> Self {
        Self {
            pending_batches: Vec::new(),
            batch_size,
            seen_files: HashSet::new(),
            s3,
            bucket,
            tokio_runtime: runtime,
            futures: vec![],
        }
    }

    pub fn write_file_to_batch(&mut self, content: Vec<u8>) {
        self.pending_batches.push(content);

        info!(
            "Added file to batch, current batch size: {}",
            self.pending_batches.len()
        );

        if self.pending_batches.len() >= self.batch_size {
            let files_to_upload = self.pending_batches.drain(..).collect::<Vec<_>>();
            // A u16 drawn at random collides often enough to matter: at 128
            // batches the chance of at least one collision is about 12%, and a
            // collision silently overwrites an already-uploaded batch. Same
            // scheme as peridot-batch-ctx, so both arms of the comparison name
            // their objects the same way.
            let key = format!("batch_{}.parquet", Uuid::new_v4());

            let compressed = create_parquet_from_binary_chunks(&files_to_upload);

            let local_file_path = key.clone();
            std::fs::write(&local_file_path, &compressed)
                .expect("Failed to write local parquet file");

            info!(
                "Uploading batch with {} files to S3 with key: {}",
                files_to_upload.len(),
                key
            );

            let client = self.s3.clone();
            let bucket = self.bucket.clone();
            let compressed_clone = compressed.clone();
            self.futures.push(self.tokio_runtime.spawn(async move {
                let _ = client
                    .put_object()
                    .bucket(bucket)
                    .key(key)
                    .body(aws_sdk_s3::primitives::ByteStream::from(compressed_clone))
                    .send()
                    .await;
            }));
        }
    }

    pub fn scan_and_process(&mut self, dir: &str) {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            if !fname.starts_with("tensor_") || !fname.ends_with(".bin") {
                continue;
            }

            if self.seen_files.contains(fname) {
                continue;
            }

            if let Ok(content) = fs::read(&path) {
                self.write_file_to_batch(content);
                self.seen_files.insert(fname.to_string());
            }
        }
    }
}

pub fn create_parquet_from_binary_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let schema = Arc::new(Schema::new(vec![
        Field::new("binary_data", DataType::Binary, false),
    ]));

    let binary_refs: Vec<&[u8]> = chunks.iter().map(|v| v.as_slice()).collect();
    let array = Arc::new(BinaryArray::from(binary_refs)) as ArrayRef;

    let batch =
        RecordBatch::try_new(schema.clone(), vec![array]).expect("Failed to create RecordBatch");

    let mut buffer = Cursor::new(Vec::new());
    let props = WriterProperties::builder().build();
    let mut writer =
        ArrowWriter::try_new(&mut buffer, schema.clone(), Some(props)).expect("Writer failed");

    writer.write(&batch).unwrap();
    writer.close().unwrap();

    buffer.into_inner()
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let subscriber = FmtSubscriber::builder().with_max_level(Level::INFO).finish();
    tracing::subscriber::set_global_default(subscriber).expect("Failed to set logger");

    let shared_rt = Arc::new(Runtime::new().unwrap());
    let config = aws_config::load_from_env().await;
    // See the note in peridot-s3-ctx: the SDK addresses buckets
    // virtual-hosted-style by default, which a self-hosted S3-compatible store
    // reached by hostname cannot serve. Off by default, so Amazon S3 is
    // unaffected.
    let force_path_style = std::env::var("AWS_S3_FORCE_PATH_STYLE")
        .map(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let s3_client = if force_path_style {
        Client::from_conf(
            aws_sdk_s3::config::Builder::from(&config)
                .force_path_style(true)
                .build(),
        )
    } else {
        Client::new(&config)
    };

    let mut batcher = Batcher::new(args.batch_size, s3_client, args.bucket.clone(), shared_rt);

    loop {
        batcher.scan_and_process(&args.input_dir);
        sleep(Duration::from_secs(2)).await;
    }
}

