//! Heartbeat engine: periodic synthetic messages to keep agents active (Req 34).
//!
//! - Configurable interval (default 30min)
//! - Dedicated session key: hash(agent_id, "heartbeat", agent_id)
//! - Synthetic message: "[HEARTBEAT] Check HEARTBEAT.md and act if needed."
//! - Tiered convergence-aware frequency (Task 20.4):
//!   Stable→120s, Active→30s, Escalated→15s, Critical→5s
//! - L4 is NOT disabled — uses Tier0 binary pings at 5s intervals
//! - Checks PLATFORM_KILLED and per-agent pause/quarantine before every execution

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::tiers::{interval_for_state, TieredHeartbeatState};

/// Errors from heartbeat operations.
#[derive(Debug, thiserror::Error)]
pub enum HeartbeatError {
    #[error("heartbeat run failed: {0}")]
    RunFailed(String),
}

/// The canonical heartbeat message.
pub const HEARTBEAT_MESSAGE: &str = "[HEARTBEAT] Check HEARTBEAT.md and act if needed.";

/// Heartbeat configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Base interval in minutes (default 30). Used only by deprecated interval_for_level.
    pub base_interval_minutes: u32,
    /// Active hours start (0-23, inclusive).
    pub active_hours_start: u8,
    /// Active hours end (0-23, inclusive).
    pub active_hours_end: u8,
    /// Timezone offset in hours from UTC.
    pub timezone_offset_hours: i32,
    /// Maximum cost per heartbeat cycle.
    pub cost_ceiling: f64,
}

impl Default for HeartbeatConfig {
    fn default() -> Self {
        Self {
            base_interval_minutes: 30,
            active_hours_start: 8,
            active_hours_end: 22,
            timezone_offset_hours: 0,
            cost_ceiling: 0.50,
        }
    }
}

/// Convergence-aware interval mapping (DEPRECATED).
///
/// **Deprecated**: This function SLOWS DOWN at higher convergence levels
/// (L0-1→30m, L2→60m, L3→120m, L4→disabled), which is WRONG per Task 20.4.
/// Use [`crate::tiers::interval_for_state()`] instead, which SPEEDS UP at
/// higher levels (Stable→120s, Active→30s, Escalated→15s, Critical→5s).
/// L4 is NOT disabled in the new implementation.
#[deprecated(
    since = "0.1.0",
    note = "Use ghost_heartbeat::tiers::interval_for_state() instead — this function incorrectly slows down at higher convergence levels"
)]
pub fn interval_for_level(base_minutes: u32, convergence_level: u8) -> Option<Duration> {
    match convergence_level {
        0 | 1 => Some(Duration::from_secs(base_minutes as u64 * 60)),
        2 => Some(Duration::from_secs(base_minutes as u64 * 60 * 2)),
        3 => Some(Duration::from_secs(base_minutes as u64 * 60 * 4)),
        _ => None, // L4+ → disabled
    }
}

/// Compute the dedicated heartbeat session key.
/// Deterministic: hash(agent_id, "heartbeat", agent_id).
pub fn heartbeat_session_key(agent_id: Uuid) -> Uuid {
    let input = format!("{}:heartbeat:{}", agent_id, agent_id);
    let hash = blake3::hash(input.as_bytes());
    let bytes: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
    Uuid::from_bytes(bytes)
}

/// Heartbeat engine state.
///
/// Uses [`TieredHeartbeatState`] and [`interval_for_state()`] (Task 20.4)
/// for convergence-aware frequency that SPEEDS UP at higher levels.
pub struct HeartbeatEngine {
    pub config: HeartbeatConfig,
    pub agent_id: Uuid,
    pub session_key: Uuid,
    pub platform_killed: Arc<AtomicBool>,
    pub last_beat: Option<DateTime<Utc>>,
    pub total_cost: f64,
    agent_paused: Arc<AtomicBool>,
    /// Tiered heartbeat state for convergence-aware frequency (Task 20.4).
    pub tiered_state: TieredHeartbeatState,
}

impl HeartbeatEngine {
    pub fn new(
        config: HeartbeatConfig,
        agent_id: Uuid,
        platform_killed: Arc<AtomicBool>,
        agent_paused: Arc<AtomicBool>,
    ) -> Self {
        let session_key = heartbeat_session_key(agent_id);
        Self {
            config,
            agent_id,
            session_key,
            platform_killed,
            last_beat: None,
            total_cost: 0.0,
            agent_paused,
            tiered_state: TieredHeartbeatState::new(),
        }
    }

