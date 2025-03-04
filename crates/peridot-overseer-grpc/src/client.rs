use hyper_util::rt::TokioIo;
use overseer::{overseer_client::OverseerClient, RegisterModuleRequest, UpdateMaxBandwidthRequest};
use std::collections::HashMap;
use tokio::net::UnixStream;
use tokio::runtime::Runtime;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;
use crate::client::overseer::RemoveModuleRequest;

pub mod overseer {
    tonic::include_proto!("overseer");
}
pub struct OverseerGrpcClient {
    client: OverseerClient<tonic::transport::Channel>,
    runtime: Runtime,
}

impl OverseerGrpcClient {
    pub fn new(address: &str) -> OverseerGrpcClient {
        let runtime = Runtime::new().unwrap();
        let channel = runtime
            .block_on(
                Endpoint::try_from("http://[::]:50051")
                    .unwrap()
                    .connect_with_connector(service_fn(|_: Uri| async {
                        Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(address).await?))
                    })),
            )
            .unwrap();
        OverseerGrpcClient {
            client: OverseerClient::new(channel),
            runtime,
        }
    }

    pub fn register_module(&mut self, pid: u32, demand: f64) -> Result<(), tonic::Status> {
        let register_request = tonic::Request::new(RegisterModuleRequest {
            pid,
            demand,
        });
        let result = self
            .runtime
            .block_on(self.client.register_module(register_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn update_max_bandwidth(&mut self, pids: Vec<u32>) -> HashMap<u32, f64> {
        let stats_request = tonic::Request::new(UpdateMaxBandwidthRequest { pids });
        let result = self
            .runtime
            .block_on(self.client.update_max_bandwidth(stats_request));
        match result {
            Ok(response) => response.into_inner().stats,
            Err(_) => HashMap::new(),
        }
    }

    pub fn remove_module(&mut self, pid: u32) -> Result<(), tonic::Status> {
        let remove_request = tonic::Request::new(RemoveModuleRequest { pid });
        let result = self
            .runtime
            .block_on(self.client.remove_module(remove_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
