//! State publisher — atomic write to convergence state files (A34 Gap 3, AC7).
//!
//! Publishes per-agent convergence state as JSON via atomic file writes
//! (write to temp + rename). Consumed by ghost-policy and read-only-pipeline.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shared convergence state for a single agent.
///
/// Written atomically to `~/.ghost/data/convergence_state/{agent_id}.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceSharedState {
    pub agent_id: Uuid,
    pub score: f64,
    pub level: u8,
    pub signal_scores: [f64; 8],
    pub consecutive_normal: u32,
    pub cooldown_until: Option<DateTime<Utc>>,
    pub ack_required: bool,
    pub updated_at: DateTime<Utc>,
}

/// Publishes convergence state via atomic file writes.
pub struct StatePublisher {
    state_dir: PathBuf,
}

impl StatePublisher {
    pub fn new(state_dir: PathBuf) -> Self {
        Self { state_dir }
    }

    /// Publish state for an agent (atomic write: temp file + rename).
    pub fn publish(&self, state: &ConvergenceSharedState) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        let agent_str = state.agent_id.to_string();
        let path = self.state_dir.join(format!("{agent_str}.json"));
        let tmp_path = self.state_dir.join(format!("{agent_str}.json.tmp"));
        let json = serde_json::to_string_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &path)?;
        Ok(())
    }
}
