use log::{debug, info, warn};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{fs, time};
use tonic::async_trait;
use tonic::{transport::Server, Request, Response, Status};
use peridot_overseer_grpc::service::overseer_proto::overseer_server::OverseerServer;
use peridot_overseer_grpc::service::OverseerService;

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

async fn update_io_stats(io_stats_per_second: Arc<Mutex<HashMap<u32, (Vec<i32>, Vec<i32>)>>>, interval: u64) {
    let mut io_stats: IoStatsMap = IoStatsMap::default();
    let mut interval = time::interval(Duration::from_secs(interval));

    loop {
        let keys;
        {
            keys = &io_stats_per_second.lock().unwrap().keys();
        }
        interval.tick().await;
        for &pid in keys {
            if let Some(stats) = read_io_stats(pid).await {
                let curr_read_bytes = stats.read_bytes;
                let curr_write_bytes = stats.write_bytes;
                if let Some(prev_stats) = io_stats.get(&pid) {
                    let read_bytes = curr_read_bytes - prev_stats.read_bytes;
                    let write_bytes = curr_write_bytes - prev_stats.write_bytes;
                    {
                        let mut io_stats_per_second_locked = &io_stats_per_second.lock().unwrap();
                        io_stats_per_second_locked.get_mut(&pid).unwrap().0.push(read_bytes);
                        io_stats_per_second_locked.get_mut(&pid).unwrap().1.push(write_bytes);
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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().collect::<Vec<String>>();
    let addr = args
        .get(1)
        .unwrap_or(&String::from("0.0.0.0:50051"))
        .parse()?;

    let io_stats_per_second = Arc::new(Mutex::new(HashMap::new()));

    // Spawn the periodic task
    tokio::spawn(update_io_stats(io_stats_per_second.clone(), 1));
    let service = OverseerService::new(io_stats_per_second.clone());


    info!("Overseer running on {}", addr);

    let server_handler = Server::builder()
        .add_service(OverseerServer::new(service))
        .serve(addr);

    Ok(())
}
