use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{process, thread};
use std::collections::HashMap;
use std::thread::JoinHandle;
use std::time::Duration;
use log::{debug, info, warn};
use crate::metrics::MetricsSubscriber;

pub struct TokenBucket {
    max_tokens: Arc<Mutex<u64>>,
    tokens: Arc<(Mutex<u64>, Condvar)>,
    refill_freq: Arc<Mutex<u64>>,
    refill_thread_handle: Option<JoinHandle<()>>,
    end_threads: Arc<AtomicBool>,
}

unsafe impl Send for TokenBucket {}
unsafe impl Sync for TokenBucket {}

impl TokenBucket {
    pub fn new(initial_tokens: u64, max_tokens: u64, refill_freq: u64) -> TokenBucket {
        let tokens = Arc::new((Mutex::new(initial_tokens), Condvar::new()));
        TokenBucket {
            max_tokens: Arc::new(Mutex::new(max_tokens)),
            tokens,
            refill_freq: Arc::new(Mutex::new(refill_freq)),
            refill_thread_handle: None,
            end_threads: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_max_tokens(&mut self, max_tokens: u64) {
        let mut max_cap = self.max_tokens.lock().unwrap();
        let mut freq = self.refill_freq.lock().unwrap();
        *max_cap = max_tokens;
        *freq = max_tokens;
    }

    fn do_consume(tokens: Arc<(Mutex<u64>, Condvar)>, n_tokens: u64) {
        let (lock, cvar) = &*tokens;
        let mut curr_tokens = lock.lock().unwrap();
        while *curr_tokens < n_tokens {
            curr_tokens = cvar.wait(curr_tokens).unwrap();
        }
        *curr_tokens -= n_tokens;
        cvar.notify_all();
    }

    pub fn consume(&self, n_tokens: u64) -> JoinHandle<()> {
        let tokens = Arc::clone(&self.tokens);
        let max_tokens = Arc::clone(&self.max_tokens);
        thread::spawn(move || {
            let max_cap = *max_tokens.lock().unwrap();
            let mut tokens_left = n_tokens;
            if tokens_left > max_cap {
                while tokens_left > 0 {
                    let consume_amount = std::cmp::min(tokens_left, max_cap);
                    TokenBucket::do_consume(Arc::clone(&tokens), consume_amount);
                    tokens_left -= consume_amount;
                }
            } else {
                TokenBucket::do_consume(tokens, n_tokens);
            }
        })
    }

    fn refill(
        tokens: Arc<(Mutex<u64>, Condvar)>,
        end_threads: Arc<AtomicBool>,
        max_tokens: Arc<Mutex<u64>>,
        refill_freq: Arc<Mutex<u64>>,
    ) {
        debug!("Refill thread started.");
        let interval = Duration::from_secs(1);
        while !end_threads.load(Ordering::Relaxed) {
            let (lock, cvar) = &*tokens;
            {
                let mut curr_tokens = lock.lock().unwrap();
                let current_max_tokens = *max_tokens.lock().unwrap();
                let current_refill_freq = *refill_freq.lock().unwrap();
                debug!(
                    "{}: Refill. Freq: {}. Prev tokens: {}",
                    process::id(), current_refill_freq, *curr_tokens
                );
                *curr_tokens = std::cmp::min(*curr_tokens + current_refill_freq, current_max_tokens);
                cvar.notify_all();
            }
            thread::sleep(interval);
        }
    }

    pub fn start_refill_thread(&mut self) {
        let tokens = Arc::clone(&self.tokens);
        let end_threads = Arc::clone(&self.end_threads);
        let max_tokens = Arc::clone(&self.max_tokens);
        let refill_freq = Arc::clone(&self.refill_freq);
        let handle = thread::spawn(move || {
            TokenBucket::refill(tokens, end_threads, max_tokens, refill_freq);
        });
        self.refill_thread_handle = Some(handle);
    }
}

impl MetricsSubscriber for TokenBucket {
    /// Applies a new rate published by the overseer.
    ///
    /// An unusable update leaves the current rate in place rather than taking
    /// the guest down with it: this runs on the metrics thread, off the I/O
    /// critical path, and the bucket already holds a workable rate from its
    /// configuration. Two ways an update can be unusable:
    ///
    /// * `token_rate` is absent, which indexing would have panicked on;
    /// * it is negative or not a number, which `as u64` would have saturated
    ///   to 0 — a bucket that never refills, so the guest blocks on its next
    ///   read or write and never wakes.
    fn update(&mut self, metrics: &HashMap<String, f64>) {
        let Some(&rate) = metrics.get("token_rate") else {
            warn!("metrics update carried no `token_rate`; keeping the current rate");
            return;
        };

        if !rate.is_finite() || rate < 0.0 {
            warn!("ignoring unusable `token_rate` {rate}; keeping the current rate");
            return;
        }

        self.set_max_tokens(rate as u64);
    }
}

impl Drop for TokenBucket {
    fn drop(&mut self) {
        self.end_threads.store(true, Ordering::Relaxed);
        if let Some(handle) = self.refill_thread_handle.take() {
            handle.join().unwrap();
            info!("{}: TokenBucket dropped. Refill thread stopped.", process::id());
        } else {
            warn!("Refill thread already stopped.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consume_non_blocking() {
        let mut bucket = TokenBucket::new(10, 20, 5);
        bucket.start_refill_thread();
        let handle = bucket.consume(15);
        handle.join().unwrap();
    }

    fn max_tokens(bucket: &TokenBucket) -> u64 {
        *bucket.max_tokens.lock().unwrap()
    }

    #[test]
    fn update_applies_a_published_rate() {
        let mut bucket = TokenBucket::new(10, 20, 5);
        bucket.update(&HashMap::from([("token_rate".to_string(), 4096.0)]));
        assert_eq!(max_tokens(&bucket), 4096);
    }

    /// An update without the key used to panic, taking the guest with it.
    #[test]
    fn update_without_token_rate_keeps_the_current_rate() {
        let mut bucket = TokenBucket::new(10, 20, 5);
        let before = max_tokens(&bucket);
        bucket.update(&HashMap::from([("written_bytes".to_string(), 1.0)]));
        assert_eq!(max_tokens(&bucket), before);
    }

    /// A negative or non-finite rate saturates to 0 through `as u64`, leaving
    /// a bucket that never refills and a guest that blocks forever.
    #[test]
    fn update_with_an_unusable_rate_keeps_the_current_rate() {
        for bad in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut bucket = TokenBucket::new(10, 20, 5);
            let before = max_tokens(&bucket);
            bucket.update(&HashMap::from([("token_rate".to_string(), bad)]));
            assert_eq!(max_tokens(&bucket), before, "rate {bad} should be ignored");
        }
    }
}