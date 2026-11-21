use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct Limiter {
    // who it limits, for the rate limit log
    name: &'static str,
    rate: f64,
    burst: f64,
    bucket: Mutex<Bucket>,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Limiter {
    pub fn new(name: &'static str, rate: f64, burst: f64) -> Limiter {
        Limiter {
            name,
            rate,
            burst,
            bucket: Mutex::new(Bucket {
                tokens: burst,
                updated: Instant::now(),
            }),
        }
    }

    // how long a request made now would wait, without taking a token
    pub fn wait_estimate(&self) -> Duration {
        let bucket = self
            .bucket
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let refill = bucket.updated.elapsed().as_secs_f64() * self.rate;
        let tokens = (bucket.tokens + refill).min(self.burst);
        Duration::from_secs_f64(((1.0 - tokens) / self.rate).max(0.0))
    }

    // details say what the request was, for the rate limit log
    pub async fn acquire(&self, max_wait: Option<Duration>, details: &str) -> bool {
        let wait = {
            let mut bucket = self
                .bucket
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let now = Instant::now();
            let refill = now.duration_since(bucket.updated).as_secs_f64() * self.rate;
            bucket.tokens = (bucket.tokens + refill).min(self.burst);
            bucket.updated = now;
            let wait = Duration::from_secs_f64(((1.0 - bucket.tokens) / self.rate).max(0.0));
            if max_wait.is_some_and(|max| wait > max) {
                drop(bucket);
                crate::rate_log::waited(self.name, "gave up waiting", wait, details);
                return false;
            }
            bucket.tokens -= 1.0;
            wait
        };
        crate::rate_log::waited(self.name, "waited", wait, details);
        if !wait.is_zero() {
            crate::platform::sleep(wait).await;
        }
        true
    }
}
