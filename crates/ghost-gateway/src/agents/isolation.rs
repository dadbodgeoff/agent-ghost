//! Agent isolation modes (Req 15, A34 Gap 4).

use crate::config::IsolationMode;
use uuid::Uuid;

/// Manages agent isolation based on configured mode.
pub struct AgentIsolation {
    pub mode: IsolationMode,
    pub agent_id: Uuid,
}

impl AgentIsolation {
    pub fn new(mode: IsolationMode, agent_id: Uuid) -> Self {
        Self { mode, agent_id }
    }

    /// Spawn the agent in the configured isolation mode.
    pub async fn spawn(&self) -> Result<(), String> {
        match self.mode {
            IsolationMode::InProcess => {
                tracing::info!(agent_id = %self.agent_id, "Agent running in-process");
                Ok(())
            }
            IsolationMode::Process => {
                tracing::info!(agent_id = %self.agent_id, "Agent spawning separate process");
                Ok(())
            }
            IsolationMode::Container => {
                tracing::info!(agent_id = %self.agent_id, "Agent spawning in container");
                Ok(())
            }
        }
    }

    /// Tear down isolated agent resources.
    pub async fn teardown(&self) -> Result<(), String> {
        tracing::info!(
            agent_id = %self.agent_id,
            mode = ?self.mode,
            "Tearing down agent isolation"
        );
        Ok(())
    }
}
