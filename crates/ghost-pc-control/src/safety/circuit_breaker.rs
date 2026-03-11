//! Circuit breaker for PC control actions.
//!
//! Prevents runaway action loops by monitoring:
//! - **Rate limiting**: Too many actions per second trips the breaker.
//! - **Failure counting**: Consecutive failures trip the breaker.
//! - **Cooldown**: After tripping, the breaker stays open for a
//!   configurable duration before allowing a single probe action.
//!
//! State machine: `Closed` → `Open` → `HalfOpen` → `Closed` (on success)
//!                                                 → `Open` (on failure)

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ghost_skills::skill::SkillError;

/// Circuit breaker states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// Normal operation — actions are allowed.
    Closed,
    /// Tripped — all actions are blocked until cooldown expires.
    Open,
    /// Cooldown expired — one probe action is allowed.
    HalfOpen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CircuitBreakerSettings {
    pub rate_limit: u32,
    pub failure_threshold: u32,
    pub cooldown: Duration,
}

/// PC control circuit breaker.
///
/// Protected by `Mutex` in the skill layer — all access is serialized.
pub struct PcControlCircuitBreaker {
    /// Rolling window of recent action timestamps.
    recent_actions: VecDeque<Instant>,

    /// Maximum actions per second.
    rate_limit: u32,

    /// Consecutive failure counter.
    failure_count: u32,

    /// Failures before tripping.
    failure_threshold: u32,

    /// Current state.
    state: CircuitState,

    /// When the breaker tripped (for cooldown calculation).
    tripped_at: Option<Instant>,

    /// How long to stay open before transitioning to half-open.
    cooldown: Duration,
}

