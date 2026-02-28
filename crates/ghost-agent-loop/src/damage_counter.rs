//! Damage counter — monotonically non-decreasing failure counter (Req 12 AC4–AC5).
//!
//! Never resets within a run. Halts at threshold.
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
}

impl Default for DamageCounter {
    fn default() -> Self {
        Self::new(5)
    }
}
