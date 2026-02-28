//! ITPAdapter trait (Req 4 AC5).

use crate::events::*;

/// Trait for ITP event consumers. Object-safe (can be `Box<dyn ITPAdapter>`).
pub trait ITPAdapter: Send + Sync {
    fn on_session_start(&self, event: &SessionStartEvent);
    fn on_message(&self, event: &InteractionMessageEvent);
    fn on_session_end(&self, event: &SessionEndEvent);
    fn on_agent_state(&self, event: &AgentStateSnapshotEvent);
}
