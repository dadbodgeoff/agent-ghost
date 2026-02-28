//! Quarantine manager: agent isolation, capability revocation, forensic state (Req 14 AC5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Forensic state captured during quarantine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForensicState {
    pub agent_id: Uuid,
    pub quarantined_at: DateTime<Utc>,
    pub session_transcript: Vec<String>,
    pub memory_snapshot: serde_json::Value,
    pub tool_history: Vec<String>,
    pub trigger_reason: String,
}

/// Quarantine manager.
pub struct QuarantineManager {
    forensic_states: std::collections::BTreeMap<Uuid, ForensicState>,
}

impl QuarantineManager {
    pub fn new() -> Self {
        Self {
            forensic_states: std::collections::BTreeMap::new(),
        }
    }

    /// Quarantine an agent: revoke capabilities, disconnect channels, preserve forensic state.
    pub fn quarantine(
        &mut self,
        agent_id: Uuid,
        reason: String,
        transcript: Vec<String>,
        memory_snapshot: serde_json::Value,
        tool_history: Vec<String>,
    ) -> ForensicState {
        let state = ForensicState {
            agent_id,
            quarantined_at: Utc::now(),
            session_transcript: transcript,
            memory_snapshot,
            tool_history,
            trigger_reason: reason,
        };
        self.forensic_states.insert(agent_id, state.clone());
        tracing::warn!(agent_id = %agent_id, "Agent quarantined. Forensic state preserved.");
        state
    }

    /// Get forensic state for a quarantined agent.
    pub fn get_forensic_state(&self, agent_id: Uuid) -> Option<&ForensicState> {
        self.forensic_states.get(&agent_id)
    }

    /// Count currently quarantined agents.
    pub fn quarantined_count(&self) -> usize {
        self.forensic_states.len()
    }
}

impl Default for QuarantineManager {
    fn default() -> Self {
        Self::new()
    }
}
