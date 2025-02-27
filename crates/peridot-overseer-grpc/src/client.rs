use std::collections::HashMap;
use tokio::net::UnixStream;
use overseer::{overseer_client::OverseerClient, ModuleRequest, UpdateIoStatsRequest};
use tokio::runtime::Runtime;
use tonic::transport::{Endpoint, Uri};
use tower::service_fn;
use hyper_util::rt::TokioIo;

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
            .block_on(Endpoint::try_from("http://[::]:50051").unwrap()
                .connect_with_connector(service_fn(|_: Uri| async {
                    let path = address;
                    Ok::<_, std::io::Error>(TokioIo::new(UnixStream::connect(path).await?))
                })))
            .unwrap();
        OverseerGrpcClient {
            client: OverseerClient::new(channel),
            runtime,
        }
    }

    pub fn register_module(&mut self, pid: u32) -> Result<(), tonic::Status> {
        let register_request = tonic::Request::new(ModuleRequest { pid });
        let result = self
            .runtime
            .block_on(self.client.register_module(register_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn update_io_stats(&mut self, pid: u32) -> HashMap<u32, i32> {
        let stats_request = tonic::Request::new(UpdateIoStatsRequest { pids: vec![pid] });
        let result = self
            .runtime
            .block_on(self.client.update_io_stats(stats_request));
        match result {
            Ok(response) => response.into_inner().stats,
            Err(_) => HashMap::new(),
        }
    }

    pub fn remove_module(&mut self, pid: u32) -> Result<(), tonic::Status> {
        let remove_request = tonic::Request::new(ModuleRequest { pid });
        let result = self
            .runtime
            .block_on(self.client.remove_module(remove_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
