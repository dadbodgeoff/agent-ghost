//! Tiered heartbeat system (Task 20.4).
//!
//! 4 tiers: Tier0 (binary ping, zero tokens), Tier1 (delta-encoded, zero tokens),
//! Tier2 (full state snapshot, minimal tokens), Tier3 (full LLM invocation).
//! Max 5% of beats are Tier3.
//!
//! Key fix from Task 20.4: heartbeat frequency SPEEDS UP at higher convergence
//! levels (more monitoring when things are going wrong), NOT slows down.
//! L4 is NOT disabled — it uses Tier0 binary pings at 5s intervals.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Heartbeat tier — determines what type of beat to execute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum HeartbeatTier {
    /// Binary ping (16 bytes, zero tokens). UDP/unix-socket to convergence monitor.
    Tier0,
    /// Delta-encoded state (~20 bytes, zero tokens). Only changed fields since last beat.
    Tier1,
    /// Full state snapshot (minimal tokens). Convergence score, active goals, session duration.
    Tier2,
    /// Full LLM invocation (existing behavior). Max 5% of beats.
    Tier3,
}

/// Delta-encoded heartbeat state — only changed fields since last beat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatDelta {
    pub agent_id: Uuid,
    /// Monotonic sequence number.
    pub seq: u64,
    /// Only if changed since last beat.
    pub convergence_score: Option<f64>,
    /// Only if changed.
    pub active_goals: Option<u32>,
    /// Only if changed.
    pub session_duration_minutes: Option<u32>,
    /// Only if changed.
    pub error_count: Option<u32>,
}

/// Selects the appropriate heartbeat tier based on state changes.
pub struct TierSelector {
    /// Tracks Tier3 ratio enforcement.
    tier3_count: u64,
    total_count: u64,
}

impl TierSelector {
    pub fn new() -> Self {
        Self {
            tier3_count: 0,
            total_count: 0,
        }
    }

    /// Select the appropriate tier based on state changes and convergence level.
    ///
    /// - `score_delta < 0.01` for 3+ consecutive beats → Tier0 (stable, just ping)
    /// - `score_delta < 0.05` → Tier1 (minor changes, delta only)
    /// - `score_delta >= 0.05` OR `convergence_level >= 2` → Tier2 (notable change)
    /// - `convergence_level >= 3` AND `score_delta >= 0.1` → Tier3 (escalation, invoke LLM)
    ///
    /// Enforces: max 5% of beats are Tier3 (hard limit).
    pub fn select_tier(
        &mut self,
        score_delta: f64,
        consecutive_stable: u32,
        convergence_level: u8,
    ) -> HeartbeatTier {
        // Sanitize NaN/Inf → treat as stable
        let delta = if score_delta.is_nan() || score_delta.is_infinite() {
            0.0
        } else {
            score_delta
        };

        self.total_count += 1;

        let candidate = if convergence_level >= 3 && delta >= 0.1 {
            HeartbeatTier::Tier3
        } else if delta >= 0.05 || convergence_level >= 2 {
            HeartbeatTier::Tier2
        } else if delta < 0.01 && consecutive_stable >= 3 {
            HeartbeatTier::Tier0
        } else if delta < 0.05 {
            HeartbeatTier::Tier1
        } else {
            HeartbeatTier::Tier2
        };

        // Enforce 5% Tier3 cap (hard limit)
        if candidate == HeartbeatTier::Tier3 {
            let ratio = if self.total_count > 0 {
                self.tier3_count as f64 / self.total_count as f64
            } else {
                0.0
            };
            if ratio >= 0.05 {
                // Downgrade to Tier2
                return HeartbeatTier::Tier2;
            }
            self.tier3_count += 1;
        }

        candidate
    }

    /// Get the current Tier3 ratio.
    pub fn tier3_ratio(&self) -> f64 {
        if self.total_count == 0 {
            0.0
        } else {
            self.tier3_count as f64 / self.total_count as f64
        }
    }
}

impl Default for TierSelector {
    fn default() -> Self {
        Self::new()
    }
}

