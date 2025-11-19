use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crate::metrics::MetricsProducer;

pub struct PeridotCounter {
    counter: u64,
}

impl PeridotCounter {
    pub fn new() -> Self {
        Self {
            counter: 0
        }
    }

    pub fn increment_counter(&mut self, bytes: u64) {
        self.counter += bytes;
    }

    pub fn get_counter(&self) -> u64 {
        self.counter
    }
}

impl MetricsProducer for Arc<Mutex<PeridotCounter>> {
    fn produce(&self, metrics: &mut HashMap<String, f64>) {
        let counter = self.lock().unwrap();
        metrics.insert("written_bytes".into(), counter.get_counter() as f64);
    }
}
