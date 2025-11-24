use crate::conf::PeridotConfig;
use peridot_overseer_grpc::client::OverseerGrpcClient;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex, RwLock};
use std::{fs, time};
use tokio::task::JoinHandle;
use uuid::Uuid;
use anyhow::{anyhow, Result};
use log::warn;

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

///
/// [MetricsPublisher] is responsible for collecting metrics from and sending metrics to the Overseer,
/// and notifying subscribers about metric updates.
///
pub struct MetricsPublisher {
    subscribers: Arc<RwLock<Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>>>,
    producers: Arc<RwLock<Vec<Arc<dyn MetricsProducer + Send + Sync>>>>,
    tx: Option<Sender<()>>,
    thread_handle: Option<JoinHandle<()>>,
}

impl MetricsPublisher {
    /// Creates a new [MetricsPublisher] with the given subscribers. Subscribers are items that implement the [MetricsSubscriber] trait.
    /// [MetricsSubscriber]s must be thread-safe and sendable across threads, hence the use of [Arc] and [Mutex].
    pub fn new(
        subscribers: Vec<Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>>,
        producers: Vec<Arc<dyn MetricsProducer + Send + Sync>>,
    ) -> Self {
        MetricsPublisher {
            subscribers: Arc::new(RwLock::new(subscribers)),
            producers: Arc::new(RwLock::new(producers)),
            tx: None,
            thread_handle: None,
        }
    }

    pub fn subscribe_producer(&mut self, producer: Arc<dyn MetricsProducer + Send + Sync>) {
        let mut prods = self.producers.write().unwrap();
        prods.push(producer);
    }

    pub fn subscribe(&mut self, subscriber: Arc<Mutex<dyn MetricsSubscriber + Send + Sync>>) {
        let mut subs = self.subscribers.write().unwrap();
        subs.push(subscriber);
    }

    /// Spawns a thread that periodically collects metrics and sends them to the overseer.
    /// Updates items that implement the [MetricsSubscriber] trait with the received metrics.
    pub async fn spawn_metrics_update_thread(&mut self, conf: &PeridotConfig) -> Result<()> {
        let client =
            OverseerGrpcClient::new(conf.overseer_address.as_ref().expect("overseer_address must be set").as_str()).await;
        let (tx, rx) = std::sync::mpsc::channel();
        self.tx = Some(tx);

        let mut client = match client {
            Ok(client) => client,
            Err(e) => {
                let error = format!("Failed to create grpc client: {}", e.to_string());
                return Err(anyhow!(error));
            }
        };

        let module_id = Uuid::new_v4().to_string();

        let demand = conf.io.demand;

        match client.register_module(&module_id, demand).await {
            Ok(()) => {
                log::info!("Starting overseer thread. Wasm module ID: {}", &module_id);
                let subscribers = Arc::clone(&self.subscribers);
                let producers = Arc::clone(&self.producers);

                let handle = tokio::spawn(async move {
                    loop {
                        if let Ok(()) = rx.try_recv() {
                            log::info!("Overseer thread received shutdown signal.");
                            break;
                        }

                        // gather metrics without holding the lock across .await
                        println!("Overseer gathering metrics from producers.");
                        let gathered_metrics = {
                            let prods = producers.read().unwrap();
                            let mut metrics = HashMap::new();
                            for producer in prods.iter() {
                                producer.produce(&mut metrics);
                                println!("Overseer gathered metrics: {:?}", &metrics);
                            }
                            metrics
                        }; // <-- lock released here

                        let received_metrics =
                            client.update_metrics(&module_id, gathered_metrics).await;

                        if !received_metrics.is_empty() {
                            let subs = subscribers.read().unwrap();
                            for subscriber in subs.iter() {
                                let mut s = subscriber.lock().unwrap();
                                warn!(
                                    "Overseer updating subscriber with metrics: {:?}",
                                    &received_metrics
                                );
                                s.update(&received_metrics);
                            }
                        } else {
                            warn!(
                                "No metrics received from overseer for module ID: {}",
                                &module_id
                            );
                        }

                        tokio::time::sleep(time::Duration::from_secs(1)).await;
                    }

                    client.remove_module(&module_id).await.unwrap();
                });

                self.thread_handle = Some(handle);
                Ok(())
            }
            Err(status) => {
                Err(anyhow!(
                    "Failed to register module. Status: {}",
                    status
                ))
            }
        }
    }

    /// Stops the overseer thread by sending a shutdown signal and joining the thread.
    pub async fn stop_metrics_update_thread(&mut self) {
        if let Some(tx) = self.tx.take() {
            match tx.send(()) {
                Ok(()) => log::info!("Sent shutdown signal to overseer thread."),
                Err(e) => log::error!(
                    "Failed to send shutdown signal to overseer thread: {}",
                    e.to_string()
                ),
            }

            if let Some(handle) = self.thread_handle.take() {
                match handle.await {
                    Ok(_) => log::info!("Overseer thread has been shut down."),
                    Err(e) => log::error!("Failed to join overseer task: {:?}", e),
                }
            }
        } else {
            log::warn!("Overseer thread handle not found, cannot shut down thread.");
            return;
        }
    }
}

pub struct DiskIOMetricsProducer;

impl MetricsProducer for DiskIOMetricsProducer {
    fn produce(&self, metrics: &mut HashMap<String, f64>) {
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

        metrics.insert(
            "read_bytes".to_string(),
            *values.get("read_bytes").unwrap_or(&0) as f64,
        );
        metrics.insert(
            "write_bytes".to_string(),
            *values.get("write_bytes").unwrap_or(&0) as f64,
        );
    }
}
