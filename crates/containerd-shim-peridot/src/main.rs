use containerd_shim_wasm::shim::{Cli, Config};
use containerd_shim_peridot::PeridotShim;

fn main() {
    let shim_config = Config {
        default_log_level: "error".to_string(),
        ..Default::default()
    };
    PeridotShim::run(shim_config);
}