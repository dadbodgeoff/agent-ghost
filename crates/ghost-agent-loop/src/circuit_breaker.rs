//! Tool-level circuit breaker with error classification (Req 12 AC1–AC3, AC6, WP1-B).
//!
//! 3 states: Closed, Open, HalfOpen.
//! INDEPENDENT from the provider circuit breaker in ghost-llm (A22.2).
//! Policy denials do NOT increment this circuit breaker (AC6).
//!
//! Error classification (WP1-B):
//! - Transient (5xx) and Fatal errors increment the failure counter.
//! - RateLimit (429), AuthFailure (401/403), and ModelRefusal do NOT.
//! This prevents non-transient errors from tripping the breaker.

use std::time::{Duration, Instant};

/// Circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    /// Normal operation — calls pass through.
    Closed,
    /// Tripped — no LLM calls or tool execution (AC2).
    Open,
    /// Cooldown elapsed — allow one probe call.
    HalfOpen,
}

/// Classification of LLM/tool failure types (WP1-B).
///
/// Only `Transient` and `Fatal` errors increment the circuit breaker counter.
/// Other failure types are handled differently (retry with backoff, alert, etc.)
/// and should NOT trip the breaker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureType {
    /// Transient server error (HTTP 500, 502, 503, timeout).
    /// Increments circuit breaker counter.
    Transient,
    /// Rate limited (HTTP 429). Retry with backoff, don't trip breaker.
    RateLimit,
    /// Authentication failure (HTTP 401, 403). Bad key, don't trip breaker.
    AuthFailure,
    /// Model refused the request (content policy, context too long).
    /// Not the provider's fault — don't trip breaker.
    ModelRefusal,
    /// Fatal/unrecoverable error (invalid request, bad schema).
    /// Increments circuit breaker counter.
    Fatal,
}

impl FailureType {
    /// Whether this failure type should increment the circuit breaker counter.
    pub fn should_increment(&self) -> bool {
        matches!(self, FailureType::Transient | FailureType::Fatal)
    }
}

/// Classify an LLM error string into a FailureType.
///
/// Heuristic-based classification from error messages and HTTP status codes
/// commonly returned by LLM providers.
pub fn classify_llm_error(error: &str) -> FailureType {
    let lower = error.to_lowercase();

    // Rate limiting.
    if lower.contains("429")
        || lower.contains("rate limit")
        || lower.contains("too many requests")
        || lower.contains("quota exceeded")
    {
        return FailureType::RateLimit;
    }

    // Auth failures.
    if lower.contains("401")
        || lower.contains("403")
        || lower.contains("unauthorized")
        || lower.contains("forbidden")
        || lower.contains("invalid api key")
        || lower.contains("authentication")
    {
        return FailureType::AuthFailure;
    }

    // Model refusals.
    if lower.contains("content policy")
        || lower.contains("content_policy")
        || lower.contains("context_length_exceeded")
        || lower.contains("context length")
        || lower.contains("model refused")
        || lower.contains("safety")
        || lower.contains("max_tokens")
    {
        return FailureType::ModelRefusal;
    }

    // Transient server errors.
    if lower.contains("500")
        || lower.contains("502")
        || lower.contains("503")
        || lower.contains("timeout")
        || lower.contains("timed out")
        || lower.contains("connection")
        || lower.contains("overloaded")
        || lower.contains("server error")
        || lower.contains("internal error")
    {
        return FailureType::Transient;
    }

    // Default: treat unknown errors as fatal (they'll increment the counter
    // but this is conservative — better to trip on unknown than to ignore).
    FailureType::Fatal
}

