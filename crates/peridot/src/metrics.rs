use crate::conf::PeridotConfig;
use peridot_overseer_grpc::client::OverseerGrpcClient;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::{fs, thread, time};
use uuid::Uuid;

///
/// [MetricsPublisher] is responsible for collecting metrics from and sending metrics to the Overseer,
/// and notifying subscribers about metric updates.
///
pub struct MetricsPublisher {
    subscribers: Arc<RwLock<Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>>>,
    producers: Arc<RwLock<Vec<Box<dyn MetricsProducer + Send + Sync>>>>,
    tx: Option<Sender<()>>,
    thread_handle: Option<thread::JoinHandle<()>>,
}

impl MetricsPublisher {
    /// Creates a new [MetricsPublisher] with the given subscribers. Subscribers are items that implement the [MetricsSubscriber] trait.
    /// [MetricsSubscriber]s must be thread-safe and sendable across threads, hence the use of [Arc] and [Mutex].
    pub fn new(
        subscribers: Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>,
        producers: Vec<Box<dyn MetricsProducer + Send + Sync>>,
    ) -> Self {
        MetricsPublisher {
            subscribers: Arc::new(RwLock::new(subscribers)),
            producers: Arc::new(RwLock::new(producers)),
            tx: None,
            thread_handle: None,
        }
    }

    pub fn subscribe_producer(&mut self, producer: Box<dyn MetricsProducer + Send + Sync>) {
        let mut prods = self.producers.write().unwrap();
        prods.push(producer);
    }

    pub fn subscribe(&mut self, subscriber: Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>) {
        let mut subs = self.subscribers.write().unwrap();
        subs.push(subscriber);
    }

    /// Spawns a thread that periodically collects metrics and sends them to the overseer.
    /// Updates items that implement the [MetricsSubscriber] trait with the received metrics.
    pub async fn spawn_metrics_update_thread(&mut self, conf: PeridotConfig) {
        let client = OverseerGrpcClient::new("/run/peridot/overseer.sock").await;
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx = Some(tx);

        let mut client = match client {
            Ok(client) => client,
            Err(e) => {
                log::error!(
                    "Failed to create grpc client, overseer will be unavailable. Error: {}",
                    e.to_string()
                );
                return;
            }
        };

        let module_id = Uuid::new_v4().to_string();

        let demand = conf.io.demand;

        match client.register_module(&module_id, demand) {
            Ok(()) => {
                log::info!("Starting overseer thread. Wasm module ID: {}", &module_id);
                let subscribers = Arc::clone(&self.subscribers);
                let producers = Arc::clone(&self.producers);

                let handle = thread::spawn(move || {
                    loop {
                        if let Ok(()) = rx.try_recv() {
                            log::info!("Overseer thread received shutdown signal.");
                            break;
                        }
                        let mut gathered_metrics = HashMap::new();

                        let prods = producers.read().unwrap();

                        for producer in prods.iter() {
                            producer.produce(&mut gathered_metrics);
                        }

                        MetricsPublisher::update_disk_metrics(&mut gathered_metrics);

                        let received_metrics = client.update_metrics(&module_id, gathered_metrics);
                        if !received_metrics.is_empty() {
                            let subs = subscribers.read().unwrap();
                            for subscriber in subs.iter() {
                                let mut s = subscriber.lock().unwrap();
                                s.update(&received_metrics);
                            }
                        }

                        // more update calls can be added here
                        thread::sleep(time::Duration::from_secs(1));
                    }
                    client.remove_module(&module_id).unwrap();
                });

                self.thread_handle = Some(handle);
            }
            Err(status) => {
                log::error!(
                    "Failed to register module, overseer will be unavailable. Status: {}",
                    status
                );
            }
        }
    }

    /// Stops the overseer thread by sending a shutdown signal and joining the thread.
    pub fn stop_metrics_update_thread(&mut self) {
        if let Some(tx) = self.tx.take() {
            match tx.send(()) {
                Ok(()) => log::info!("Sent shutdown signal to overseer thread."),
                Err(e) => log::error!(
                    "Failed to send shutdown signal to overseer thread: {}",
                    e.to_string()
                ),
            }

            if let Some(handle) = self.thread_handle.take() {
                match handle.join() {
                    Ok(()) => log::info!("Overseer thread has been shut down."),
                    Err(e) => log::error!("Failed to join overseer thread: {:?}", e),
                }
            }
        } else {
            log::warn!("Overseer thread handle not found, cannot shut down thread.");
            return;
        }
    }

    fn update_disk_metrics(metrics_map: &mut HashMap<String, f64>) {
        let pid = std::process::id();
        let path = format!("/proc/{}/io", pid);
        let mut values: HashMap<&str, u64> = HashMap::new();
        let input = fs::read_to_string(Path::new(&path)).unwrap();

        for line in input.lines() {
            let parts: Vec<&str> = line.split(':').map(|s| s.trim()).collect();
            if parts.len() == 2 {
                if let Ok(value) = parts[1].parse::<u64>() {
                    values.insert(parts[0], value);
                }
            }
        }

        metrics_map.insert(
            "read_bytes".to_string(),
            *values.get("read_bytes").unwrap_or(&0) as f64,
        );
        metrics_map.insert(
            "write_bytes".to_string(),
            *values.get("write_bytes").unwrap_or(&0) as f64,
        );
    }
}

/// [MetricsSubscriber] is a trait that defines the behavior of an object that can receive metric updates.
/// [MetricsSubscriber]s subscribe to a [MetricsPublisher] to receive updates.
pub trait MetricsSubscriber {
    /// Updates the subscriber with the given metrics.
    fn update(&mut self, metrics: &HashMap<String, f64>);
}

/// [MetricsProducer] is a trait that defines the behavior of an object that can produce metrics.
/// [MetricsProducer]s can be registered with a [MetricsPublisher] to provide additional metrics
pub trait MetricsProducer {
    fn produce(&self, metrics: &mut HashMap<String, f64>);
}
