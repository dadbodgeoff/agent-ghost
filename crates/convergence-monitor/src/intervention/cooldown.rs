//! Cooldown management and config time-locking (A8).
//!
//! - Config locked during active sessions.
//! - Raising thresholds always allowed (more conservative).
//! - Lowering thresholds rejected during lock.
//! - Dual-key required for critical changes (e.g., disabling convergence).
//! - Minimum floor enforced on all thresholds.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PendingCriticalAction {
    ThresholdChange { current: f64, proposed: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingDualKeyChange {
    pub token_hash: String,
    pub issued_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub initiator: String,
    pub intended_action: String,
    pub action: PendingCriticalAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DualKeyInitiationResult {
    Initiated {
        token: String,
        expires_at: chrono::DateTime<chrono::Utc>,
    },
    AlreadyPending {
        intended_action: String,
        expires_at: chrono::DateTime<chrono::Utc>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum DualKeyConfirmationResult {
    Confirmed {
        pending_change: PendingDualKeyChange,
    },
    MissingPending,
    InvalidToken,
    Expired {
        pending_change: PendingDualKeyChange,
    },
    SameActorRejected,
}

/// Cooldown state for config time-locking.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CooldownManager {
    /// Whether config changes are locked (active session in progress).
    pub config_locked: bool,
    /// Minimum floor for all thresholds.
    pub threshold_floor: f64,
    /// Pending critical change awaiting second confirmation.
    pub pending_dual_key_change: Option<PendingDualKeyChange>,
}

impl CooldownManager {
    pub fn new() -> Self {
        Self {
            config_locked: false,
            threshold_floor: 0.1,
            pending_dual_key_change: None,
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
    pub fn can_change_threshold(&self, current: f64, proposed: f64) -> bool {
        if proposed < self.threshold_floor {
            return false;
        }

        // Always allow raising thresholds
        if proposed >= current {
            return true;
        }

        // Reject lowering during lock
        if self.config_locked {
            return false;
        }

        true
    }

    /// Initiate a dual-key critical change (A8).
    ///
    /// Critical changes (e.g., disabling convergence monitoring, lowering
    /// kill thresholds below floor) require two independent confirmations.
    /// Returns a token that must be confirmed by a second key holder.
    pub fn initiate_dual_key_change(
        &mut self,
        initiator: impl Into<String>,
        action: PendingCriticalAction,
        now: chrono::DateTime<chrono::Utc>,
        ttl: std::time::Duration,
    ) -> DualKeyInitiationResult {
        self.prune_expired_dual_key_change(now);

        if let Some(pending) = self.pending_dual_key_change.as_ref() {
            return DualKeyInitiationResult::AlreadyPending {
                intended_action: pending.intended_action.clone(),
                expires_at: pending.expires_at,
            };
        }

        let token = format!("dualkey-{}", uuid::Uuid::now_v7());
        let expires_at =
            now + chrono::Duration::from_std(ttl).unwrap_or_else(|_| chrono::Duration::minutes(5));
        let initiator = initiator.into();
        let intended_action = match &action {
            PendingCriticalAction::ThresholdChange { current, proposed } => {
                format!("threshold_change:{current}->{proposed}")
            }
        };
        self.pending_dual_key_change = Some(PendingDualKeyChange {
            token_hash: hash_token(&token),
            issued_at: now,
            expires_at,
            initiator,
            intended_action,
            action,
        });
        DualKeyInitiationResult::Initiated { token, expires_at }
    }

    /// Confirm a dual-key critical change with the token from the first key.
    /// A second actor must confirm before the pending action can proceed.
    pub fn confirm_dual_key_change(
        &mut self,
        token: &str,
        confirmer: &str,
        now: chrono::DateTime<chrono::Utc>,
    ) -> DualKeyConfirmationResult {
        if let Some(expired) = self.prune_expired_dual_key_change(now) {
            return DualKeyConfirmationResult::Expired {
                pending_change: expired,
            };
        }

        let Some(pending) = self.pending_dual_key_change.as_ref() else {
            return DualKeyConfirmationResult::MissingPending;
        };

        if pending.initiator == confirmer {
            return DualKeyConfirmationResult::SameActorRejected;
        }

        if pending.token_hash == hash_token(token) {
            let pending_change = pending.clone();
            self.pending_dual_key_change = None;
            DualKeyConfirmationResult::Confirmed { pending_change }
        } else {
            DualKeyConfirmationResult::InvalidToken
        }
    }

    /// Cancel a pending dual-key change.
    #[allow(dead_code)]
    pub fn cancel_dual_key_change(&mut self) {
        self.pending_dual_key_change = None;
    }

    #[cfg(test)]
    pub fn pending_dual_key_change(&self) -> Option<&PendingDualKeyChange> {
        self.pending_dual_key_change.as_ref()
    }

    pub fn prune_expired_dual_key_change(
        &mut self,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Option<PendingDualKeyChange> {
        let expired = self
            .pending_dual_key_change
            .as_ref()
            .is_some_and(|pending| pending.expires_at <= now);
        if expired {
            self.pending_dual_key_change.take()
        } else {
            None
        }
    }

    /// Check if a change is critical and requires dual-key confirmation.
    ///
    /// Critical changes:
    /// - Disabling convergence monitoring entirely
    /// - Lowering safety-critical thresholds above the enforced floor
    /// - Removing safety-critical triggers
    pub fn is_critical_change(&self, current: f64, proposed: f64) -> bool {
        proposed < current && proposed >= self.threshold_floor
    }
}

impl Default for CooldownManager {
    fn default() -> Self {
        Self::new()
    }
}

fn hash_token(token: &str) -> String {
    blake3::hash(token.as_bytes()).to_hex().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dual_key_tokens_expire_and_cannot_be_reused() {
        let mut manager = CooldownManager::new();
        let now = chrono::Utc::now();
        let DualKeyInitiationResult::Initiated { token, expires_at } = manager
            .initiate_dual_key_change(
                "api",
                PendingCriticalAction::ThresholdChange {
                    current: 0.4,
                    proposed: 0.2,
                },
                now,
                std::time::Duration::from_secs(300),
            )
        else {
            panic!("expected change initiation to succeed");
        };

        let pending = manager.pending_dual_key_change().unwrap().clone();
        assert_eq!(pending.initiator, "api");
        assert_eq!(pending.intended_action, "threshold_change:0.4->0.2");
        assert_eq!(pending.expires_at, expires_at);

        assert_eq!(
            manager.confirm_dual_key_change(
                &token,
                "reviewer",
                now + chrono::Duration::seconds(60)
            ),
            DualKeyConfirmationResult::Confirmed {
                pending_change: pending.clone()
            }
        );
        assert_eq!(
            manager.confirm_dual_key_change(
                &token,
                "reviewer",
                now + chrono::Duration::seconds(61)
            ),
            DualKeyConfirmationResult::MissingPending
        );

        let DualKeyInitiationResult::Initiated {
            token: expired_token,
            ..
        } = manager.initiate_dual_key_change(
            "api",
            PendingCriticalAction::ThresholdChange {
                current: 0.4,
                proposed: 0.2,
            },
            now,
            std::time::Duration::from_secs(1),
        )
        else {
            panic!("expected expired token initiation to succeed");
        };
        assert_eq!(
            manager.confirm_dual_key_change(
                &expired_token,
                "reviewer",
                now + chrono::Duration::seconds(5)
            ),
            DualKeyConfirmationResult::Expired {
                pending_change: PendingDualKeyChange {
                    token_hash: hash_token(&expired_token),
                    issued_at: now,
                    expires_at: now + chrono::Duration::seconds(1),
                    initiator: "api".to_string(),
                    intended_action: "threshold_change:0.4->0.2".to_string(),
                    action: PendingCriticalAction::ThresholdChange {
                        current: 0.4,
                        proposed: 0.2,
                    },
                }
            }
        );
    }

    #[test]
    fn dual_key_change_cannot_be_self_confirmed_or_silently_replaced() {
        let mut manager = CooldownManager::new();
        let now = chrono::Utc::now();
        let first = manager.initiate_dual_key_change(
            "api",
            PendingCriticalAction::ThresholdChange {
                current: 0.5,
                proposed: 0.4,
            },
            now,
            std::time::Duration::from_secs(300),
        );
        let DualKeyInitiationResult::Initiated { token, expires_at } = first else {
            panic!("expected first initiation to succeed");
        };

        assert_eq!(
            manager.initiate_dual_key_change(
                "api-2",
                PendingCriticalAction::ThresholdChange {
                    current: 0.6,
                    proposed: 0.4,
                },
                now + chrono::Duration::seconds(5),
                std::time::Duration::from_secs(300),
            ),
            DualKeyInitiationResult::AlreadyPending {
                intended_action: "threshold_change:0.5->0.4".to_string(),
                expires_at
            }
        );
        assert_eq!(
            manager.confirm_dual_key_change(&token, "api", now + chrono::Duration::seconds(10)),
            DualKeyConfirmationResult::SameActorRejected
        );
        assert!(manager.pending_dual_key_change().is_some());
    }

    #[test]
    fn floor_is_enforced_even_for_lowering_attempts() {
        let manager = CooldownManager::new();
        assert!(!manager.can_change_threshold(0.85, 0.05));
        assert!(!manager.is_critical_change(0.85, 0.05));
        assert!(manager.is_critical_change(0.85, 0.8));
    }
}
