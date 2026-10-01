mod policy;

use crate::policy::MinMaxFairShare;
use clap::Parser;
use log::info;
use peridot_overseer_grpc::service::overseer_proto::overseer_server::OverseerServer;
use peridot_overseer_grpc::service::OverseerService;
use std::collections::HashMap;
use std::error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tokio::{time};
use tonic::transport::Server;

/// Peridot Overseer
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// TCP port to listen on
    #[arg(required = true)]
    port: u16,

    /// Address to bind to.
    ///
    /// Defaults to loopback, which is right when the modules share a host
    /// with the Overseer. Modules on other hosts -- the two-node arrangement
    /// the paper's scalability experiment uses, or a containerised deployment
    /// where the shim runs the Overseer elsewhere -- cannot reach a loopback
    /// socket, so pass `--bind 0.0.0.0` there. There is no authentication on
    /// this interface, so bind it to a reachable address only on a network
    /// you trust.
    #[arg(short, long, default_value = "127.0.0.1")]
    bind: String,

    /// Max I/O bandwidth (B/s)
    #[arg(required = true)]
    max_bandwidth: f64,

    /// Update interval (s)
    #[arg(short, long, default_value_t = 1)]
    update_interval: u64,

    /// Log level (e.g. debug, info, warn, error). Defaults to "info".
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn error::Error>> {
    let args = Args::parse();

    if std::env::var_os("RUST_LOG").is_none() {
        std::env::set_var("RUST_LOG", "debug");
    }
    env_logger::init();

    let addr: std::net::SocketAddr = format!("{}:{}", args.bind, args.port)
        .parse()
        .map_err(|e| format!("invalid bind address '{}:{}': {e}", args.bind, args.port))?;
    let max_bandwidth = args.max_bandwidth;
    let update_interval = args.update_interval;

    let rates: Arc<Mutex<HashMap<String, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let demands: Arc<Mutex<HashMap<String, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let received_metrics: Arc<Mutex<HashMap<String, Vec<f64>>>> = Arc::new(Mutex::new(HashMap::new()));
    let processed_metrics: Arc<Mutex<HashMap<String, f64>>> = Arc::new(Mutex::new(HashMap::new()));
    let end_thread = Arc::new(AtomicBool::new(false));

    let update_io_stats_future = tokio::spawn(update_io_stats(
        Arc::clone(&received_metrics),
        Arc::clone(&processed_metrics),
        Arc::clone(&rates),
        Arc::clone(&demands),
        max_bandwidth,
        update_interval,
        end_thread.clone(),
    ));

    let service = OverseerService::new(
        Arc::clone(&received_metrics),
        Arc::clone(&processed_metrics),
        Arc::clone(&demands),
        Arc::clone(&rates),
    );

    info!("Overseer running on {}", addr);

    Server::builder()
        .add_service(OverseerServer::new(service))
        .serve_with_shutdown(addr, grpc_sigint(end_thread.clone()))
        .await?;

    end_thread.store(true, Ordering::Relaxed);
    update_io_stats_future
        .await?
        .expect("Error joining I/O stats update task");

    Ok(())
}

/// Handle SIGINT for graceful shutdown
async fn grpc_sigint(end_threads: Arc<AtomicBool>) {
    let _ = signal(SignalKind::interrupt())
        .expect("Failed to create a new SIGINT signal handler for gRPC")
        .recv()
        .await;

    end_threads.store(true, Ordering::Relaxed);
    info!("gRPC shutdown complete");
}

async fn update_io_stats(
    received_metrics: Arc<Mutex<HashMap<String, Vec<f64>>>>,
    _processed_metrics: Arc<Mutex<HashMap<String, f64>>>,
    rates: Arc<Mutex<HashMap<String, f64>>>,
    demands: Arc<Mutex<HashMap<String, f64>>>,
    max_bandwidth: f64,
    interval: u64,
    end_thread: Arc<AtomicBool>,
) -> Result<(), Box<dyn error::Error + Send + Sync>> {
    let mut interval = time::interval(Duration::from_secs(interval));
    let policy = MinMaxFairShare::new(max_bandwidth, Arc::clone(&demands), Arc::clone(&rates));

    received_metrics.lock().await.clear();

    while !end_thread.load(Ordering::Relaxed) {
        interval.tick().await;
        policy.allocate_bandwidth().await;
    }

    Ok(())
}
