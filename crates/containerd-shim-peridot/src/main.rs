use containerd_shim_wasm::shim::{Cli};
use containerd_shim_peridot::PeridotShim;

fn main() {
    env_logger::init();
    PeridotShim::run(None);
}