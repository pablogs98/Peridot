use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Min-Max Fair Share Control Algorithm
/// as seen in PAIO, from [Usenix FAST '22](https://www.usenix.org/system/files/fast22-macedo.pdf).
pub struct MinMaxFairShare {
    max_bandwidth: f64,
    demands: Arc<Mutex<HashMap<String, f64>>>,
    rates: Arc<Mutex<HashMap<String, f64>>>,
}

impl MinMaxFairShare {
    pub fn new(
        max_bandwidth: f64,
        demands: Arc<Mutex<HashMap<String, f64>>>,
        rates: Arc<Mutex<HashMap<String, f64>>>,
    ) -> MinMaxFairShare {
        MinMaxFairShare {
            max_bandwidth,
            demands,
            rates,
        }
    }

    /// Distributes `max_bandwidth` across the registered modules.
    ///
    /// Each module is granted its demand when that demand fits within an equal
    /// share of what is left, and the equal share otherwise; whatever a modest
    /// demand does not take is redistributed among the ones still to be served.
    ///
    /// **Demands have to be considered in ascending order for this to hold.**
    /// Serving a large demand while a small one is still pending clamps the
    /// large one to a fair share computed against a module that was never going
    /// to use it, and the difference is then left unallocated. With a global
    /// 1 Gbps and demands of 100/200/300/400 Mbps, ascending order grants every
    /// module its demand, while descending order grants 100/200/250/250 and
    /// strands 200 Mbps. Iterating the `HashMap` directly gave whichever of
    /// those two answers the hasher happened to produce on that run.
    pub async fn allocate_bandwidth(&self) {
        let mut ordered: Vec<(String, f64)> = {
            let demands = self.demands.lock().await;
            demands.iter().map(|(k, &v)| (k.clone(), v)).collect()
        };
        // Ties broken by module id so that a given set of demands always
        // produces the same allocation, run to run.
        ordered.sort_by(|(a_key, a_demand), (b_key, b_demand)| {
            a_demand
                .partial_cmp(b_demand)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a_key.cmp(b_key))
        });

        let mut left_bandwidth = self.max_bandwidth;
        let mut active = ordered.len();
        let mut allocation = HashMap::with_capacity(ordered.len());

        for (key, demand) in ordered {
            let fair_share = left_bandwidth / active as f64;
            let rate = if demand <= fair_share {
                demand
            } else {
                fair_share
            };
            left_bandwidth -= rate;
            active -= 1;
            allocation.insert(key, rate);
        }

        *self.rates.lock().await = allocation;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn allocate(max_bandwidth: f64, demands: &[(&str, f64)]) -> HashMap<String, f64> {
        let demands: HashMap<String, f64> =
            demands.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let rates = Arc::new(Mutex::new(HashMap::new()));
        let policy = MinMaxFairShare::new(
            max_bandwidth,
            Arc::new(Mutex::new(demands)),
            Arc::clone(&rates),
        );
        policy.allocate_bandwidth().await;
        let out = rates.lock().await.clone();
        out
    }

    /// The configuration from the paper's dynamic I/O experiment: four demands
    /// summing to exactly the global policy, so every module should receive
    /// precisely what it asked for.
    #[tokio::test]
    async fn demands_summing_to_capacity_are_all_satisfied() {
        let rates = allocate(
            1000.0,
            &[("m1", 100.0), ("m2", 200.0), ("m3", 300.0), ("m4", 400.0)],
        )
        .await;

        assert_eq!(rates["m1"], 100.0);
        assert_eq!(rates["m2"], 200.0);
        assert_eq!(rates["m3"], 300.0);
        assert_eq!(rates["m4"], 400.0);
    }

    /// The property the ordering exists to guarantee: what a small demand
    /// leaves behind is handed to the modules that can use it, so an
    /// oversubscribed policy is still fully allocated.
    #[tokio::test]
    async fn spare_capacity_is_redistributed_to_larger_demands() {
        let rates = allocate(
            1000.0,
            &[("small", 100.0), ("a", 900.0), ("b", 900.0), ("c", 900.0)],
        )
        .await;

        assert_eq!(rates["small"], 100.0);
        for key in ["a", "b", "c"] {
            assert_eq!(rates[key], 300.0, "{key} should share the remaining 900");
        }
        assert_eq!(rates.values().sum::<f64>(), 1000.0);
    }

    /// Insertion order must not change the outcome. Before demands were
    /// sorted, these two maps could allocate differently purely because of
    /// `HashMap` iteration order.
    #[tokio::test]
    async fn allocation_is_independent_of_insertion_order() {
        let ascending = allocate(
            1000.0,
            &[("m1", 100.0), ("m2", 200.0), ("m3", 300.0), ("m4", 400.0)],
        )
        .await;
        let descending = allocate(
            1000.0,
            &[("m4", 400.0), ("m3", 300.0), ("m2", 200.0), ("m1", 100.0)],
        )
        .await;

        assert_eq!(ascending, descending);
    }

    /// A module that finishes is removed from `demands`, and the next round
    /// has to hand its share to whoever is still running.
    #[tokio::test]
    async fn capacity_is_reclaimed_when_a_module_leaves() {
        let before = allocate(1000.0, &[("a", 900.0), ("b", 900.0)]).await;
        assert_eq!(before["a"], 500.0);
        assert_eq!(before["b"], 500.0);

        let after = allocate(1000.0, &[("a", 900.0)]).await;
        assert_eq!(after["a"], 900.0);
    }

    #[tokio::test]
    async fn no_modules_allocates_nothing() {
        assert!(allocate(1000.0, &[]).await.is_empty());
    }
}
