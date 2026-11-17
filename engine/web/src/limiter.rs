use std::sync::Mutex;
use web_time::{Duration, Instant};

pub struct Limiter {
    rate: f64,
    burst: f64,
    bucket: Mutex<Bucket>,
}

struct Bucket {
    tokens: f64,
    updated: Instant,
}

impl Limiter {
    pub fn new(rate: f64, burst: f64) -> Limiter {
        Limiter {
            rate,
            burst,
            bucket: Mutex::new(Bucket {
                tokens: burst,
                updated: Instant::now(),
            }),
        }
    }

    pub async fn acquire(&self, max_wait: Option<Duration>) -> bool {
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
                return false;
            }
            bucket.tokens -= 1.0;
            wait
        };
        if !wait.is_zero() {
            crate::platform::sleep(wait).await;
        }
        true
    }
}
