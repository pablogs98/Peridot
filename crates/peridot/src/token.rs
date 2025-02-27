use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use log::{debug, info, warn};

pub struct TokenBucket {
    max_capacity: u32,
    tokens: Arc<(Mutex<u32>, Condvar)>,
    pub refill_freq: u32,
    refill_thread_handle: Option<JoinHandle<()>>,
    end_threads: Arc<AtomicBool>,
    stats: HashMap<i32, Vec<i32>>,
}

impl TokenBucket {
    pub fn new(initial_tokens: u32, max_capacity: u32, refill_freq: u32) -> TokenBucket {
        let tokens = Arc::new((Mutex::new(initial_tokens), Condvar::new()));
        TokenBucket {
            max_capacity,
            tokens,
            refill_freq,
            refill_thread_handle: None,
            end_threads: Arc::new(AtomicBool::new(false)),
            stats: HashMap::new(),
        }
    }

    fn do_consume(&self, n_tokens: u32) {
        let (lock, cvar) = &*self.tokens;
        {
            let mut curr_tokens = lock.lock().unwrap();
            while *curr_tokens < n_tokens {
                curr_tokens = cvar.wait(curr_tokens).unwrap();
            }
            *curr_tokens -= n_tokens;
        }
    }

    pub fn consume(&self, n_tokens: u32) {
        if n_tokens > self.max_capacity {
            let mut tokens_left = n_tokens;
            while tokens_left > 0 {
                self.do_consume(self.max_capacity);
                tokens_left -= self.max_capacity;

                if tokens_left < self.max_capacity {
                    self.do_consume(tokens_left);
                    tokens_left = 0;
                }
            }
        } else {
            self.do_consume(n_tokens);
        }
    }

    fn refill(
        tokens: Arc<(Mutex<u32>, Condvar)>,
        end_threads: Arc<AtomicBool>,
        max_capacity: u32,
        refill_freq: u32,
    ) {
        debug!("Refill thread started successfully. Max capacity: {} token(s). Refill frequency: {} token(s)/s. ", max_capacity, refill_freq);
        while !end_threads.load(Ordering::Relaxed) {
            let (lock, cvar) = &*tokens;
            let mut curr_tokens = lock.lock().unwrap();
            *curr_tokens = std::cmp::min(*curr_tokens + refill_freq, max_capacity);
            cvar.notify_all();
            thread::sleep(Duration::from_secs(1)); // Increment every second
        }
    }

    pub fn start_refill_thread(&mut self) {
        let tokens = self.tokens.clone();
        let end_threads = self.end_threads.clone();
        let max_capacity = self.max_capacity;
        let refill_freq = self.refill_freq;

        let refill_thread_handle = thread::spawn(move || {
            TokenBucket::refill(
                tokens,
                end_threads,
                max_capacity,
                refill_freq
            )
        });

        self.refill_thread_handle = Some(refill_thread_handle);
    }
}

impl Drop for TokenBucket {
    fn drop(&mut self) {
        self.end_threads.store(true, Ordering::Relaxed);
        if let Some(handle) = self.refill_thread_handle.take() {
            handle.join().unwrap();
            info!("TokenBucket dropped. Refill thread stopped successfully.");
        }
        else {
            warn!("Attempted to join refill thread but it was already stopped.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        if std::env::var_os("RUST_LOG").is_none() {
            std::env::set_var("RUST_LOG", "debug");
        }
        env_logger::init();
        let mut token_bucket = TokenBucket::new(1024, 1024, 1);
        token_bucket.start_refill_thread();
        token_bucket.consume(1024, 1);
    }
}