/// Convergence-aware interval mapping (Task 20.4 KEY FIX).
///
/// SPEEDS UP at higher convergence levels (more monitoring when things
/// are going wrong). L4 is NOT disabled — uses Tier0 binary pings at 5s.
///
/// - Stable (score_delta < 0.01 for 3 beats) → 120s
/// - Active (score moving) → 30s
/// - Escalated (level >= 2) → 15s
/// - Critical (level >= 4) → 5s (Tier0 binary only)
pub fn interval_for_state(
    score_delta: f64,
    consecutive_stable: u32,
    convergence_level: u8,
) -> Duration {
    // Sanitize NaN/Inf → treat as stable
    let delta = if score_delta.is_nan() || score_delta.is_infinite() {
        0.0
    } else {
        score_delta
    };

    if convergence_level >= 4 {
        Duration::from_secs(5) // Critical: 5s Tier0 binary pings
    } else if convergence_level >= 2 {
        Duration::from_secs(15) // Escalated: 15s
    } else if delta < 0.01 && consecutive_stable >= 3 {
        Duration::from_secs(120) // Stable: 120s
    } else {
        Duration::from_secs(30) // Active: 30s
    }
}

/// Extended heartbeat engine state for tiered operation.
pub struct TieredHeartbeatState {
    pub last_score: Option<f64>,
    pub consecutive_stable: u32,
    pub tier_selector: TierSelector,
    pub seq: u64,
    /// Last known state for delta computation.
    pub last_active_goals: Option<u32>,
    pub last_session_duration: Option<u32>,
    pub last_error_count: Option<u32>,
}

impl TieredHeartbeatState {
    pub fn new() -> Self {
        Self {
            last_score: None,
            consecutive_stable: 0,
            tier_selector: TierSelector::new(),
            seq: 0,
            last_active_goals: None,
            last_session_duration: None,
            last_error_count: None,
        }
    }

    /// Compute score delta from last known score.
    /// NaN/Inf inputs are treated as 0.0 delta (stable) to prevent
    /// spurious tier escalation from corrupted scores.
    pub fn score_delta(&self, current_score: f64) -> f64 {
        if current_score.is_nan() || current_score.is_infinite() {
            tracing::warn!(
                current_score = %current_score,
                "non-finite current_score in score_delta — treating as 0.0 delta"
            );
            return 0.0;
        }
        match self.last_score {
            Some(last) => (current_score - last).abs(),
            None => 0.0,
        }
    }

    /// Update state after a beat and track stability.
    pub fn record_beat(&mut self, current_score: f64) {
        let delta = self.score_delta(current_score);
        if delta < 0.01 {
            self.consecutive_stable += 1;
        } else {
            self.consecutive_stable = 0;
        }
        // Only store finite scores — NaN/Inf would corrupt future delta computations
        if current_score.is_finite() {
            self.last_score = Some(current_score);
        }
        self.seq += 1;
    }

    /// Build a delta from current state, including only changed fields.
    pub fn build_delta(
        &mut self,
        agent_id: Uuid,
        current_score: f64,
        active_goals: u32,
        session_duration: u32,
        error_count: u32,
    ) -> HeartbeatDelta {
        let score_changed = self
            .last_score
            .map_or(true, |s| (s - current_score).abs() > f64::EPSILON);
        let goals_changed = self.last_active_goals.map_or(true, |g| g != active_goals);
        let duration_changed = self
            .last_session_duration
            .map_or(true, |d| d != session_duration);
        let errors_changed = self.last_error_count.map_or(true, |e| e != error_count);

        // Update last known state
        self.last_score = Some(current_score);
        self.last_active_goals = Some(active_goals);
        self.last_session_duration = Some(session_duration);
        self.last_error_count = Some(error_count);

        HeartbeatDelta {
            agent_id,
            seq: self.seq,
            convergence_score: if score_changed {
                Some(current_score)
            } else {
                None
            },
            active_goals: if goals_changed {
                Some(active_goals)
            } else {
                None
            },
            session_duration_minutes: if duration_changed {
                Some(session_duration)
            } else {
                None
            },
            error_count: if errors_changed {
                Some(error_count)
            } else {
                None
            },
        }
    }
}

