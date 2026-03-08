//! Agent registry: lookup by name, by channel binding, lifecycle tracking.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DURABLE_AGENT_NAMESPACE: Uuid = Uuid::from_u128(0x6ba7b812_9dad_11d1_80b4_00c04fd430c8);

pub fn durable_agent_id(name: &str) -> Uuid {
    Uuid::new_v5(&DURABLE_AGENT_NAMESPACE, name.as_bytes())
}

/// Agent lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentLifecycleState {
    Starting,
    Ready,
    Stopping,
    Stopped,
}

impl AgentLifecycleState {
    pub fn can_transition_to(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Self::Starting, Self::Ready)
                | (Self::Ready, Self::Stopping)
                | (Self::Stopping, Self::Stopped)
        )
    }
}

/// Registered agent entry.
#[derive(Debug, Clone)]
pub struct RegisteredAgent {
    pub id: Uuid,
    pub name: String,
    pub state: AgentLifecycleState,
    pub channel_bindings: Vec<String>,
    pub capabilities: Vec<String>,
    pub skills: Option<Vec<String>>,
    pub spending_cap: f64,
    /// Optional template name for agent initialization (Finding #16).
    pub template: Option<String>,
}

/// Agent registry for the gateway.
pub struct AgentRegistry {
    agents_by_id: BTreeMap<Uuid, RegisteredAgent>,
    name_to_id: BTreeMap<String, Uuid>,
    channel_to_id: BTreeMap<String, Uuid>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents_by_id: BTreeMap::new(),
            name_to_id: BTreeMap::new(),
            channel_to_id: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, agent: RegisteredAgent) {
        let id = agent.id;
        self.name_to_id.insert(agent.name.clone(), id);
        for binding in &agent.channel_bindings {
            self.channel_to_id.insert(binding.clone(), id);
        }
        self.agents_by_id.insert(id, agent);
    }

    pub fn lookup_by_name(&self, name: &str) -> Option<&RegisteredAgent> {
        self.name_to_id
            .get(name)
            .and_then(|id| self.agents_by_id.get(id))
    }

    pub fn lookup_by_channel(&self, channel: &str) -> Option<&RegisteredAgent> {
        self.channel_to_id
            .get(channel)
            .and_then(|id| self.agents_by_id.get(id))
    }

    pub fn lookup_by_id(&self, id: Uuid) -> Option<&RegisteredAgent> {
        self.agents_by_id.get(&id)
    }

    pub fn lookup_by_id_mut(&mut self, id: Uuid) -> Option<&mut RegisteredAgent> {
        self.agents_by_id.get_mut(&id)
    }

    pub fn default_agent(&self) -> Option<&RegisteredAgent> {
        self.name_to_id
            .values()
            .next()
            .and_then(|id| self.agents_by_id.get(id))
    }

    pub fn all_agents(&self) -> Vec<&RegisteredAgent> {
        self.agents_by_id.values().collect()
    }

    /// Remove an agent from the registry.
    /// Returns the removed agent, or None if not found.
    pub fn unregister(&mut self, id: Uuid) -> Option<RegisteredAgent> {
        if let Some(agent) = self.agents_by_id.remove(&id) {
            self.name_to_id.remove(&agent.name);
            for binding in &agent.channel_bindings {
                self.channel_to_id.remove(binding);
            }
            Some(agent)
        } else {
            None
        }
    }

    pub fn transition_state(&mut self, id: Uuid, to: AgentLifecycleState) -> Result<(), String> {
        let agent = self
            .agents_by_id
            .get_mut(&id)
            .ok_or_else(|| format!("Agent {id} not found"))?;
        if !agent.state.can_transition_to(to) {
            return Err(format!("Invalid transition: {:?} -> {:?}", agent.state, to));
        }
        agent.state = to;
        Ok(())
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
