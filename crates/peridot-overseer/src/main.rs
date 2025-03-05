mod policy;

use crate::policy::MinMaxFairShare;
use clap::Parser;
use log::{info, warn};
use peridot_overseer_grpc::service::overseer_proto::overseer_server::OverseerServer;
use peridot_overseer_grpc::service::OverseerService;
use std::collections::HashMap;
use std::error;
use std::fs::remove_file;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixListener;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tokio::time::Instant;
use tokio::{fs, time};
use tonic::codegen::tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;

/// Peridot Overseer
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Unix Domain Socket (UDS) address (e.g. /tmp/peridot.sock)
    #[arg(required = true)]
    address: String,

    /// Max I/O bandwidth (B/s)
    #[arg(required = true)]
    max_bandwidth: f64,

    /// Update interval (s)
    #[arg(short, long, default_value_t = 1)]
    update_interval: u64,

    /// Log level (e.g. debug, info, warn, error). Defaults to "info".
    /// Overridden by RUST_LOG env var if set.
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

struct IoStats {
    read_bytes: u64,
    write_bytes: u64,
}

type IoStatsMap = HashMap<u32, IoStats>;

async fn read_io_stats(pid: u32) -> Option<IoStats> {
    let path = format!("/proc/{}/io", pid);
    let mut values: HashMap<&str, u64> = HashMap::new();

    let input = fs::read_to_string(Path::new(&path)).await;
    match input {
        Ok(input) => {
            for line in input.lines() {
                let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
                if parts.len() == 2 {
                    if let Ok(value) = parts[1].parse::<u64>() {
                        values.insert(parts[0], value);
                    }
                }
            }
            Some(IoStats {
                read_bytes: *values.get("read_bytes").unwrap_or(&0),
                write_bytes: *values.get("write_bytes").unwrap_or(&0),
            })
        }
        Err(_) => None,
    }
}

async fn update_io_stats(
    rates: Arc<Mutex<HashMap<u32, f64>>>,
    demands: Arc<Mutex<HashMap<u32, f64>>>,
    max_bandwidth: f64,
    interval: u64,
    end_thread: Arc<AtomicBool>,
) -> Result<(), Box<dyn error::Error + Send + Sync>> {
    let mut io_stats: IoStatsMap = IoStatsMap::default();
    let mut interval = time::interval(Duration::from_secs(interval));
    let policy = MinMaxFairShare::new(max_bandwidth, demands.clone(), rates.clone());
    let mut file = File::create("/tmp/io_stats.txt").await?;
    file.write("timestamp_ms,pid,read_bytes,write_bytes\n".as_bytes())
        .await?;
    let start = Instant::now();

    while !end_thread.load(Ordering::Relaxed) {
        let demands_lock = demands.lock().await;
        let keys = demands_lock.keys().cloned().collect::<Vec<_>>();
        drop(demands_lock);

        interval.tick().await;
        policy.allocate_bandwidth().await;
        let mut to_remove = Vec::new();
        for pid in keys {
            if let Some(stats) = read_io_stats(pid).await {
                let curr_read_bytes = stats.read_bytes;
                let curr_write_bytes = stats.write_bytes;
                if let Some(prev_stats) = io_stats.get(&pid) {
                    let read_bytes = curr_read_bytes - prev_stats.read_bytes;
                    let write_bytes = curr_write_bytes - prev_stats.write_bytes;
                    let timestamp = start.elapsed().as_millis();
                    file.write_all(
                        format!("{},{},{},{}\n", timestamp, pid, read_bytes, write_bytes)
                            .as_bytes(),
                    )
                    .await
                    .unwrap();
                }
                io_stats.insert(pid, stats);
            } else {
                warn!("Failed to read io stats for PID: {}. Removing task.", pid);
                to_remove.push(pid);
            }
        }
        for pid in to_remove {
            demands.lock().await.remove(&pid);
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let args = Args::parse();

    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let addr = &args.address;
    let max_bandwidth = args.max_bandwidth;
    let update_interval = args.update_interval;

    let rates: Arc<Mutex<HashMap<u32, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let demands: Arc<Mutex<HashMap<u32, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let end_thread = Arc::new(AtomicBool::new(false));

    let update_io_stats_future = tokio::spawn(update_io_stats(
        rates.clone(),
        demands.clone(),
        max_bandwidth,
        update_interval,
        end_thread.clone(),
    ));
    let service = OverseerService::new(demands.clone(), rates.clone());
    let uds = UnixListener::bind(addr)?;
    let uds_stream = UnixListenerStream::new(uds);

    info!("Overseer running on {}", addr);

    Server::builder()
        .add_service(OverseerServer::new(service))
        .serve_with_incoming_shutdown(uds_stream, grpc_sigint(end_thread.clone()))
        .await?;

    end_thread.store(true, Ordering::Relaxed);
    update_io_stats_future
        .await?
        .expect("Error joining I/O stats update task");
    remove_file(addr)?;
    Ok(())
}

async fn grpc_sigint(end_threads: Arc<AtomicBool>) {
    let _ = signal(SignalKind::interrupt())
        .expect("Failed to create a new SIGINT signal handler for gRPC")
        .recv()
        .await;

    end_threads.store(true, Ordering::Relaxed);
    info!("gRPC shutdown complete");
}
