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

    async fn update_metrics(
        &self,
        request: Request<UpdateMetricsRequest>,
    ) -> Result<Response<UpdateMetricsResponse>, Status> {
        let inner = request.into_inner();

        // 1) Actualizar received_metrics con UN solo lock
        {
            let mut map = self.received_metrics.lock().await;
            for (key, value) in &inner.metrics {
                map.entry(key.clone()).or_default().push(*value);
            }
        }

        // 2) Leer rates con UN solo lock
        let rate = {
            let rates = self.rates.lock().await;
            match rates.get(&inner.module_id) {
                Some(v) => *v,
                None => {
                    // evita unwrap
                    return Err(Status::not_found("Module ID not found in rates"));
                }
            }
        };

        // 3) Construir respuesta
        Ok(Response::new(UpdateMetricsResponse {
            metrics: HashMap::from([("token_rate".into(), rate)]),
        }))
    }
}
