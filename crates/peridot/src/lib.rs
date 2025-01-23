use std::collections::HashMap;
use serde::Deserialize;
use serde_yaml;
use std::fs;
use std::path::Path;

#[derive(Deserialize)]
pub struct PeridotConfig {
    pub configs: HashMap<String, HashMap<String, u32>>,
}

impl PeridotConfig {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<PeridotConfig, Box<dyn std::error::Error>> {
        let contents = fs::read_to_string(path)?;
        let configs: HashMap<String, HashMap<String, u32>> = serde_yaml::from_str(&contents)?;
        Ok(PeridotConfig { configs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let config = PeridotConfig::new("resources/test_config.yaml").unwrap();
        assert_eq!(config.configs.get("io_intensive.wasm").unwrap().get("io_max_bandwidth").unwrap(), &1024);
    }
}