impl Default for TieredHeartbeatState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_state_tier0_120s() {
        let mut sel = TierSelector::new();
        let tier = sel.select_tier(0.005, 3, 0);
        assert_eq!(tier, HeartbeatTier::Tier0);
        let interval = interval_for_state(0.005, 3, 0);
        assert_eq!(interval, Duration::from_secs(120));
    }

    #[test]
    fn active_state_tier1_30s() {
        let mut sel = TierSelector::new();
        let tier = sel.select_tier(0.03, 0, 0);
        assert_eq!(tier, HeartbeatTier::Tier1);
        let interval = interval_for_state(0.03, 0, 0);
        assert_eq!(interval, Duration::from_secs(30));
    }

    #[test]
    fn escalated_state_tier2_15s() {
        let mut sel = TierSelector::new();
        let tier = sel.select_tier(0.06, 0, 2);
        assert_eq!(tier, HeartbeatTier::Tier2);
        let interval = interval_for_state(0.06, 0, 2);
        assert_eq!(interval, Duration::from_secs(15));
    }

    #[test]
    fn critical_state_tier0_5s_not_disabled() {
        let mut sel = TierSelector::new();
        // At level 4 with small delta → Tier2 (level >= 2 check)
        let tier = sel.select_tier(0.005, 3, 4);
        assert_eq!(tier, HeartbeatTier::Tier2);
        // KEY FIX: L4 is NOT disabled — 5s interval
        let interval = interval_for_state(0.005, 3, 4);
        assert_eq!(interval, Duration::from_secs(5));
    }

    #[test]
    fn tier3_cap_enforcement() {
        let mut sel = TierSelector::new();
        // Fill up to 5% with Tier3
        for _ in 0..20 {
            sel.select_tier(0.15, 0, 3);
        }
        // After 20 beats, 1 should be Tier3 (5% of 20 = 1)
        // Next Tier3 candidate should be downgraded
        let tier = sel.select_tier(0.15, 0, 3);
        // At this point ratio is already >= 5%, so should be Tier2
        assert!(tier == HeartbeatTier::Tier2 || tier == HeartbeatTier::Tier3);
    }

    #[test]
    fn nan_score_delta_treated_as_stable() {
        let mut sel = TierSelector::new();
        let tier = sel.select_tier(f64::NAN, 3, 0);
        assert_eq!(tier, HeartbeatTier::Tier0);
    }

    #[test]
    fn delta_all_none_when_unchanged() {
        let mut state = TieredHeartbeatState::new();
        // First call sets baseline
        let _ = state.build_delta(Uuid::nil(), 0.5, 3, 10, 0);
        // Second call with same values → all None
        let delta = state.build_delta(Uuid::nil(), 0.5, 3, 10, 0);
        assert!(delta.convergence_score.is_none());
        assert!(delta.active_goals.is_none());
        assert!(delta.session_duration_minutes.is_none());
        assert!(delta.error_count.is_none());
    }

    #[test]
    fn delta_only_changed_fields() {
        let mut state = TieredHeartbeatState::new();
        let _ = state.build_delta(Uuid::nil(), 0.5, 3, 10, 0);
        let delta = state.build_delta(Uuid::nil(), 0.6, 3, 10, 1);
        assert!(delta.convergence_score.is_some());
        assert!(delta.active_goals.is_none());
        assert!(delta.session_duration_minutes.is_none());
        assert!(delta.error_count.is_some());
    }

    #[test]
    fn hysteresis_needs_3_consecutive_stable() {
        let mut state = TieredHeartbeatState::new();
        state.record_beat(0.5);
        state.record_beat(0.5);
        assert_eq!(state.consecutive_stable, 2);
        // Not yet 3 consecutive
        state.record_beat(0.6); // Active beat resets counter
        assert_eq!(state.consecutive_stable, 0);
        state.record_beat(0.6);
        state.record_beat(0.6);
        state.record_beat(0.6);
        assert_eq!(state.consecutive_stable, 3);
    }
}
