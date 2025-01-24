use std::collections::HashMap;
use serde::Deserialize;
use serde_yaml;
use std::fs;
use std::path::Path;

pub struct PeridotConfig {
    pub configs: HashMap<String, PeridotConfigEntry>,
}

#[derive(Deserialize)]
pub struct PeridotConfigEntry {
    pub peridot_config: HashMap<String, u32>,
    pub args: String,
}

impl PeridotConfig {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<PeridotConfig, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let configs: HashMap<String, PeridotConfigEntry> = serde_yaml::from_str(&contents)?;
        Ok(PeridotConfig { configs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let config = PeridotConfig::new("resources/test_config.yaml").unwrap();
        assert_eq!(config.configs.get("io_intensive.wasm").unwrap().peridot_config.get("io_max_bandwidth").unwrap(), &1024);
    }
}
