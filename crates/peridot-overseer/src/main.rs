mod policy;

use crate::policy::MinMaxFairShare;
use log::{debug, error, info, warn};
use peridot_overseer_grpc::service::overseer_proto::overseer_server::OverseerServer;
use peridot_overseer_grpc::service::OverseerService;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::UnixListener;
use tokio::sync::Mutex;
use tokio::{fs, time};
use tonic::async_trait;
use tonic::codegen::tokio_stream::wrappers::UnixListenerStream;
use tonic::{transport::Server, Request, Response, Status};

struct IoStats {
    read_bytes: i32,
    write_bytes: i32,
}

type IoStatsMap = HashMap<u32, IoStats>;

async fn read_io_stats(pid: u32) -> Option<IoStats> {
    let path = format!("/proc/{}/io", pid);
    let mut values: HashMap<&str, u64> = HashMap::new();

    let input = fs::read_to_string(Path::new(&path));
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
            IoStats {
                read_bytes: *values.get("read_bytes").unwrap_or(&0),
                write_bytes: *values.get("write_bytes").unwrap_or(&0),
            }
        }
        Err(_) => None,
    };
}

async fn update_io_stats(
    rates: Arc<Mutex<HashMap<u32, f64>>>,
    demands: Arc<Mutex<HashMap<u32, f64>>>,
    max_bandwidth: f64,
    interval: u64,
) {
    let mut io_stats: IoStatsMap = IoStatsMap::default();
    let mut interval = time::interval(Duration::from_secs(interval));
    let policy = MinMaxFairShare::new(max_bandwidth, demands.clone(), rates.clone());

    loop {
        let keys;
        {
            keys = &demands.lock().await.keys();
        }
        interval.tick().await;
        policy.allocate_bandwidth().await;

        for &pid in keys {
            if let Some(stats) = read_io_stats(pid).await {
                let curr_read_bytes = stats.read_bytes;
                let curr_write_bytes = stats.write_bytes;
                if let Some(prev_stats) = io_stats.get(&pid) {
                    let read_bytes = curr_read_bytes - prev_stats.read_bytes;
                    let write_bytes = curr_write_bytes - prev_stats.write_bytes;
                    {
                        debug!("PID: {}, Read: {}, Write: {}", pid, read_bytes, write_bytes);
                    }
                }
                io_stats.insert(pid, stats);
            } else {
                warn!("Failed to read io stats for PID: {}", pid);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let args = std::env::args().collect::<Vec<String>>();
    if args.len() < 4 {
        error!(
            "Usage: {} <uds_listen_address> <max_bandwidth> <update_interval>",
            args[0]
        );
        std::process::exit(1);
    }
    let addr = args.get(1).unwrap();
    let max_bandwidth = args.get(2).unwrap().parse::<f64>().unwrap();
    let update_interval = args.get(3).unwrap().parse::<u64>().unwrap();

    let rates: Arc<Mutex<HashMap<u32, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let demands: Arc<Mutex<HashMap<u32, f64>>> = Arc::new(Mutex::new(HashMap::new()));

    tokio::spawn(update_io_stats(
        rates.clone(),
        demands.clone(),
        max_bandwidth,
        update_interval,
    ));
    let service = OverseerService::new(demands.clone(), rates.clone());
    let uds = UnixListener::bind(addr)?;
    let uds_stream = UnixListenerStream::new(uds);

    info!("Overseer running on {}", addr);

    Server::builder()
        .add_service(OverseerServer::new(service))
        .serve_with_incoming(uds_stream)
        .await?;

    Ok(())
}
