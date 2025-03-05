use std::collections::HashMap;
use serde::Deserialize;
use serde_yaml;
use std::fs;
use std::path::Path;


pub type PeridotConfig = HashMap<String, PeridotConfigEntry>;

#[derive(Deserialize)]
pub struct PeridotConfigEntry {
    pub peridot_config: HashMap<String, f64>,
    pub args: Vec<String>,
}
pub fn new_config<P: AsRef<Path>>(path: P) -> Result<PeridotConfig, Box<dyn std::error::Error>> {
    let contents = fs::read_to_string(path)?;
    let config: PeridotConfig = serde_yaml::from_str(&contents)?;
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let config = new_config("../resources/example_config.yaml").unwrap();
        assert_eq!(config.get("io_intensive.wasm").unwrap().peridot_config.get("io_max_bandwidth").unwrap(), &1024.0);
    }
}
