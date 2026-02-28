//! Heartbeat engine: periodic synthetic messages to keep agents active (Req 34).
//!
//! - Configurable interval (default 30min)
//! - Dedicated session key: hash(agent_id, "heartbeat", agent_id)
//! - Synthetic message: "[HEARTBEAT] Check HEARTBEAT.md and act if needed."
//! - Convergence-aware frequency: L0-1→30m, L2→60m, L3→120m, L4→disabled
//! - Checks PLATFORM_KILLED and per-agent pause/quarantine before every execution

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// The canonical heartbeat message.
pub const HEARTBEAT_MESSAGE: &str = "[HEARTBEAT] Check HEARTBEAT.md and act if needed.";

/// Heartbeat configuration.
#[derive(Debug, Clone)]
pub struct HeartbeatConfig {
    /// Base interval in minutes (default 30).
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

/// Convergence-aware interval mapping.
/// L0-1 → base, L2 → 2x, L3 → 4x, L4 → disabled.
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
pub struct HeartbeatEngine {
    pub config: HeartbeatConfig,
    pub agent_id: Uuid,
    pub session_key: Uuid,
    pub platform_killed: Arc<AtomicBool>,
    pub last_beat: Option<DateTime<Utc>>,
    pub total_cost: f64,
    agent_paused: Arc<AtomicBool>,
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
        }
    }

    /// Check if the heartbeat should fire now.
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

        // Get convergence-aware interval
        let interval = match interval_for_level(self.config.base_interval_minutes, convergence_level)
        {
            Some(i) => i,
            None => return false, // L4 → disabled
        };

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

    /// Record that a heartbeat fired.
    pub fn record_beat(&mut self, cost: f64) {
        self.last_beat = Some(Utc::now());
        self.total_cost += cost;
    }

    /// Get the synthetic heartbeat message.
    pub fn message(&self) -> &'static str {
        HEARTBEAT_MESSAGE
    }
}