impl PcControlCircuitBreaker {
    /// Create a new circuit breaker with the given parameters.
    pub fn new(rate_limit: u32, failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            recent_actions: VecDeque::with_capacity(rate_limit as usize + 1),
            rate_limit,
            failure_count: 0,
            failure_threshold,
            state: CircuitState::Closed,
            tripped_at: None,
            cooldown,
        }
    }

    /// Current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }

    pub fn settings(&self) -> CircuitBreakerSettings {
        CircuitBreakerSettings {
            rate_limit: self.rate_limit,
            failure_threshold: self.failure_threshold,
            cooldown: self.cooldown,
        }
    }

    /// Check whether an action is allowed. Returns `Ok(())` if allowed,
    /// or `Err(SkillError::CircuitBreakerOpen)` if blocked.
    pub fn check(&mut self, action: &str) -> Result<(), SkillError> {
        match self.state {
            CircuitState::Open => {
                // Check if cooldown has expired.
                if let Some(tripped_at) = self.tripped_at {
                    if tripped_at.elapsed() >= self.cooldown {
                        tracing::info!(
                            action,
                            "Circuit breaker cooldown expired — transitioning to half-open"
                        );
                        self.state = CircuitState::HalfOpen;
                        // Fall through to half-open handling.
                    } else {
                        let remaining = self.cooldown - tripped_at.elapsed();
                        return Err(SkillError::CircuitBreakerOpen(format!(
                            "circuit breaker open, cooldown remaining: {:.1}s",
                            remaining.as_secs_f64(),
                        )));
                    }
                } else {
                    return Err(SkillError::CircuitBreakerOpen(
                        "circuit breaker open".into(),
                    ));
                }
                // HalfOpen: allow one probe.
                self.record_action();
                Ok(())
            }
            CircuitState::HalfOpen => {
                // Allow one probe action.
                self.record_action();
                Ok(())
            }
            CircuitState::Closed => {
                // Check rate limit.
                let now = Instant::now();
                self.recent_actions.push_back(now);

                // Remove actions older than 1 second.
                while let Some(&front) = self.recent_actions.front() {
                    if now.duration_since(front) >= Duration::from_secs(1) {
                        self.recent_actions.pop_front();
                    } else {
                        break;
                    }
                }

                if self.recent_actions.len() > self.rate_limit as usize {
                    self.trip("rate limit exceeded");
                    return Err(SkillError::CircuitBreakerOpen(format!(
                        "rate limit exceeded: {} actions/sec (limit: {})",
                        self.recent_actions.len(),
                        self.rate_limit,
                    )));
                }

                Ok(())
            }
        }
    }

    /// Record a successful action. Resets failure counter and closes
    /// the breaker if it was half-open.
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        if self.state == CircuitState::HalfOpen {
            tracing::info!("Circuit breaker probe succeeded — closing");
            self.state = CircuitState::Closed;
            self.tripped_at = None;
        }
    }

    /// Record a failed action. Increments failure counter and may trip
    /// the breaker.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;

        if self.state == CircuitState::HalfOpen {
            // Probe failed — reopen.
            tracing::warn!("Circuit breaker probe failed — reopening");
            self.trip("probe failed in half-open state");
            return;
        }

        if self.failure_count >= self.failure_threshold {
            self.trip(&format!(
                "{} consecutive failures (threshold: {})",
                self.failure_count, self.failure_threshold,
            ));
        }
    }

    /// Manually reset the circuit breaker to closed state.
    pub fn reset(&mut self) {
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.tripped_at = None;
        self.recent_actions.clear();
    }

    pub fn reconfigure(&mut self, rate_limit: u32, failure_threshold: u32, cooldown: Duration) {
        self.rate_limit = rate_limit;
        self.failure_threshold = failure_threshold;
        self.cooldown = cooldown;
        self.recent_actions
            .truncate(rate_limit.saturating_add(1) as usize);
    }

    fn trip(&mut self, reason: &str) {
        tracing::warn!(reason, "PC control circuit breaker tripped");
        self.state = CircuitState::Open;
        self.tripped_at = Some(Instant::now());
    }

    fn record_action(&mut self) {
        self.recent_actions.push_back(Instant::now());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_closed() {
        let cb = PcControlCircuitBreaker::new(5, 3, Duration::from_secs(30));
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn allows_actions_below_rate_limit() {
        let mut cb = PcControlCircuitBreaker::new(10, 3, Duration::from_secs(30));
        for _ in 0..10 {
            assert!(cb.check("test").is_ok());
        }
    }

    #[test]
    fn trips_on_rate_limit() {
        let mut cb = PcControlCircuitBreaker::new(3, 10, Duration::from_secs(30));

        // First 3 should succeed.
        for _ in 0..3 {
            assert!(cb.check("test").is_ok());
        }

        // Fourth should trip the breaker.
        let result = cb.check("test");
        assert!(result.is_err());
        assert_eq!(cb.state(), CircuitState::Open);

        match result.unwrap_err() {
            SkillError::CircuitBreakerOpen(msg) => {
                assert!(msg.contains("rate limit"));
            }
            other => panic!("Expected CircuitBreakerOpen, got: {other:?}"),
        }
    }

    #[test]
    fn trips_on_repeated_failures() {
        let mut cb = PcControlCircuitBreaker::new(100, 3, Duration::from_secs(30));

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed);

        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn success_resets_failure_count() {
        let mut cb = PcControlCircuitBreaker::new(100, 3, Duration::from_secs(30));

        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        cb.record_failure();
        cb.record_failure();

        // Should still be closed — failures were reset by success.
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn blocks_actions_when_open() {
        let mut cb = PcControlCircuitBreaker::new(100, 1, Duration::from_secs(300));

        cb.record_failure(); // Trips immediately (threshold = 1).
        assert_eq!(cb.state(), CircuitState::Open);

        let result = cb.check("test");
        assert!(result.is_err());
    }

    #[test]
    fn transitions_to_half_open_after_cooldown() {
        let mut cb = PcControlCircuitBreaker::new(100, 1, Duration::from_millis(1));

        cb.record_failure(); // Trip.
        assert_eq!(cb.state(), CircuitState::Open);

        // Wait for cooldown.
        std::thread::sleep(Duration::from_millis(5));

        // Should transition to half-open and allow one probe.
        assert!(cb.check("probe").is_ok());
        // State should be HalfOpen (or Closed if the action itself transitions).
        // After check succeeds in HalfOpen, we need record_success to close.
    }

    #[test]
    fn half_open_success_closes_breaker() {
        let mut cb = PcControlCircuitBreaker::new(100, 1, Duration::from_millis(1));

        cb.record_failure(); // Trip.
        std::thread::sleep(Duration::from_millis(5));

        cb.check("probe").unwrap(); // Transitions to half-open, allows probe.
        cb.record_success(); // Should close.

        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn half_open_failure_reopens() {
        let mut cb = PcControlCircuitBreaker::new(100, 1, Duration::from_millis(1));

        cb.record_failure(); // Trip.
        std::thread::sleep(Duration::from_millis(5));

        cb.check("probe").unwrap(); // Half-open probe.
        cb.record_failure(); // Probe failed → reopen.

        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn manual_reset() {
        let mut cb = PcControlCircuitBreaker::new(100, 1, Duration::from_secs(300));

        cb.record_failure(); // Trip.
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert!(cb.check("test").is_ok());
    }
}
