//! Agent registry: lookup by name, by channel binding, lifecycle tracking.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::safety::kill_switch::{KillLevel, KillSwitchState};

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
    pub isolation: crate::config::IsolationMode,
    pub full_access: bool,
    pub capabilities: Vec<String>,
    pub skills: Option<Vec<String>>,
    pub baseline_capabilities: Vec<String>,
    pub baseline_skills: Option<Vec<String>>,
    pub access_pullback_active: bool,
    pub spending_cap: f64,
    /// Optional template name for agent initialization (Finding #16).
    pub template: Option<String>,
}

impl RegisteredAgent {
    pub fn apply_access_pullback(&mut self) -> bool {
        if self.access_pullback_active {
            return false;
        }

        self.capabilities.clear();
        self.skills = Some(Vec::new());
        self.access_pullback_active = true;
        true
    }

    pub fn restore_access_profile(&mut self) -> bool {
        if !self.access_pullback_active {
            return false;
        }

        self.capabilities = self.baseline_capabilities.clone();
        self.skills = self.baseline_skills.clone();
        self.access_pullback_active = false;
        true
    }
}

/// Agent registry for the gateway.
pub struct AgentRegistry {
    agents_by_id: BTreeMap<Uuid, RegisteredAgent>,
    name_to_id: BTreeMap<String, Uuid>,
    channel_to_id: BTreeMap<String, Uuid>,
    sandbox_by_id: BTreeMap<Uuid, crate::config::AgentSandboxConfig>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents_by_id: BTreeMap::new(),
            name_to_id: BTreeMap::new(),
            channel_to_id: BTreeMap::new(),
            sandbox_by_id: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, agent: RegisteredAgent) {
        let sandbox = if agent.full_access {
            crate::config::AgentSandboxConfig::off()
        } else {
            crate::config::AgentSandboxConfig::default()
        };
        self.register_with_sandbox(agent, sandbox);
    }

    pub fn register_with_sandbox(
        &mut self,
        agent: RegisteredAgent,
        sandbox: crate::config::AgentSandboxConfig,
    ) {
        let id = agent.id;
        self.name_to_id.insert(agent.name.clone(), id);
        for binding in &agent.channel_bindings {
            self.channel_to_id.insert(binding.clone(), id);
        }
        self.agents_by_id.insert(id, agent);
        self.sandbox_by_id.insert(id, sandbox);
    }

    pub fn bind_channel(&mut self, channel: String, agent_id: Uuid) -> Result<(), String> {
        if !self.agents_by_id.contains_key(&agent_id) {
            return Err(format!("Agent {agent_id} not found"));
        }

        if let Some(previous_agent_id) = self.channel_to_id.insert(channel.clone(), agent_id) {
            if previous_agent_id != agent_id {
                if let Some(previous_agent) = self.agents_by_id.get_mut(&previous_agent_id) {
                    previous_agent
                        .channel_bindings
                        .retain(|binding| binding != &channel);
                }
            }
        }

        let agent = self
            .agents_by_id
            .get_mut(&agent_id)
            .ok_or_else(|| format!("Agent {agent_id} not found"))?;
        if !agent
            .channel_bindings
            .iter()
            .any(|binding| binding == &channel)
        {
            agent.channel_bindings.push(channel);
        }
        Ok(())
    }

    pub fn unbind_channel(&mut self, channel: &str) -> Option<Uuid> {
        let agent_id = self.channel_to_id.remove(channel)?;
        if let Some(agent) = self.agents_by_id.get_mut(&agent_id) {
            agent.channel_bindings.retain(|binding| binding != channel);
        }
        Some(agent_id)
    }

    pub fn clear_channel_bindings(&mut self) {
        self.channel_to_id.clear();
        for agent in self.agents_by_id.values_mut() {
            agent.channel_bindings.clear();
        }
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

    pub fn sandbox_for(&self, id: Uuid) -> crate::config::AgentSandboxConfig {
        self.sandbox_by_id.get(&id).cloned().unwrap_or_default()
    }

    pub fn update_sandbox(
        &mut self,
        id: Uuid,
        sandbox: crate::config::AgentSandboxConfig,
    ) -> Result<(), String> {
        if !self.agents_by_id.contains_key(&id) {
            return Err(format!("Agent {id} not found"));
        }
        self.sandbox_by_id.insert(id, sandbox);
        Ok(())
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
            self.sandbox_by_id.remove(&id);
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

    pub fn sync_access_pullbacks(&mut self, kill_state: &KillSwitchState) -> Vec<Uuid> {
        let mut changed = Vec::new();
        let pull_back_all = kill_state.platform_level == KillLevel::KillAll;

        for agent in self.agents_by_id.values_mut() {
            let should_pull_back = pull_back_all
                || kill_state
                    .per_agent
                    .get(&agent.id)
                    .is_some_and(|state| state.level == KillLevel::Quarantine);

            let updated = if should_pull_back {
                agent.apply_access_pullback()
            } else {
                agent.restore_access_profile()
            };

            if updated {
                changed.push(agent.id);
            }
        }

        changed
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_access_pullbacks_strips_and_restores_effective_access() {
        let agent_id = Uuid::now_v7();
        let mut registry = AgentRegistry::new();
        registry.register(RegisteredAgent {
            id: agent_id,
            name: "pullback-target".into(),
            state: AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            isolation: crate::config::IsolationMode::InProcess,
            full_access: false,
            capabilities: vec!["shell_execute".into(), "web_search".into()],
            skills: None,
            baseline_capabilities: vec!["shell_execute".into(), "web_search".into()],
            baseline_skills: None,
            access_pullback_active: false,
            spending_cap: 5.0,
            template: None,
        });

        let mut kill_state = KillSwitchState::default();
        kill_state.per_agent.insert(
            agent_id,
            crate::safety::kill_switch::AgentKillState {
                agent_id,
                level: KillLevel::Quarantine,
                activated_at: None,
                trigger: Some("test".into()),
            },
        );

        let changed = registry.sync_access_pullbacks(&kill_state);
        assert_eq!(changed, vec![agent_id]);

        let pulled_back = registry.lookup_by_id(agent_id).unwrap();
        assert!(pulled_back.capabilities.is_empty());
        assert_eq!(pulled_back.skills, Some(Vec::new()));
        assert!(pulled_back.access_pullback_active);

        let changed = registry.sync_access_pullbacks(&KillSwitchState::default());
        assert_eq!(changed, vec![agent_id]);

        let restored = registry.lookup_by_id(agent_id).unwrap();
        assert_eq!(
            restored.capabilities,
            vec!["shell_execute".to_string(), "web_search".to_string()]
        );
        assert_eq!(restored.skills, None);
        assert!(!restored.access_pullback_active);
    }

    #[test]
    fn bind_and_unbind_channel_keep_reverse_index_in_sync() {
        let agent_id = Uuid::now_v7();
        let mut registry = AgentRegistry::new();
        registry.register(RegisteredAgent {
            id: agent_id,
            name: "channel-owner".into(),
            state: AgentLifecycleState::Ready,
            channel_bindings: vec!["cli:channel-owner".into()],
            isolation: crate::config::IsolationMode::InProcess,
            full_access: false,
            capabilities: Vec::new(),
            skills: None,
            baseline_capabilities: Vec::new(),
            baseline_skills: None,
            access_pullback_active: false,
            spending_cap: 5.0,
            template: None,
        });

        registry
            .bind_channel("slack:workspace:ops".into(), agent_id)
            .unwrap();
        let found = registry.lookup_by_channel("slack:workspace:ops").unwrap();
        assert_eq!(found.id, agent_id);
        assert_eq!(
            registry.lookup_by_id(agent_id).unwrap().channel_bindings,
            vec![
                "cli:channel-owner".to_string(),
                "slack:workspace:ops".to_string()
            ]
        );

        registry.unbind_channel("slack:workspace:ops");
        assert!(registry.lookup_by_channel("slack:workspace:ops").is_none());
        assert_eq!(
            registry.lookup_by_id(agent_id).unwrap().channel_bindings,
            vec!["cli:channel-owner".to_string()]
        );
    }
}
