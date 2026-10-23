//! In-process token-bucket rate limiting.
//!
//! Buckets are keyed by client IP and route scope. The limit is per minute; the
//! bucket refills continuously so a burst up to `capacity` is allowed and then
//! requests are paced. State is in-process and per-instance, which is documented
//! in the README (a shared Redis limiter is not implemented).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Injectable monotonic clock, in milliseconds.
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// A token bucket.
#[derive(Debug, Clone, Copy)]
struct Bucket {
    tokens: f64,
    updated_ms: u64,
}

/// Per-IP, per-route token bucket limiter.
pub struct RateLimiter {
    capacity: f64,
    refill_per_ms: f64,
    clock: Clock,
    buckets: Mutex<HashMap<String, Bucket>>,
    max_entries: usize,
}

impl std::fmt::Debug for RateLimiter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RateLimiter")
            .field("capacity", &self.capacity)
            .field("refill_per_ms", &self.refill_per_ms)
            .field("max_entries", &self.max_entries)
            .finish_non_exhaustive()
    }
}

impl RateLimiter {
    /// Create a limiter allowing `per_minute` requests per key.
    pub fn new(per_minute: u32) -> Self {
        Self::with_clock(
            per_minute,
            Arc::new(|| {
                let elapsed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            }),
        )
    }

    /// Create a limiter with an injectable clock (used by tests).
    pub fn with_clock(per_minute: u32, clock: Clock) -> Self {
        let capacity = f64::from(per_minute.max(1));
        Self {
            capacity,
            refill_per_ms: capacity / 60_000.0,
            clock,
            buckets: Mutex::new(HashMap::new()),
            max_entries: 10_000,
        }
    }

    /// Capacity of one bucket.
    pub fn capacity(&self) -> f64 {
        self.capacity
    }

    /// Try to take one token for `key`.
    ///
    /// Returns `Ok(())` when allowed and the retry delay in whole seconds when
    /// the bucket is empty.
    pub fn check(&self, key: &str) -> Result<(), u64> {
        let now_ms = (self.clock)();
        let mut buckets = match self.buckets.lock() {
            Ok(guard) => guard,
            // A panic while holding the lock cannot corrupt a `f64`/`u64` map in
            // any way that matters here, so recover instead of poisoning every
            // future request.
            Err(poisoned) => poisoned.into_inner(),
        };

        if buckets.len() > self.max_entries {
            let stale_before = now_ms.saturating_sub(10 * 60 * 1000);
            buckets.retain(|_, bucket| bucket.updated_ms >= stale_before);
            if buckets.len() > self.max_entries {
                buckets.clear();
            }
        }

        let bucket = buckets.entry(key.to_owned()).or_insert(Bucket {
            tokens: self.capacity,
            updated_ms: now_ms,
        });
        let elapsed = now_ms.saturating_sub(bucket.updated_ms) as f64;
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_ms).min(self.capacity);
        bucket.updated_ms = now_ms;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            Ok(())
        } else {
            let missing = 1.0 - bucket.tokens;
            let wait_ms = (missing / self.refill_per_ms).ceil();
            let seconds = (wait_ms / 1000.0).ceil().max(1.0);
            Err(seconds as u64)
        }
    }

    /// Number of tracked buckets (observability and tests).
    pub fn tracked_keys(&self) -> usize {
        match self.buckets.lock() {
            Ok(guard) => guard.len(),
            Err(poisoned) => poisoned.into_inner().len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn limiter(per_minute: u32) -> (RateLimiter, Arc<AtomicU64>) {
        let now = Arc::new(AtomicU64::new(0));
        let clock_now = Arc::clone(&now);
        let limiter = RateLimiter::with_clock(
            per_minute,
            Arc::new(move || clock_now.load(Ordering::SeqCst)),
        );
        (limiter, now)
    }

    #[test]
    fn burst_is_allowed_then_rejected() {
        let (limiter, _now) = limiter(5);
        for _ in 0..5 {
            assert!(limiter.check("ip:login").is_ok());
        }
        let retry = limiter.check("ip:login").expect_err("must be limited");
        assert!(retry >= 1);
    }

    #[test]
    fn buckets_are_isolated_by_key() {
        let (limiter, _now) = limiter(1);
        assert!(limiter.check("a").is_ok());
        assert!(limiter.check("a").is_err());
        assert!(limiter.check("b").is_ok());
    }

    #[test]
    fn tokens_refill_over_time() {
        let (limiter, now) = limiter(60);
        for _ in 0..60 {
            assert!(limiter.check("ip").is_ok());
        }
        assert!(limiter.check("ip").is_err());
        now.store(1_000, Ordering::SeqCst);
        assert!(limiter.check("ip").is_ok());
    }

    #[test]
    fn retry_after_reflects_refill_rate() {
        let (limiter, _now) = limiter(1);
        assert!(limiter.check("ip").is_ok());
        let retry = limiter.check("ip").expect_err("limited");
        // One token per minute: the next token is at least ~59 seconds away.
        assert!(retry >= 59, "retry was {retry}");
    }

    #[test]
    fn tracked_keys_are_reported() {
        let (limiter, _now) = limiter(10);
        assert_eq!(limiter.tracked_keys(), 0);
        assert!(limiter.check("a").is_ok());
        assert!(limiter.check("b").is_ok());
        assert_eq!(limiter.tracked_keys(), 2);
    }
}
