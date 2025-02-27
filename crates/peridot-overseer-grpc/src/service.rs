use log::info;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::{fs, time};
use tonic::async_trait;
use tonic::{transport::Server, Request, Response, Status};

pub mod overseer_proto {
    tonic::include_proto!("overseer");
}

use overseer_proto::{
    overseer_server::{Overseer, OverseerServer},
    RegisterModuleRequest, RemoveModuleRequest, ModuleResponse, UpdateIoStatsRequest, UpdateIoStatsResponse,
};

#[derive(Default)]
pub struct OverseerService {
    io_stats: Arc<Mutex<HashMap<u32, (Vec<i32>, Vec<i32>)>>>,
}

impl OverseerService {
    pub fn new(io_stats: Arc<Mutex<HashMap<u32, (Vec<i32>, Vec<i32>)>>>) -> Self {
        Self { io_stats }
    }
}

#[async_trait]
impl Overseer for OverseerService {
    async fn register_module(
        &self,
        request: Request<RegisterModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let pid = request.into_inner().pid;
        let mut io_stats = self.io_stats.lock().unwrap();
        io_stats.insert(pid, (Vec::new(), Vec::new()));
        info!("Registered module with PID: {}", pid);
        Ok(Response::new(ModuleResponse {}))
    }

    async fn remove_module(
        &self,
        request: Request<RemoveModuleRequest>,
    ) -> Result<Response<ModuleResponse>, Status> {
        let pid = request.into_inner().pid;
        let mut io_stats = self.io_stats.lock().unwrap();
        io_stats.remove(&pid);
        info!("Removed module with PID: {}", pid);
        Ok(Response::new(ModuleResponse {}))
    }

    // change with update_token_bucket rate
    async fn update_max_bandwidth(
        &self,
        request: Request<UpdateIoStatsRequest>,
    ) -> Result<Response<UpdateIoStatsResponse>, Status> {
        let pids = request.into_inner().pids;
        let stats = Vec::new();
        let response = UpdateIoStatsResponse {
            stats: stats.into_iter().collect(),
        };
        Ok(Response::new(response))
    }
}
