/// Min-Max Fair Share Control Algorithm
/// as seen in PAIO: https://www.usenix.org/system/files/fast22-macedo.pdf
pub struct MinMaxFairShare {
    max_bandwidth: i32,
    demands: Vec<f64>,
}

impl MinMaxFairShare {
    pub fn new(max_bandwidth: i32, demands: Vec<f64>) -> MinMaxFairShare {
        MinMaxFairShare {
            max_bandwidth,
            demands,
        }
    }

    fn allocate_bandwidth(mut demands: Vec<f64>, max_b: f64) -> Vec<f64> {
        let mut rates = vec![0.0; demands.len()];
        let mut left_bandwidth = max_b;
        let mut active = demands.len();

        for i in 0..active {
            let fair_share = left_bandwidth / (active - i) as f64;

            if demands[i] <= fair_share {
                rates[i] = demands[i];
            } else {
                rates[i] = fair_share;
            }
            left_bandwidth -= rates[i];
        }

        for i in 0..active {
            rates[i] = left_bandwidth / active as f64;
        }

        rates
    }
}
