//! Canonical runtime safety wiring for live agent execution.

use std::sync::Arc;

use ghost_agent_loop::runner::AgentRunner;
use ghost_llm::provider::ChatMessage;
use thiserror::Error;
use uuid::Uuid;

use crate::agents::registry::{durable_agent_id, AgentRegistry, RegisteredAgent};
use crate::api::apply_tool_configs;
use crate::safety::kill_gate_bridge::KillGateBridge;
use crate::safety::kill_switch::KillSwitch;
use crate::state::AppState;

pub const API_SYNTHETIC_AGENT_NAME: &str = "__ghost_runtime_api__";
pub const STUDIO_SYNTHETIC_AGENT_NAME: &str = "__ghost_runtime_studio__";
pub const CLI_SYNTHETIC_AGENT_NAME: &str = "__ghost_runtime_cli__";

const RUNTIME_ID_NAMESPACE: Uuid = Uuid::from_u128(0x6ba7b814_9dad_11d1_80b4_00c04fd430c8);
const DEFAULT_SYNTHETIC_SPENDING_CAP: f64 = 10.0;

#[derive(Debug, Clone)]
pub struct ResolvedRuntimeAgent {
    pub id: Uuid,
    pub name: String,
    pub capabilities: Vec<String>,
    pub spending_cap: f64,
}

impl ResolvedRuntimeAgent {
    fn from_registered(agent: &RegisteredAgent) -> Self {
        Self {
            id: agent.id,
            name: agent.name.clone(),
            capabilities: agent.capabilities.clone(),
            spending_cap: agent.spending_cap,
        }
    }

