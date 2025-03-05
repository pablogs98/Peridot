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
    ModuleResponse, RegisterModuleRequest, RemoveModuleRequest, UpdateMaxBandwidthRequest,
    UpdateMaxBandwidthResponse,
};

#[derive(Default)]
pub struct OverseerService {
    rates: Arc<Mutex<HashMap<u32, f64>>>,
    demands: Arc<Mutex<HashMap<u32, f64>>>
}

impl OverseerService {
    pub fn new(demands: Arc<Mutex<HashMap<u32, f64>>>, rates: Arc<Mutex<HashMap<u32, f64>>>) -> Self {
        Self { demands, rates }
    }
}

#[async_trait]
impl Overseer for OverseerService {
    async fn register_module(
        &self,
        request: Request<RegisterModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let inner = request.into_inner();
        let pid = inner.pid;
        let demand = inner.demand;
        self.demands.lock().await.insert(pid, demand);
        info!("Registered module with PID: {}", pid);
        Ok(Response::new(ModuleResponse {}))
    }

    async fn remove_module(
        &self,
        request: Request<RemoveModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let pid = request.into_inner().pid;
        self.demands.lock().await.remove(&pid);
        info!("Removed module with PID: {}", pid);
        Ok(Response::new(ModuleResponse {}))
    }

    async fn update_max_bandwidth(
        &self,
        request: Request<UpdateMaxBandwidthRequest>,
    ) -> Result<Response<UpdateMaxBandwidthResponse>, Status> {
        let pids: Vec<u32> = request.into_inner().pids;
        let mut stats: HashMap<u32, f64> = HashMap::new();

        for pid in pids {
            if let Some(rate) = self.rates.lock().await.get(&pid) {
                stats.insert(pid, *rate);
            }
        }

        let response = UpdateMaxBandwidthResponse { stats };
        Ok(Response::new(response))
    }
}
