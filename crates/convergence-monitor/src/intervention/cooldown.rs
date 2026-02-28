//! Cooldown management and config time-locking (A8).

use serde::{Deserialize, Serialize};

/// Cooldown state for config time-locking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownManager {
    /// Whether config changes are locked (active session in progress).
    pub config_locked: bool,
    /// Minimum floor for all thresholds.
    pub threshold_floor: f64,
}

impl CooldownManager {
    pub fn new() -> Self {
        Self {
            config_locked: false,
            threshold_floor: 0.1,
        }
    }

    /// Lock config during active sessions.
    pub fn lock_config(&mut self) {
        self.config_locked = true;
    }

    /// Unlock config during cooldown periods.
    pub fn unlock_config(&mut self) {
        self.config_locked = false;
    }

    /// Check if a threshold change is allowed.
    ///
    /// - Always allow raising thresholds (more conservative).
    /// - Reject lowering during active sessions.
    /// - Enforce minimum floor.
    pub fn can_change_threshold(
        &self,
        current: f64,
        proposed: f64,
    ) -> bool {
        // Always allow raising thresholds
        if proposed >= current {
            return true;
        }

        // Reject lowering during lock
        if self.config_locked {
            return false;
        }

        // Enforce floor
        proposed >= self.threshold_floor
    }
}

impl Default for CooldownManager {
    fn default() -> Self {
        Self::new()
    }
}
