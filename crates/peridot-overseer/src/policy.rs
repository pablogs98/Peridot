use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Min-Max Fair Share Control Algorithm
/// as seen in PAIO: https://www.usenix.org/system/files/fast22-macedo.pdf
pub struct MinMaxFairShare {
    max_bandwidth: f64,
    demands: Arc<Mutex<HashMap<u32, f64>>>,
    rates: Arc<Mutex<HashMap<u32, f64>>>,
}

impl MinMaxFairShare {
    pub fn new(
        max_bandwidth: f64,
        demands: Arc<Mutex<HashMap<u32, f64>>>,
        rates: Arc<Mutex<HashMap<u32, f64>>>,
    ) -> MinMaxFairShare {
        MinMaxFairShare {
            max_bandwidth,
            demands,
            rates,
        }
    }

    pub async fn allocate_bandwidth(&self) {
        let mut left_bandwidth = self.max_bandwidth;
        let mut active = self.demands.lock().await.len();
        let demands_lock = self.demands.lock().await;

        for (key, &demand) in demands_lock.iter() {
            let fair_share = left_bandwidth / active as f64;

            if demand <= fair_share {
                self.rates.lock().await.insert(*key, demand);
            } else {
                self.rates.lock().await.insert(*key, fair_share);
            }
            left_bandwidth -= self.rates.lock().await[&key];
            active -= 1;
        }
    }
}
