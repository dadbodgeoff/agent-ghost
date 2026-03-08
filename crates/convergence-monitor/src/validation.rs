//! Event validation (Req 9 AC3).
//!
//! Schema check, timestamp sanity, rate limiting.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("missing required field: {field}")]
    #[allow(dead_code)]
    MissingField { field: String },

    #[error(
        "clock skew: event timestamp {event_time} is {skew_secs}s in the future (max {max_secs}s)"
    )]
    ClockSkew {
        event_time: DateTime<Utc>,
        skew_secs: i64,
        max_secs: i64,
    },

    #[error("rate limit exceeded for connection {connection_id}: {count}/{max} events/min")]
    RateLimitExceeded {
        connection_id: String,
        count: u32,
        max: u32,
    },

    #[error("empty session_id")]
    EmptySessionId,

    #[error("malformed event: {reason}")]
    #[allow(dead_code)]
    Malformed { reason: String },
}

/// Token bucket rate limiter per connection.
pub struct RateLimiter {
    buckets: BTreeMap<String, TokenBucket>,
    max_per_min: u32,
}

struct TokenBucket {
    tokens: u32,
    last_refill: Instant,
    refill_remainder_nanos: u128,
    last_touched: Instant,
}

impl RateLimiter {
    pub fn new(max_per_min: u32) -> Self {
        Self {
            buckets: BTreeMap::new(),
            max_per_min,
        }
    }

    /// Try to consume a token. Returns `Ok(())` if allowed, `Err` if rate limited.
    pub fn try_consume(&mut self, connection_id: &str) -> Result<(), ValidationError> {
        self.try_consume_at(connection_id, Instant::now())
    }

    pub fn try_consume_at(
        &mut self,
        connection_id: &str,
        now: Instant,
    ) -> Result<(), ValidationError> {
        let bucket = self
            .buckets
            .entry(connection_id.to_string())
            .or_insert(TokenBucket {
                tokens: self.max_per_min,
                last_refill: now,
                refill_remainder_nanos: 0,
                last_touched: now,
            });

        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed > Duration::ZERO {
            let scaled =
                elapsed.as_nanos() * u128::from(self.max_per_min) + bucket.refill_remainder_nanos;
            let minute_nanos = Duration::from_secs(60).as_nanos();
            let refill = (scaled / minute_nanos).min(u128::from(self.max_per_min)) as u32;
            bucket.refill_remainder_nanos = scaled % minute_nanos;
            if refill > 0 {
                bucket.tokens = (bucket.tokens + refill).min(self.max_per_min);
                bucket.last_refill = now;
            }
        }
        bucket.last_touched = now;

        if bucket.tokens == 0 {
            return Err(ValidationError::RateLimitExceeded {
                connection_id: connection_id.to_string(),
                count: self.max_per_min,
                max: self.max_per_min,
            });
        }

        bucket.tokens -= 1;
        Ok(())
    }

    pub fn prune_idle(&mut self, idle_horizon: Duration) {
        self.prune_idle_at(Instant::now(), idle_horizon);
    }

    pub fn prune_idle_at(&mut self, now: Instant, idle_horizon: Duration) {
        self.buckets
            .retain(|_, bucket| now.duration_since(bucket.last_touched) < idle_horizon);
    }

    #[cfg(test)]
    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }
}

/// Validate an incoming event.
pub struct EventValidator {
    clock_skew_tolerance: Duration,
}

impl EventValidator {
    pub fn new(clock_skew_tolerance: Duration) -> Self {
        Self {
            clock_skew_tolerance,
        }
    }

    /// Validate event timestamp against clock skew tolerance.
    pub fn validate_timestamp(&self, event_time: DateTime<Utc>) -> Result<(), ValidationError> {
        let now = Utc::now();
        let skew = event_time - now;
        let max_secs = self.clock_skew_tolerance.as_secs() as i64;

        if skew.num_seconds() > max_secs {
            return Err(ValidationError::ClockSkew {
                event_time,
                skew_secs: skew.num_seconds(),
                max_secs,
            });
        }

        Ok(())
    }

    /// Validate that session_id is non-empty.
    pub fn validate_session_id(&self, session_id: &Uuid) -> Result<(), ValidationError> {
        if session_id.is_nil() {
            return Err(ValidationError::EmptySessionId);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limiter_refills_deterministically() {
        let mut limiter = RateLimiter::new(60);
        let start = Instant::now();

        for _ in 0..60 {
            limiter.try_consume_at("conn-1", start).unwrap();
        }
        assert!(matches!(
            limiter.try_consume_at("conn-1", start),
            Err(ValidationError::RateLimitExceeded { .. })
        ));

        limiter
            .try_consume_at("conn-1", start + Duration::from_secs(1))
            .unwrap();
    }

    #[test]
    fn rate_limiter_prunes_idle_buckets() {
        let mut limiter = RateLimiter::new(10);
        let start = Instant::now();
        limiter.try_consume_at("conn-1", start).unwrap();
        assert_eq!(limiter.bucket_count(), 1);

        limiter.prune_idle_at(start + Duration::from_secs(301), Duration::from_secs(300));
        assert_eq!(limiter.bucket_count(), 0);
    }

    #[test]
    fn rate_limiter_handles_long_runtime_edges_without_drift() {
        let mut limiter = RateLimiter::new(3);
        let start = Instant::now();

        for _ in 0..3 {
            limiter.try_consume_at("conn-1", start).unwrap();
        }
        assert!(matches!(
            limiter.try_consume_at("conn-1", start),
            Err(ValidationError::RateLimitExceeded { .. })
        ));

        limiter
            .try_consume_at("conn-1", start + Duration::from_secs(20))
            .unwrap();
        assert!(matches!(
            limiter.try_consume_at("conn-1", start + Duration::from_secs(39)),
            Err(ValidationError::RateLimitExceeded { .. })
        ));
        limiter
            .try_consume_at("conn-1", start + Duration::from_secs(40))
            .unwrap();
        limiter
            .try_consume_at("conn-1", start + Duration::from_secs(60))
            .unwrap();

        assert!(matches!(
            limiter.try_consume_at("conn-1", start + Duration::from_secs(60)),
            Err(ValidationError::RateLimitExceeded { .. })
        ));

        limiter
            .try_consume_at("conn-1", start + Duration::from_secs(60 * 60 * 24 * 365))
            .unwrap();
    }
}