    /// Check if the heartbeat should fire now.
    ///
    /// Uses `interval_for_state()` (Task 20.4) which SPEEDS UP at higher
    /// convergence levels. L4 is NOT disabled — uses 5s Tier0 binary pings.
    pub fn should_fire(&self, convergence_level: u8) -> bool {
        // Check kill switch
        if self.platform_killed.load(Ordering::SeqCst) {
            return false;
        }

        // Check agent pause
        if self.agent_paused.load(Ordering::SeqCst) {
            return false;
        }

        // Check cost ceiling
        if self.total_cost >= self.config.cost_ceiling {
            tracing::warn!(
                agent_id = %self.agent_id,
                cost = self.total_cost,
                ceiling = self.config.cost_ceiling,
                "Heartbeat cost ceiling reached"
            );
            return false;
        }

        // Task 20.4: Use interval_for_state() which SPEEDS UP at higher levels.
        // L4 is NOT disabled — uses 5s Tier0 binary pings.
        //
        // We don't have the *current* score here (it's computed during the beat),
        // so we use the last known delta from record_beat(). The consecutive_stable
        // counter already tracks whether recent beats had small deltas.
        // A delta of 0.0 with consecutive_stable < 3 → Active (30s).
        // A delta of 0.0 with consecutive_stable >= 3 → Stable (120s).
        // convergence_level >= 2 overrides to Escalated (15s) or Critical (5s).
        let score_delta = self.tiered_state.last_score
            .and_then(|_last| {
                // Use the stored consecutive_stable to infer recent delta behavior.
                // If consecutive_stable > 0, last deltas were < 0.01.
                // If consecutive_stable == 0, last delta was >= 0.01.
                if self.tiered_state.consecutive_stable > 0 {
                    Some(0.005) // Small delta — reflects recent stability
                } else {
                    Some(0.05) // Non-trivial delta — reflects recent activity
                }
            })
            .unwrap_or(0.0);
        let interval = interval_for_state(
            score_delta,
            self.tiered_state.consecutive_stable,
            convergence_level,
        );

        // Check if enough time has elapsed
        match self.last_beat {
            None => true,
            Some(last) => {
                let elapsed = Utc::now() - last;
                elapsed
                    .to_std()
                    .map(|d| d >= interval)
                    .unwrap_or(true)
            }
        }
    }

    /// Record that a heartbeat fired, updating tiered state.
    pub fn record_beat(&mut self, cost: f64) {
        self.last_beat = Some(Utc::now());
        self.total_cost += cost;
    }

    /// Record a beat with convergence score for tiered tracking.
    pub fn record_beat_with_score(&mut self, cost: f64, convergence_score: f64) {
        self.last_beat = Some(Utc::now());
        self.total_cost += cost;
        self.tiered_state.record_beat(convergence_score);
    }

    /// Get the synthetic heartbeat message.
    pub fn message(&self) -> &'static str {
        HEARTBEAT_MESSAGE
    }

    /// Read the current convergence score from the shared state file.
    /// Returns 0.0 if the file doesn't exist or can't be parsed.
    fn read_convergence_score(agent_id: Uuid) -> f64 {
        let home = match std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            Ok(h) => h,
            Err(_) => return 0.0,
        };
        let path = format!("{}/.ghost/data/convergence_state/{}.json", home, agent_id);
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => return 0.0,
        };
        let v: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return 0.0,
        };
        v["score"].as_f64().unwrap_or(0.0)
    }

    /// Fire a heartbeat turn by dispatching to the agent runner.
    ///
    /// This is the bridge between the heartbeat engine and the agentic loop.
    /// It calls `pre_loop` with the heartbeat session key and synthetic message,
    /// then runs a full agent turn.
    pub async fn fire(
        &mut self,
        runner: &mut ghost_agent_loop::runner::AgentRunner,
        fallback_chain: &mut ghost_agent_loop::runner::LLMFallbackChain,
    ) -> Result<ghost_agent_loop::runner::RunResult, HeartbeatError> {
        let ctx = runner
            .pre_loop(
                self.agent_id,
                self.session_key,
                "heartbeat",
                HEARTBEAT_MESSAGE,
            )
            .await
            .map_err(|e| HeartbeatError::RunFailed(e.to_string()))?;

        let mut ctx = ctx;
        let result = runner
            .run_turn(&mut ctx, fallback_chain, HEARTBEAT_MESSAGE)
            .await;

        match &result {
            Ok(run_result) => {
                // Read convergence score from shared state for tiered tracking.
                // Falls back to 0.0 if unavailable (first boot / monitor down).
                let convergence_score = Self::read_convergence_score(self.agent_id);
                self.record_beat_with_score(run_result.total_cost, convergence_score);
                tracing::info!(
                    agent_id = %self.agent_id,
                    cost = run_result.total_cost,
                    tool_calls = run_result.tool_calls_made,
                    convergence_score,
                    "heartbeat turn completed"
                );
            }
            Err(e) => {
                self.record_beat(0.0);
                tracing::warn!(
                    agent_id = %self.agent_id,
                    error = %e,
                    "heartbeat turn failed"
                );
            }
        }

        result.map_err(|e| HeartbeatError::RunFailed(e.to_string()))
    }
}
