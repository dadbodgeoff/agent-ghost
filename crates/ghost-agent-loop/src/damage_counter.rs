//! Damage counter — monotonically non-decreasing failure counter (Req 12 AC4–AC5).
//!
//! Monotonically non-decreasing within a session. Halts at threshold.
//! Resets between sessions via `reset()` so previous session damage
//! does not block future sessions.
//! Independent from CircuitBreaker (AC5).

/// Monotonically non-decreasing damage counter.
pub struct DamageCounter {
    count: u32,
    threshold: u32,
}

impl DamageCounter {
    pub fn new(threshold: u32) -> Self {
        Self {
            count: 0,
            threshold,
        }
    }

    /// Increment the damage counter. Never decrements.
    pub fn increment(&mut self) {
        self.count += 1;
        if self.count >= self.threshold {
            tracing::error!(
                count = self.count,
                threshold = self.threshold,
                "damage counter threshold reached — halting run"
            );
        }
    }

    /// Check if the threshold has been reached (GATE 1.5).
    pub fn is_halted(&self) -> bool {
        self.count >= self.threshold
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn threshold(&self) -> u32 {
        self.threshold
    }

    /// Reset the counter to zero for a new session.
    /// Threshold is preserved. Called between sessions so a previous
    /// session's damage does not block future sessions.
    pub fn reset(&mut self) {
        tracing::debug!(
            previous_count = self.count,
            threshold = self.threshold,
            "damage counter reset for new session"
        );
        self.count = 0;
    }

    /// Update the threshold value.
    pub fn set_threshold(&mut self, threshold: u32) {
        self.threshold = threshold;
    }
}

impl Default for DamageCounter {
    fn default() -> Self {
        Self::new(5)
    }
}
