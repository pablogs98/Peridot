use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
pub struct PeridotConfig {
    pub args: Vec<String>,
    pub io: IoConfig,
    pub cpu: CpuConfig,
}

#[derive(Debug, Deserialize)]
pub struct IoConfig {
    pub demand: f64,
    pub max_bandwidth: f64, // optional
}

#[derive(Debug, Deserialize)]
pub struct CpuConfig {
    pub demand: f64,
    pub utilization: f64,
}

#[derive(Debug, Deserialize)]
pub struct ModuleConfig {
    pub args: Vec<String>,
    pub peridot_config: PeridotConfigInner,
}

#[derive(Debug, Deserialize)]
pub struct PeridotConfigInner {
    pub priority: u32,
    pub demand: f64,
}

impl PeridotConfig {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<PeridotConfig, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let config: PeridotConfig = serde_yaml::from_str(&contents)?;
        Ok(config)
    }
}

