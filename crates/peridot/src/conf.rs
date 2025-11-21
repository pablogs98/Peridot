use serde::Deserialize;
use std::fs;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Deserialize, Clone)]
pub struct PeridotConfig {
    pub overseer_address: Option<String>,
    pub args: Vec<String>,
    pub io: IoConfig,
    pub cpu: CpuConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct IoConfig {
    pub demand: f64,
    pub max_bandwidth: f64, // optional
}

#[derive(Debug, Deserialize, Clone)]
pub struct CpuConfig {
    pub demand: f64,
    pub utilization: f64,
}

#[derive(Debug, Deserialize)]
pub struct PeridotConfigInner {
    pub priority: u32,
    pub demand: f64,
}

impl PeridotConfig {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<PeridotConfig> {
        let contents = fs::read_to_string(path)?;
        let config: PeridotConfig = serde_yaml::from_str(&contents)?;
        Ok(config)
    }
}

