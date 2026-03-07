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

    #[error("clock skew: event timestamp {event_time} is {skew_secs}s in the future (max {max_secs}s)")]
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
        let now = Instant::now();
        let bucket = self.buckets.entry(connection_id.to_string()).or_insert(TokenBucket {
            tokens: self.max_per_min,
            last_refill: now,
        });

        // Refill tokens based on elapsed time
        let elapsed = now.duration_since(bucket.last_refill);
        if elapsed >= Duration::from_secs(60) {
            bucket.tokens = self.max_per_min;
            bucket.last_refill = now;
        } else {
            let refill = (elapsed.as_secs_f64() / 60.0 * self.max_per_min as f64) as u32;
            bucket.tokens = (bucket.tokens + refill).min(self.max_per_min);
            if refill > 0 {
                bucket.last_refill = now;
            }
        }

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
