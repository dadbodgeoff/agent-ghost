//! Cooldown management and config time-locking (A8).
//!
//! - Config locked during active sessions.
//! - Raising thresholds always allowed (more conservative).
//! - Lowering thresholds rejected during lock.
//! - Dual-key required for critical changes (e.g., disabling convergence).
//! - Minimum floor enforced on all thresholds.

use serde::{Deserialize, Serialize};

/// Cooldown state for config time-locking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownManager {
    /// Whether config changes are locked (active session in progress).
    pub config_locked: bool,
    /// Minimum floor for all thresholds.
    pub threshold_floor: f64,
    /// Whether a dual-key confirmation is pending for a critical change.
    pub dual_key_pending: bool,
    /// The dual-key confirmation token (set by first key, verified by second).
    pub dual_key_token: Option<String>,
}

impl CooldownManager {
    pub fn new() -> Self {
        Self {
            config_locked: false,
            threshold_floor: 0.1,
            dual_key_pending: false,
            dual_key_token: None,
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

    /// Initiate a dual-key critical change (A8).
    ///
    /// Critical changes (e.g., disabling convergence monitoring, lowering
    /// kill thresholds below floor) require two independent confirmations.
    /// Returns a token that must be confirmed by a second key holder.
    pub fn initiate_dual_key_change(&mut self) -> String {
        let token = format!("dualkey-{}", uuid::Uuid::now_v7());
        self.dual_key_pending = true;
        self.dual_key_token = Some(token.clone());
        token
    }

    /// Confirm a dual-key critical change with the token from the first key.
    /// Returns true if the confirmation is valid and the change can proceed.
    pub fn confirm_dual_key_change(&mut self, token: &str) -> bool {
        if !self.dual_key_pending {
            return false;
        }
        if self.dual_key_token.as_deref() == Some(token) {
            self.dual_key_pending = false;
            self.dual_key_token = None;
            true
        } else {
            false
        }
    }

    /// Cancel a pending dual-key change.
    pub fn cancel_dual_key_change(&mut self) {
        self.dual_key_pending = false;
        self.dual_key_token = None;
    }

    /// Check if a change is critical and requires dual-key confirmation.
    ///
    /// Critical changes:
    /// - Disabling convergence monitoring entirely
    /// - Lowering kill thresholds below the floor
    /// - Removing safety-critical triggers
    pub fn is_critical_change(&self, proposed: f64) -> bool {
        proposed < self.threshold_floor
    }
}

impl Default for CooldownManager {
    fn default() -> Self {
        Self::new()
    }
}
