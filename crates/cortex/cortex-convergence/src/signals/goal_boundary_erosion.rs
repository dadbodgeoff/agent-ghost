//! S5: Goal boundary erosion (Jensen-Shannon divergence).
//! Throttled to every 5th message per AC11. Requires Standard privacy.

use super::{PrivacyLevel, Signal, SignalInput};

pub struct GoalBoundaryErosionSignal {
    /// Cached value for throttling.
    cached_value: std::sync::atomic::AtomicU64,
}

impl GoalBoundaryErosionSignal {
    pub fn new() -> Self {
        Self {
            cached_value: std::sync::atomic::AtomicU64::new(0.0f64.to_bits()),
        }
    }

    fn get_cached(&self) -> f64 {
        f64::from_bits(self.cached_value.load(std::sync::atomic::Ordering::Relaxed))
    }

    fn set_cached(&self, val: f64) {
        self.cached_value.store(val.to_bits(), std::sync::atomic::Ordering::Relaxed);
    }
}

impl Default for GoalBoundaryErosionSignal {
    fn default() -> Self {
        Self::new()
    }
}

impl Signal for GoalBoundaryErosionSignal {
    fn id(&self) -> u8 { 5 }
    fn name(&self) -> &'static str { "goal_boundary_erosion" }
    fn requires_privacy_level(&self) -> PrivacyLevel { PrivacyLevel::Standard }

    fn compute(&self, data: &SignalInput) -> f64 {
        // Throttle: only recompute every 5th message (AC11)
        if data.message_index % 5 != 0 && data.message_index > 0 {
            return self.get_cached();
        }

        if data.existing_goal_tokens.is_empty() || data.proposed_goal_tokens.is_empty() {
            return 0.0;
        }

        // Jaccard distance as a proxy for goal drift
        let existing: std::collections::HashSet<&str> =
            data.existing_goal_tokens.iter().map(|s| s.as_str()).collect();
        let proposed: std::collections::HashSet<&str> =
            data.proposed_goal_tokens.iter().map(|s| s.as_str()).collect();

        let intersection = existing.intersection(&proposed).count() as f64;
        let union = existing.union(&proposed).count() as f64;

        let similarity = if union > 0.0 { intersection / union } else { 1.0 };
        let erosion = (1.0 - similarity).clamp(0.0, 1.0);

        self.set_cached(erosion);
        erosion
    }
}