    pub fn synthetic(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: durable_agent_id(&name),
            name,
            capabilities: Vec::new(),
            spending_cap: DEFAULT_SYNTHETIC_SPENDING_CAP,
        }
    }

    pub fn synthetic_with_id(name: impl Into<String>, id: Uuid) -> Self {
        Self {
            id,
            name: name.into(),
            capabilities: Vec::new(),
            spending_cap: DEFAULT_SYNTHETIC_SPENDING_CAP,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeSafetyContext {
    pub agent: ResolvedRuntimeAgent,
    pub session_id: Uuid,
    pub run_id: Uuid,
    pub message_id: Option<Uuid>,
    pub kill_switch: Arc<KillSwitch>,
    pub kill_gate: Option<Arc<std::sync::RwLock<KillGateBridge>>>,
    pub convergence_profile: String,
    pub capability_scope: Vec<String>,
}

impl RuntimeSafetyContext {
    pub fn from_state(
        state: &AppState,
        agent: ResolvedRuntimeAgent,
        session_id: Uuid,
        message_id: Option<Uuid>,
    ) -> Self {
        Self {
            capability_scope: agent.capabilities.clone(),
            agent,
            session_id,
            run_id: Uuid::now_v7(),
            message_id,
            kill_switch: Arc::clone(&state.kill_switch),
            kill_gate: state.kill_gate.clone(),
            convergence_profile: state.convergence_profile.clone(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnerBuildOptions {
    pub system_prompt: Option<String>,
    pub conversation_history: Vec<ChatMessage>,
    pub skill_allowlist: Option<Vec<String>>,
}

#[derive(Debug, Error)]
pub enum RuntimeSafetyError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("agent registry lock poisoned")]
    AgentRegistryPoisoned,
    #[error("db pool error: {0}")]
    DbPool(String),
}

pub struct RuntimeSafetyBuilder<'a> {
    state: &'a AppState,
}

impl<'a> RuntimeSafetyBuilder<'a> {
    pub fn new(state: &'a AppState) -> Self {
        Self { state }
    }

    pub fn resolve_agent(
        &self,
        requested: Option<&str>,
        synthetic_name: &str,
    ) -> Result<ResolvedRuntimeAgent, RuntimeSafetyError> {
        let registry = self
            .state
            .agents
            .read()
            .map_err(|_| RuntimeSafetyError::AgentRegistryPoisoned)?;
        resolve_runtime_agent(&registry, requested, synthetic_name)
    }

    pub fn resolve_stored_agent(
        &self,
        stored: &str,
        synthetic_name: &str,
    ) -> Result<ResolvedRuntimeAgent, RuntimeSafetyError> {
        let registry = self
            .state
            .agents
            .read()
            .map_err(|_| RuntimeSafetyError::AgentRegistryPoisoned)?;
        Ok(resolve_stored_runtime_agent(&registry, stored, synthetic_name))
    }

    pub fn build_live_runner(
        &self,
        ctx: &RuntimeSafetyContext,
        options: RunnerBuildOptions,
    ) -> Result<AgentRunner, RuntimeSafetyError> {
        let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
        ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

        runner.db = Some(
            self.state
                .db
                .legacy_connection()
                .map_err(|e| RuntimeSafetyError::DbPool(e.to_string()))?,
        );

        if let Ok(cwd) = std::env::current_dir() {
            runner.tool_executor.set_workspace_root(cwd);
        }
        apply_tool_configs(&mut runner.tool_executor, &self.state.tools_config);

        runner.soul_identity = options
            .system_prompt
            .filter(|prompt| !prompt.is_empty())
            .unwrap_or_else(default_soul_identity);
        runner.environment = ghost_agent_loop::context::environment::build_environment_context(
            std::env::current_dir().ok().as_deref(),
        );
        runner.conversation_history = options.conversation_history;
        runner.spending_cap = ctx.agent.spending_cap;

        let legacy_db = self
            .state
            .db
            .legacy_connection()
            .map_err(|e| RuntimeSafetyError::DbPool(e.to_string()))?;
        let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
            Arc::clone(&self.state.safety_skills),
            legacy_db,
            ctx.convergence_profile.clone(),
        );
        ghost_agent_loop::tools::skill_bridge::register_skills(
            &bridge,
            &mut runner.tool_registry,
            options.skill_allowlist.as_deref(),
        );
        runner.tool_executor.set_skill_bridge(bridge);

        let cost_tracker = Arc::clone(&self.state.cost_tracker);
        runner.cost_recorder = Some(Arc::new(move |agent_id, session_id, cost, is_compaction| {
            cost_tracker.record(agent_id, session_id, cost, is_compaction);
        }));

        Ok(runner)
    }
}

pub fn resolve_runtime_agent(
    registry: &AgentRegistry,
    requested: Option<&str>,
    synthetic_name: &str,
) -> Result<ResolvedRuntimeAgent, RuntimeSafetyError> {
    if let Some(requested) = requested.map(str::trim).filter(|value| !value.is_empty()) {
        return resolve_explicit_runtime_agent(registry, requested);
    }

    Ok(registry
        .default_agent()
        .map(ResolvedRuntimeAgent::from_registered)
        .unwrap_or_else(|| ResolvedRuntimeAgent::synthetic(synthetic_name)))
}

pub fn resolve_stored_runtime_agent(
    registry: &AgentRegistry,
    stored: &str,
    synthetic_name: &str,
) -> ResolvedRuntimeAgent {
    if let Ok(agent) = resolve_explicit_runtime_agent(registry, stored) {
        return agent;
    }

    let stored_id = parse_or_stable_uuid(stored, synthetic_name);
    ResolvedRuntimeAgent::synthetic_with_id(synthetic_name, stored_id)
}

pub fn parse_or_stable_uuid(value: &str, scope: &str) -> Uuid {
    Uuid::parse_str(value)
        .unwrap_or_else(|_| Uuid::new_v5(&RUNTIME_ID_NAMESPACE, format!("{scope}:{value}").as_bytes()))
}

pub fn default_soul_identity() -> String {
    let soul_path = crate::bootstrap::ghost_home().join("config").join("SOUL.md");
    std::fs::read_to_string(soul_path)
        .ok()
        .filter(|content| !content.is_empty())
        .unwrap_or_default()
}

fn resolve_explicit_runtime_agent(
    registry: &AgentRegistry,
    requested: &str,
) -> Result<ResolvedRuntimeAgent, RuntimeSafetyError> {
    if let Ok(agent_id) = Uuid::parse_str(requested) {
        return registry
            .lookup_by_id(agent_id)
            .map(ResolvedRuntimeAgent::from_registered)
            .ok_or_else(|| RuntimeSafetyError::AgentNotFound(requested.to_string()));
    }

    registry
        .lookup_by_name(requested)
        .map(ResolvedRuntimeAgent::from_registered)
        .ok_or_else(|| RuntimeSafetyError::AgentNotFound(requested.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::registry::{AgentLifecycleState, RegisteredAgent};

    fn registered(name: &str) -> RegisteredAgent {
        RegisteredAgent {
            id: durable_agent_id(name),
            name: name.to_string(),
            state: AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            capabilities: vec!["shell".into()],
            spending_cap: 7.5,
            template: None,
        }
    }

    #[test]
    fn resolve_runtime_agent_prefers_registered_default_agent() {
        let mut registry = AgentRegistry::new();
        registry.register(registered("zeta"));
        registry.register(registered("alpha"));

        let resolved = resolve_runtime_agent(&registry, None, API_SYNTHETIC_AGENT_NAME).unwrap();

        assert_eq!(resolved.name, "alpha");
        assert_eq!(resolved.id, durable_agent_id("alpha"));
        assert_eq!(resolved.spending_cap, 7.5);
    }

    #[test]
    fn resolve_runtime_agent_accepts_uuid_and_name() {
        let mut registry = AgentRegistry::new();
        let alpha = registered("alpha");
        let alpha_id = alpha.id;
        registry.register(alpha);

        let by_name = resolve_runtime_agent(&registry, Some("alpha"), API_SYNTHETIC_AGENT_NAME).unwrap();
        let by_id = resolve_runtime_agent(
            &registry,
            Some(alpha_id.to_string().as_str()),
            API_SYNTHETIC_AGENT_NAME,
        )
        .unwrap();

        assert_eq!(by_name.id, alpha_id);
        assert_eq!(by_id.id, alpha_id);
    }

    #[test]
    fn resolve_runtime_agent_uses_stable_synthetic_fallback_when_registry_empty() {
        let registry = AgentRegistry::new();

        let a = resolve_runtime_agent(&registry, None, API_SYNTHETIC_AGENT_NAME).unwrap();
        let b = resolve_runtime_agent(&registry, None, API_SYNTHETIC_AGENT_NAME).unwrap();

        assert_eq!(a.id, b.id);
        assert_eq!(a.name, API_SYNTHETIC_AGENT_NAME);
    }

    #[test]
    fn parse_or_stable_uuid_preserves_real_uuid_and_stabilizes_strings() {
        let uuid = Uuid::now_v7();
        assert_eq!(parse_or_stable_uuid(&uuid.to_string(), "studio-session"), uuid);

        let a = parse_or_stable_uuid("legacy-session", "studio-session");
        let b = parse_or_stable_uuid("legacy-session", "studio-session");
        assert_eq!(a, b);
    }
}
