use serde::Deserialize;
use std::fs;
use std::path::Path;
use anyhow::Result;

/// One entry in the `contexts` list.
///
/// Accepts either a bare name or a name with a context-specific `config` block:
///
/// ```yaml
/// contexts:
///   - counter
///   - name: batch
///     config: { batch_size: 100, bucket: peridot }
/// ```
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ContextSpec {
    Name(String),
    Detailed {
        name: String,
        #[serde(default)]
        config: serde_yaml::Value,
    },
}

impl ContextSpec {
    pub fn name(&self) -> &str {
        match self {
            ContextSpec::Name(name) => name,
            ContextSpec::Detailed { name, .. } => name,
        }
    }

    /// This entry's own `config` block, or `Null` when it has none.
    pub fn config(&self) -> &serde_yaml::Value {
        const NULL: &serde_yaml::Value = &serde_yaml::Value::Null;
        match self {
            ContextSpec::Name(_) => NULL,
            ContextSpec::Detailed { config, .. } => config,
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PeridotConfig {
    pub overseer_address: Option<String>,
    pub args: Vec<String>,
    /// Context chain to build, outermost first. The first name listed sees each hostcall
    /// before the ones after it. Absent or empty means plain WASI with no interception.
    #[serde(default)]
    pub contexts: Vec<ContextSpec>,
    /// Plugin libraries to load before building the chain. Each is a `cdylib` built with
    /// `peridot::export_peridot_plugin!`, and the contexts it registers become usable in
    /// `contexts` above without rebuilding the runtime.
    #[serde(default)]
    pub plugins: Vec<std::path::PathBuf>,
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
    /// The names in `contexts`, outermost first. For logging and error messages.
    pub fn context_names(&self) -> Vec<&str> {
        self.contexts.iter().map(ContextSpec::name).collect()
    }

    pub fn new<P: AsRef<Path>>(path: P) -> Result<PeridotConfig> {
        let contents = fs::read_to_string(path)?;
        let config: PeridotConfig = serde_yaml::from_str(&contents)?;
        Ok(config)
    }
}