/// Tool-level circuit breaker.
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    consecutive_failures: u32,
    /// Threshold for tripping (default 3).
    threshold: u32,
    /// Cooldown before transitioning Open → HalfOpen.
    cooldown: Duration,
    last_failure: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, cooldown: Duration) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            consecutive_failures: 0,
            threshold,
            cooldown,
            last_failure: None,
        }
    }

    pub fn state(&self) -> CircuitBreakerState {
        // Check for automatic Open → HalfOpen transition
        if self.state == CircuitBreakerState::Open {
            if let Some(last) = self.last_failure {
                if last.elapsed() >= self.cooldown {
                    return CircuitBreakerState::HalfOpen;
                }
            }
        }
        self.state
    }

    /// Get the number of consecutive failures.
    pub fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Check if the circuit breaker allows a call (GATE 0).
    pub fn allows_call(&mut self) -> bool {
        match self.state() {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => false,
            CircuitBreakerState::HalfOpen => {
                self.state = CircuitBreakerState::HalfOpen;
                true // Allow one probe
            }
        }
    }

    /// Record a successful call.
    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.state = CircuitBreakerState::Closed;
    }

    /// Record a failed call (NOT called for policy denials — AC6).
    /// This is the legacy API — treats all errors as transient.
    pub fn record_failure(&mut self) {
        self.record_classified_failure(FailureType::Transient);
    }

    /// Record a classified failure (WP1-B).
    ///
    /// Only `Transient` and `Fatal` errors increment the counter and can
    /// trip the breaker. `RateLimit`, `AuthFailure`, and `ModelRefusal`
    /// are logged but do not affect the breaker state.
    pub fn record_classified_failure(&mut self, failure_type: FailureType) {
        if !failure_type.should_increment() {
            tracing::debug!(
                failure_type = ?failure_type,
                "circuit breaker: non-tripping failure (not incrementing counter)"
            );
            return;
        }

        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());

        match self.state {
            CircuitBreakerState::Closed => {
                if self.consecutive_failures >= self.threshold {
                    self.state = CircuitBreakerState::Open;
                    tracing::warn!(
                        failures = self.consecutive_failures,
                        failure_type = ?failure_type,
                        "circuit breaker OPEN"
                    );
                }
            }
            CircuitBreakerState::HalfOpen => {
                // HalfOpen + failure → Open (cooldown resets)
                self.state = CircuitBreakerState::Open;
            }
            CircuitBreakerState::Open => {}
        }
    }

    pub fn threshold(&self) -> u32 {
        self.threshold
    }
}

impl Default for CircuitBreaker {
    fn default() -> Self {
        Self::new(3, Duration::from_secs(60))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limit_does_not_trip() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        for _ in 0..10 {
            cb.record_classified_failure(FailureType::RateLimit);
        }
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn test_transient_errors_trip() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_classified_failure(FailureType::Transient);
        cb.record_classified_failure(FailureType::Transient);
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        cb.record_classified_failure(FailureType::Transient);
        assert_eq!(cb.state(), CircuitBreakerState::Open);
    }

    #[test]
    fn test_model_refusal_does_not_trip() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        for _ in 0..10 {
            cb.record_classified_failure(FailureType::ModelRefusal);
        }
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_classify_429() {
        assert_eq!(classify_llm_error("HTTP 429 Too Many Requests"), FailureType::RateLimit);
        assert_eq!(classify_llm_error("rate limit exceeded"), FailureType::RateLimit);
    }

    #[test]
    fn test_classify_500() {
        assert_eq!(classify_llm_error("HTTP 500 Internal Server Error"), FailureType::Transient);
        assert_eq!(classify_llm_error("server overloaded"), FailureType::Transient);
    }

    #[test]
    fn test_classify_auth() {
        assert_eq!(classify_llm_error("401 Unauthorized"), FailureType::AuthFailure);
        assert_eq!(classify_llm_error("Invalid API key provided"), FailureType::AuthFailure);
    }

    #[test]
    fn test_classify_refusal() {
        assert_eq!(classify_llm_error("content_policy_violation"), FailureType::ModelRefusal);
        assert_eq!(classify_llm_error("context_length_exceeded"), FailureType::ModelRefusal);
    }
}
