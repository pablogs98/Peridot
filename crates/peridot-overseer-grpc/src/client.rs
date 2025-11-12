use crate::client::overseer::{RemoveModuleRequest, UpdateMetricsRequest};
use overseer::{overseer_client::OverseerClient, RegisterModuleRequest};
use std::collections::HashMap;
use std::error::Error;
use tokio::runtime::Runtime;

pub mod overseer {
    tonic::include_proto!("overseer");
}
pub struct OverseerGrpcClient {
    client: OverseerClient<tonic::transport::Channel>,
    runtime: Runtime,
}

impl OverseerGrpcClient {
    pub async fn new(address: &str) -> Result<OverseerGrpcClient, Box<dyn Error>> {
        let runtime = Runtime::new().unwrap();
        let address_owned = address.to_owned();

        Ok(OverseerGrpcClient {
            client: OverseerClient::connect(address_owned).await?,
            runtime,
        })
    }

    pub fn register_module(&mut self, module_id: &String, demand: f64) -> Result<(), tonic::Status> {
        let register_request = tonic::Request::new(RegisterModuleRequest { module_id: module_id.clone(), demand });
        let result = self
            .runtime
            .block_on(self.client.register_module(register_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub fn update_metrics(&mut self, module_id: &String, metrics: HashMap<String, f64>) -> HashMap<String, f64> {
        let stats_request = tonic::Request::new(UpdateMetricsRequest { module_id: module_id.clone(), metrics });
        let result = self
            .runtime
            .block_on(self.client.update_metrics(stats_request));
        match result {
            Ok(response) => response.into_inner().metrics,
            Err(_) => HashMap::new(),
        }
    }

    pub fn remove_module(&mut self, module_id: &String) -> Result<(), tonic::Status> {
        let remove_request = tonic::Request::new(RemoveModuleRequest { module_id: module_id.clone() });
        let result = self
            .runtime
            .block_on(self.client.remove_module(remove_request));
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
