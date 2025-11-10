use log::info;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex};
use tonic::async_trait;
use tonic::{Request, Response, Status};

pub mod overseer_proto {
    tonic::include_proto!("overseer");
}

use overseer_proto::{
    overseer_server::{Overseer},
    ModuleResponse, RegisterModuleRequest, RemoveModuleRequest, UpdateMetricsRequest, UpdateMetricsResponse
};

#[derive(Default)]
pub struct OverseerService {
    received_metrics: Arc<Mutex<HashMap<String, Vec<f64>>>>,
    _processed_metrics: Arc<Mutex<HashMap<String, f64>>>,

    // IO-specific metrics
    demands: Arc<Mutex<HashMap<String, f64>>>,
    rates: Arc<Mutex<HashMap<String, f64>>>
}

impl OverseerService {
    pub fn new(received_metrics: Arc<Mutex<HashMap<String, Vec<f64>>>>, _processed_metrics: Arc<Mutex<HashMap<String, f64>>>, demands: Arc<Mutex<HashMap<String, f64>>>, rates: Arc<Mutex<HashMap<String, f64>>>) -> Self {
        Self { received_metrics, _processed_metrics, demands, rates}
    }
}

#[async_trait]
impl Overseer for OverseerService {
    async fn register_module(
        &self,
        request: Request<RegisterModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let inner = request.into_inner();
        let module_id = &inner.module_id;
        let demand = inner.demand;
        self.demands.lock().await.insert(module_id.clone(), demand);
        info!("Registered module with PID: {} and demand: {}", module_id, demand);
        Ok(Response::new(ModuleResponse {}))
    }

    async fn remove_module(
        &self,
        request: Request<RemoveModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let module_id = &request.into_inner().module_id;
        self.demands.lock().await.remove(module_id);
        info!("Removed module with PID: {}", module_id);
        Ok(Response::new(ModuleResponse {}))
    }

    async fn update_metrics(&self, request: Request<UpdateMetricsRequest>) -> Result<Response<UpdateMetricsResponse>, Status> {
        let inner = request.into_inner();
        let metrics = &inner.metrics;
        for (key, value) in metrics {
            match self.received_metrics.lock().await.get(key) {
                Some(vec) => {
                    let mut vec = vec.clone();
                    vec.push(*value);
                    self.received_metrics.lock().await.insert(key.clone(), vec);
                }
                None => {
                    self.received_metrics.lock().await.insert(key.clone(), vec![*value]);
                }
            }
        }

        // let metrics : HashMap<String, f64> = metrics.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let mut metrics = HashMap::new();
        metrics.insert("token_rate".to_string(), *self.rates.lock().await.get(&inner.module_id).unwrap());
        Ok(Response::new(UpdateMetricsResponse {metrics}))
    }
}
