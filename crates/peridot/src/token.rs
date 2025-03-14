use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{process, thread};
use std::thread::JoinHandle;
use std::time::Duration;
use log::{debug, info, warn};

pub struct TokenBucket {
    max_capacity: Arc<Mutex<u64>>,
    tokens: Arc<(Mutex<u64>, Condvar)>,
    refill_freq: Arc<Mutex<u64>>,
    refill_thread_handle: Option<JoinHandle<()>>,
    end_threads: Arc<AtomicBool>,
}

impl TokenBucket {
    pub fn new(initial_tokens: u64, max_capacity: u64, refill_freq: u64) -> TokenBucket {
        let tokens = Arc::new((Mutex::new(initial_tokens), Condvar::new()));
        TokenBucket {
            max_capacity: Arc::new(Mutex::new(max_capacity)),
            tokens,
            refill_freq: Arc::new(Mutex::new(refill_freq)),
            refill_thread_handle: None,
            end_threads: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn set_max_capacity(&mut self, max_capacity: u64) {
        let mut max_cap = self.max_capacity.lock().unwrap();
        let mut freq = self.refill_freq.lock().unwrap();
        *max_cap = max_capacity;
        *freq = max_capacity;
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
        let tokens = self.tokens.clone();
        let max_capacity = self.max_capacity.clone();
        thread::spawn(move || {
            let max_cap = *max_capacity.lock().unwrap();
            let mut tokens_left = n_tokens;
            if tokens_left > max_cap {
                while tokens_left > 0 {
                    let consume_amount = std::cmp::min(tokens_left, max_cap);
                    TokenBucket::do_consume(tokens.clone(), consume_amount);
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
        max_capacity: Arc<Mutex<u64>>,
        refill_freq: Arc<Mutex<u64>>,
    ) {
        debug!("Refill thread started.");
        let interval = Duration::from_secs(1);
        while !end_threads.load(Ordering::Relaxed) {
            let (lock, cvar) = &*tokens;
            {
                let mut curr_tokens = lock.lock().unwrap();
                let current_max_capacity = *max_capacity.lock().unwrap();
                let current_refill_freq = *refill_freq.lock().unwrap();
                debug!(
                    "{}: Refill. Freq: {}. Prev tokens: {}",
                    process::id(), current_refill_freq, *curr_tokens
                );
                *curr_tokens = std::cmp::min(*curr_tokens + current_refill_freq, current_max_capacity);
                cvar.notify_all();
            }
            thread::sleep(interval);
        }
    }

    pub fn start_refill_thread(&mut self) {
        let tokens = self.tokens.clone();
        let end_threads = self.end_threads.clone();
        let max_capacity = self.max_capacity.clone();
        let refill_freq = self.refill_freq.clone();
        let handle = thread::spawn(move || {
            TokenBucket::refill(tokens, end_threads, max_capacity, refill_freq);
        });
        self.refill_thread_handle = Some(handle);
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
}