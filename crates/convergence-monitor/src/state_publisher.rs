//! State publisher — atomic write to convergence state files (A34 Gap 3, AC7).
//!
//! Publishes per-agent convergence state as JSON via atomic file writes
//! (write to temp + rename). Consumed by ghost-policy and read-only-pipeline.

use std::path::{Path, PathBuf};

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
        let publish_result = (|| {
            std::fs::write(&tmp_path, json)?;
            std::fs::rename(&tmp_path, &path)?;
            Ok(())
        })();
        if let Err(error) = publish_result {
            cleanup_state_temp_path(&tmp_path);
            return Err(error);
        }
        Ok(())
    }
}

fn cleanup_state_temp_path(path: &Path) {
    let _ = std::fs::remove_file(path);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publish_cleans_up_temp_file_on_rename_failure() {
        let dir = tempfile::tempdir().unwrap();
        let state = ConvergenceSharedState {
            agent_id: Uuid::new_v4(),
            score: 0.7,
            level: 2,
            signal_scores: [0.2, 0.3, 0.5, 0.6, 0.1, 0.0, 0.4, 0.8],
            consecutive_normal: 1,
            cooldown_until: None,
            ack_required: true,
            updated_at: Utc::now(),
        };
        let publisher = StatePublisher::new(dir.path().to_path_buf());
        let target_path = dir.path().join(format!("{}.json", state.agent_id));
        let temp_path = dir.path().join(format!("{}.json.tmp", state.agent_id));

        std::fs::create_dir_all(&target_path).unwrap();

        let error = publisher.publish(&state).unwrap_err();

        assert_eq!(error.kind(), std::io::ErrorKind::IsADirectory);
        assert!(!temp_path.exists(), "temp file was not cleaned up");
        assert!(target_path.is_dir(), "forced rename failure target changed");
    }
}
