use std::collections::HashMap;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

pub struct TokenBucket {
    max_capacity: u32,
    tokens: Arc<(Mutex<u32>, Condvar)>,
    pub refill_freq: u32,
    refill_thread_handle: Option<JoinHandle<()>>,
    end_threads: Arc<bool>,
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
            end_threads: Arc::new(false),
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

    pub fn consume(&self, n_tokens: u32, pid: i32) {
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
        end_threads: Arc<bool>,
        max_capacity: u32,
        refill_freq: u32,
    ) {
        while !*end_threads {
            let (lock, cvar) = &*tokens;
            let mut curr_tokens = lock.lock().unwrap();
            *curr_tokens = std::cmp::min(*curr_tokens + refill_freq, max_capacity);
            cvar.notify_all();
            // send statistics here
            thread::sleep(Duration::from_secs(1)); // Increment every second
        }
    }

    pub fn start_refill_thread(&mut self) {
        let refill_thread_handle = thread::spawn(move || {
            TokenBucket::refill(
                self.tokens.clone(),
                self.end_threads.clone(),
                self.max_capacity,
                self.refill_freq,
            )
        });

        self.refill_thread_handle = Some(refill_thread_handle);
    }

    pub fn join_refill_thread(&self, handle: JoinHandle<()>) {
        *self.end_threads = false;
        handle.join().unwrap();
    }
}
