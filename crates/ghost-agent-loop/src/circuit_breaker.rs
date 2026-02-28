//! Tool-level circuit breaker (Req 12 AC1–AC3, AC6).
//!
//! 3 states: Closed, Open, HalfOpen.
//! INDEPENDENT from the provider circuit breaker in ghost-llm (A22.2).
//! Policy denials do NOT increment this circuit breaker (AC6).

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
    pub fn record_failure(&mut self) {
        self.consecutive_failures += 1;
        self.last_failure = Some(Instant::now());

        match self.state {
            CircuitBreakerState::Closed => {
                if self.consecutive_failures >= self.threshold {
                    self.state = CircuitBreakerState::Open;
                    tracing::warn!(
                        failures = self.consecutive_failures,
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
