//! Message router: route inbound messages to (agent_id, session_id) (Req 26 AC2).

use std::collections::BTreeMap;

use uuid::Uuid;

/// Routing target.
#[derive(Debug, Clone)]
pub struct RouteTarget {
    pub agent_id: Uuid,
    pub session_id: Option<Uuid>,
}

/// Message router based on channel bindings.
pub struct MessageRouter {
    channel_bindings: BTreeMap<String, Uuid>,
}

impl MessageRouter {
    pub fn new() -> Self {
        Self {
            channel_bindings: BTreeMap::new(),
        }
    }

    /// Register a channel binding to an agent.
    pub fn bind_channel(&mut self, channel_key: String, agent_id: Uuid) {
        self.channel_bindings.insert(channel_key, agent_id);
    }

    /// Route a message based on channel key.
    pub fn route(&self, channel_key: &str) -> Option<RouteTarget> {
        self.channel_bindings.get(channel_key).map(|agent_id| RouteTarget {
            agent_id: *agent_id,
            session_id: None, // Session resolved by SessionManager
        })
    }
}

impl Default for MessageRouter {
    fn default() -> Self {
        Self::new()
    }
}
