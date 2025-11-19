use crate::client::overseer::{RemoveModuleRequest, UpdateMetricsRequest};
use overseer::{overseer_client::OverseerClient, RegisterModuleRequest};
use std::collections::HashMap;

pub mod overseer {
    tonic::include_proto!("overseer");
}
pub struct OverseerGrpcClient {
    client: OverseerClient<tonic::transport::Channel>,
}

impl OverseerGrpcClient {
    pub async fn new(address: &str) -> Result<OverseerGrpcClient, tonic::transport::Error> {
        let address_owned = address.to_owned();

        println!("Connecting to Overseer at address: {}", address_owned);

        Ok(OverseerGrpcClient {
            client: OverseerClient::connect(address_owned).await?,
        })
    }

    pub async fn register_module(&mut self, module_id: &String, demand: f64) -> Result<(), tonic::Status> {
        println!("Registering module with ID: {} and demand: {}", module_id, demand);
        let register_request = tonic::Request::new(RegisterModuleRequest { module_id: module_id.clone(), demand });

        let result =self.client.register_module(register_request).await;
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    pub async fn update_metrics(&mut self, module_id: &String, metrics: HashMap<String, f64>) -> HashMap<String, f64> {
        let stats_request = tonic::Request::new(UpdateMetricsRequest { module_id: module_id.clone(), metrics });
        println!("Updating metrics for module ID: {} with metrics: {:?}", module_id, stats_request.get_ref().metrics);
        let result = self.client.update_metrics(stats_request).await;
        match result {
            Ok(response) => {
                let updated_metrics = response.into_inner().metrics;
                println!("Updated metrics: {:?}", updated_metrics);
                updated_metrics
            }
            // print error and return empty metrics on failure
            Err(_) => {
                println!("Failed to update metrics for module ID: {}", module_id);
                HashMap::new()
            }
        }
    }

    pub async fn remove_module(&mut self, module_id: &String) -> Result<(), tonic::Status> {
        let remove_request = tonic::Request::new(RemoveModuleRequest { module_id: module_id.clone() });
        let result = self.client.remove_module(remove_request).await;
        match result {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }
}
