//! A token-bucket speed limiter shared by every connection of every download.

use std::{
    sync::Mutex,
    time::{Duration, Instant},
};

pub(crate) struct RateLimiter {
    bucket: Mutex<Bucket>,
}

struct Bucket {
    /// Bytes per second; 0 means unlimited.
    rate: u64,
    /// Available bytes. Goes negative when a chunk is larger than what is
    /// available; the debt is paid by waiting.
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(rate: u64) -> Self {
        Self {
            bucket: Mutex::new(Bucket {
                rate,
                tokens: 0.0,
                last: Instant::now(),
            }),
        }
    }

    pub fn set_rate(&self, rate: u64) {
        let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
        if b.rate != rate {
            b.rate = rate;
            b.tokens = 0.0;
            b.last = Instant::now();
        }
    }

    /// Waits until `bytes` may be written under the current limit.
    pub async fn acquire(&self, bytes: usize) {
        let wait = {
            let mut b = self.bucket.lock().unwrap_or_else(|e| e.into_inner());
            if b.rate == 0 {
                return;
            }
            let now = Instant::now();
            let rate = b.rate as f64;
            // Allow at most a quarter second of burst after an idle period.
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(rate / 4.0);
            b.last = now;
            b.tokens -= bytes as f64;
            if b.tokens >= 0.0 {
                return;
            }
            Duration::from_secs_f64(-b.tokens / rate)
        };
        tokio::time::sleep(wait).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_does_not_wait() {
        let limiter = RateLimiter::new(0);
        let start = Instant::now();
        for _ in 0..100 {
            limiter.acquire(1 << 20).await;
        }
        assert!(start.elapsed() < Duration::from_millis(50));
    }

    #[tokio::test]
    async fn limits_throughput() {
        let limiter = RateLimiter::new(100_000);
        let start = Instant::now();
        for _ in 0..10 {
            limiter.acquire(10_000).await;
        }
        // 100 kB at 100 kB/s, minus a quarter second of burst at most.
        let elapsed = start.elapsed();
        assert!(elapsed >= Duration::from_millis(700), "{elapsed:?}");
        assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
    }
}
