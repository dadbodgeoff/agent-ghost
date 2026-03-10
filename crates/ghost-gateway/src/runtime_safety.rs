//! Canonical runtime safety wiring for live agent execution.

use std::sync::Arc;

use cortex_core::safety::trigger::TriggerEvent;
use ghost_agent_loop::runner::{AgentRunner, AuthoritativeKillState, DegradedConvergenceMode};
use ghost_llm::provider::ChatMessage;
use ghost_policy::engine::{CorpPolicy, PolicyEngine};
use thiserror::Error;
use uuid::Uuid;

use crate::agents::registry::{durable_agent_id, AgentRegistry, RegisteredAgent};
use crate::api::apply_tool_configs;
use crate::config::ToolsConfig;
use crate::cost::tracker::CostTracker;
use crate::safety::kill_gate_bridge::KillGateBridge;
use crate::safety::kill_switch::{KillCheckResult, KillSwitch};
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
    pub full_access: bool,
    pub capabilities: Vec<String>,
    pub skill_allowlist: Option<Vec<String>>,
    pub spending_cap: f64,
}

impl ResolvedRuntimeAgent {
    fn from_registered(agent: &RegisteredAgent) -> Self {
        Self {
            id: agent.id,
            name: agent.name.clone(),
            full_access: agent.full_access,
            capabilities: agent.capabilities.clone(),
            skill_allowlist: agent.skills.clone(),
            spending_cap: agent.spending_cap,
        }
    }

    pub fn synthetic(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            id: durable_agent_id(&name),
            name,
            full_access: false,
            capabilities: Vec::new(),
            skill_allowlist: None,
            spending_cap: DEFAULT_SYNTHETIC_SPENDING_CAP,
        }
    }

    pub fn synthetic_with_id(name: impl Into<String>, id: Uuid) -> Self {
        Self {
            id,
            name: name.into(),
            full_access: false,
            capabilities: Vec::new(),
            skill_allowlist: None,
            spending_cap: DEFAULT_SYNTHETIC_SPENDING_CAP,
        }
    }
}

#[derive(Clone)]
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

    pub fn ensure_execution_permitted(&self) -> Result<(), ghost_agent_loop::runner::RunError> {
        match self.kill_switch.check(self.agent.id) {
            KillCheckResult::Ok => Ok(()),
            KillCheckResult::AgentPaused(_) => Err(ghost_agent_loop::runner::RunError::AgentPaused),
            KillCheckResult::AgentQuarantined(_) => {
                Err(ghost_agent_loop::runner::RunError::AgentQuarantined)
            }
            KillCheckResult::PlatformKilled => {
                Err(ghost_agent_loop::runner::RunError::PlatformKilled)
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct RunnerBuildOptions {
    pub system_prompt: Option<String>,
    pub conversation_history: Vec<ChatMessage>,
    pub skill_allowlist: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct RuntimeRunnerDependencies {
    pub db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
    pub resolved_skills: crate::skill_catalog::ResolvedSkillSet,
    pub tools_config: ToolsConfig,
    pub trigger_sender: Option<tokio::sync::mpsc::Sender<TriggerEvent>>,
    pub convergence_profile: String,
    pub monitor_enabled: bool,
    pub monitor_block_on_degraded: bool,
    pub convergence_state_stale_after: std::time::Duration,
    pub cost_tracker: Option<Arc<CostTracker>>,
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
        Ok(resolve_stored_runtime_agent(
            &registry,
            stored,
            synthetic_name,
        ))
    }

    pub fn resolve_agent_by_id_or_synthetic(
        &self,
        agent_id: Uuid,
        synthetic_name: &str,
    ) -> Result<ResolvedRuntimeAgent, RuntimeSafetyError> {
        let registry = self
            .state
            .agents
            .read()
            .map_err(|_| RuntimeSafetyError::AgentRegistryPoisoned)?;
        Ok(registry
            .lookup_by_id(agent_id)
            .map(ResolvedRuntimeAgent::from_registered)
            .unwrap_or_else(|| ResolvedRuntimeAgent::synthetic_with_id(synthetic_name, agent_id)))
    }

    pub fn build_live_runner(
        &self,
        ctx: &RuntimeSafetyContext,
        options: RunnerBuildOptions,
    ) -> Result<AgentRunner, RuntimeSafetyError> {
        let resolved_skills = self
            .state
            .skill_catalog
            .resolve_for_runtime(&ctx.agent, options.skill_allowlist.as_deref())
            .map_err(|e| RuntimeSafetyError::DbPool(e.to_string()))?;
        let deps = RuntimeRunnerDependencies {
            db: Some(
                self.state
                    .db
                    .legacy_connection()
                    .map_err(|e| RuntimeSafetyError::DbPool(e.to_string()))?,
            ),
            resolved_skills,
            tools_config: self.state.tools_config.clone(),
            trigger_sender: Some(self.state.trigger_sender.clone()),
            convergence_profile: ctx.convergence_profile.clone(),
            monitor_enabled: self.state.monitor_enabled,
            monitor_block_on_degraded: self.state.monitor_block_on_degraded,
            convergence_state_stale_after: self.state.convergence_state_stale_after,
            cost_tracker: Some(Arc::clone(&self.state.cost_tracker)),
        };
        build_live_runner_with_dependencies(ctx, deps, options)
    }
}

pub fn build_live_runner_with_dependencies(
    ctx: &RuntimeSafetyContext,
    deps: RuntimeRunnerDependencies,
    options: RunnerBuildOptions,
) -> Result<AgentRunner, RuntimeSafetyError> {
    let mut runner = ghost_agent_loop::runner::AgentRunner::new(128_000);
    ghost_agent_loop::tools::executor::register_builtin_tools(&mut runner.tool_registry);

    runner.db = deps.db.clone();

    if let Ok(cwd) = std::env::current_dir() {
        if ctx.agent.full_access {
            runner.tool_executor.set_unrestricted_workspace_root(cwd);
        } else {
            runner.tool_executor.set_workspace_root(cwd);
        }
    } else if ctx.agent.full_access {
        runner
            .tool_executor
            .set_unrestricted_workspace_root(std::path::PathBuf::from("/"));
    }
    apply_tool_configs(&mut runner.tool_executor, &deps.tools_config);
    let mut policy_engine = deps
        .trigger_sender
        .clone()
        .map(|sender| PolicyEngine::new(CorpPolicy::new()).with_trigger_sender(sender))
        .unwrap_or_else(|| PolicyEngine::new(CorpPolicy::new()));
    for capability in &ctx.capability_scope {
        policy_engine.grant_capability(ctx.agent.id, capability.clone());
    }
    for capability in &deps.resolved_skills.granted_policy_capabilities {
        policy_engine.grant_capability(ctx.agent.id, capability.clone());
    }
    runner.tool_executor.set_policy_engine(policy_engine);

    runner.soul_identity = options
        .system_prompt
        .filter(|prompt| !prompt.is_empty())
        .unwrap_or_else(default_soul_identity);
    runner.environment = ghost_agent_loop::context::environment::build_environment_context(
        std::env::current_dir().ok().as_deref(),
    );
    runner.conversation_history = options.conversation_history;
    runner.spending_cap = ctx.agent.spending_cap;
    runner.convergence_monitor_enabled = deps.monitor_enabled;
    runner.degraded_convergence_mode = if deps.monitor_block_on_degraded {
        DegradedConvergenceMode::Block
    } else {
        DegradedConvergenceMode::Allow
    };
    runner.convergence_state_stale_after = deps.convergence_state_stale_after;
    runner.authoritative_kill_state = Some({
        let kill_switch = Arc::clone(&ctx.kill_switch);
        Arc::new(move |agent_id| match kill_switch.check(agent_id) {
            crate::safety::kill_switch::KillCheckResult::Ok => AuthoritativeKillState::Clear,
            crate::safety::kill_switch::KillCheckResult::AgentPaused(_) => {
                AuthoritativeKillState::Pause
            }
            crate::safety::kill_switch::KillCheckResult::AgentQuarantined(_) => {
                AuthoritativeKillState::Quarantine
            }
            crate::safety::kill_switch::KillCheckResult::PlatformKilled => {
                AuthoritativeKillState::KillAll
            }
        })
    });
    if let Some(ref kill_gate) = ctx.kill_gate {
        runner.distributed_gate_check = Some({
            let kill_gate = Arc::clone(kill_gate);
            Arc::new(move || match kill_gate.read() {
                Ok(bridge) => bridge.is_gate_closed(),
                Err(_) => true,
            })
        });
        if let Ok(bridge) = kill_gate.read() {
            runner.kill_gate = Some(Arc::clone(&bridge.gate));
        }
    }

    if let Some(bridge_db) = deps.db.clone() {
        let bridge = ghost_agent_loop::tools::skill_bridge::SkillBridge::new(
            Arc::clone(&deps.resolved_skills.skills),
            bridge_db,
            deps.convergence_profile.clone(),
        );
        ghost_agent_loop::tools::skill_bridge::register_skills(
            &bridge,
            &mut runner.tool_registry,
            None,
        );
        runner.tool_executor.set_skill_bridge(bridge);
    }

    if let Some(cost_tracker) = deps.cost_tracker {
        runner.cost_recorder = Some(Arc::new(
            move |agent_id, session_id, cost, is_compaction| {
                cost_tracker.record(agent_id, session_id, cost, is_compaction);
            },
        ));
    }

    Ok(runner)
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
    Uuid::parse_str(value).unwrap_or_else(|_| {
        Uuid::new_v5(&RUNTIME_ID_NAMESPACE, format!("{scope}:{value}").as_bytes())
    })
}

pub fn default_soul_identity() -> String {
    let soul_path = crate::bootstrap::ghost_home()
        .join("config")
        .join("SOUL.md");
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
    use std::collections::BTreeMap;

    use super::*;
    use crate::agents::registry::{AgentLifecycleState, RegisteredAgent};
    use cortex_storage::queries::external_skill_queries::{
        self, ExternalSkillInstallState, ExternalSkillQuarantineState,
        ExternalSkillVerificationStatus,
    };
    use ghost_agent_loop::tools::skill_bridge::ExecutionContext;
    use ghost_llm::provider::LLMToolCall;
    use ghost_signing::generate_keypair;
    use ghost_skills::artifact::{
        ArtifactExecutionMode, ArtifactSourceKind, SkillArtifact, SkillManifestSource,
    };
    use serde_json::json;
    use wat::parse_str;

    fn registered(name: &str) -> RegisteredAgent {
        RegisteredAgent {
            id: durable_agent_id(name),
            name: name.to_string(),
            state: AgentLifecycleState::Ready,
            channel_bindings: Vec::new(),
            full_access: false,
            capabilities: vec!["shell_execute".into()],
            skills: None,
            baseline_capabilities: vec!["shell_execute".into()],
            baseline_skills: None,
            access_pullback_active: false,
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

        let by_name =
            resolve_runtime_agent(&registry, Some("alpha"), API_SYNTHETIC_AGENT_NAME).unwrap();
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
        assert_eq!(
            parse_or_stable_uuid(&uuid.to_string(), "studio-session"),
            uuid
        );

        let a = parse_or_stable_uuid("legacy-session", "studio-session");
        let b = parse_or_stable_uuid("legacy-session", "studio-session");
        assert_eq!(a, b);
    }

    #[test]
    fn builder_wires_authoritative_kill_state_into_runner() {
        let kill_switch = Arc::new(KillSwitch::new());
        let agent = ResolvedRuntimeAgent::synthetic("phase2-agent");
        let ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent: agent.clone(),
            session_id: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::clone(&kill_switch),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };
        let mut runner = build_live_runner_with_dependencies(
            &ctx,
            RuntimeRunnerDependencies {
                db: None,
                resolved_skills: crate::skill_catalog::ResolvedSkillSet::default(),
                tools_config: ToolsConfig::default(),
                trigger_sender: None,
                convergence_profile: "standard".into(),
                monitor_enabled: false,
                monitor_block_on_degraded: false,
                convergence_state_stale_after: std::time::Duration::from_secs(300),
                cost_tracker: None,
            },
            RunnerBuildOptions::default(),
        )
        .unwrap();

        kill_switch.activate_agent(
            agent.id,
            crate::safety::kill_switch::KillLevel::Pause,
            &cortex_core::safety::trigger::TriggerEvent::ManualPause {
                agent_id: agent.id,
                reason: "test".into(),
                initiated_by: "test".into(),
            },
        );

        let mut log = ghost_agent_loop::runner::GateCheckLog::default();
        let snapshot = ghost_agent_loop::runner::AgentRunner::default_snapshot();
        let run_ctx = runner.build_run_context(agent.id, ctx.session_id, snapshot);

        assert!(matches!(
            runner.check_gates(&run_ctx, &mut log),
            Err(ghost_agent_loop::runner::RunError::AgentPaused)
        ));
    }

    #[test]
    fn ensure_execution_permitted_fails_closed_for_paused_and_quarantined_agents() {
        crate::safety::kill_switch::PLATFORM_KILLED
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let kill_switch = Arc::new(KillSwitch::new());
        let paused_agent = ResolvedRuntimeAgent::synthetic("paused-agent");
        let paused_ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent: paused_agent.clone(),
            session_id: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::clone(&kill_switch),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };

        kill_switch.activate_agent(
            paused_agent.id,
            crate::safety::kill_switch::KillLevel::Pause,
            &cortex_core::safety::trigger::TriggerEvent::ManualPause {
                agent_id: paused_agent.id,
                reason: "test".into(),
                initiated_by: "test".into(),
            },
        );

        assert!(matches!(
            paused_ctx.ensure_execution_permitted(),
            Err(ghost_agent_loop::runner::RunError::AgentPaused)
        ));

        let quarantined_agent = ResolvedRuntimeAgent::synthetic("quarantined-agent");
        let quarantined_ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent: quarantined_agent.clone(),
            session_id: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::clone(&kill_switch),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };

        kill_switch.activate_agent(
            quarantined_agent.id,
            crate::safety::kill_switch::KillLevel::Quarantine,
            &cortex_core::safety::trigger::TriggerEvent::ManualPause {
                agent_id: quarantined_agent.id,
                reason: "test".into(),
                initiated_by: "test".into(),
            },
        );

        assert!(matches!(
            quarantined_ctx.ensure_execution_permitted(),
            Err(ghost_agent_loop::runner::RunError::AgentQuarantined)
        ));

        crate::safety::kill_switch::PLATFORM_KILLED
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[test]
    fn ensure_execution_permitted_fails_closed_for_platform_kill() {
        crate::safety::kill_switch::PLATFORM_KILLED
            .store(false, std::sync::atomic::Ordering::SeqCst);

        let kill_switch = Arc::new(KillSwitch::new());
        let agent = ResolvedRuntimeAgent::synthetic("platform-killed-agent");
        let ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent,
            session_id: Uuid::now_v7(),
            run_id: Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::clone(&kill_switch),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };

        kill_switch.activate_kill_all(&cortex_core::safety::trigger::TriggerEvent::ManualKillAll {
            reason: "test".into(),
            initiated_by: "test".into(),
        });

        assert!(matches!(
            ctx.ensure_execution_permitted(),
            Err(ghost_agent_loop::runner::RunError::PlatformKilled)
        ));

        crate::safety::kill_switch::PLATFORM_KILLED
            .store(false, std::sync::atomic::Ordering::SeqCst);
    }

    #[tokio::test]
    async fn runtime_runner_only_registers_visible_skills_and_auto_grants_policy() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("runtime-skills.db");
        let db = crate::db_pool::create_pool(db_path).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let catalog = crate::skill_catalog::SkillCatalogService::new(
            crate::skill_catalog::definitions::build_compiled_skill_definitions(
                &crate::config::GhostConfig::default(),
            )
            .definitions,
            Arc::clone(&db),
            crate::config::ExternalSkillsConfig::default(),
        )
        .await
        .unwrap();

        let agent = ResolvedRuntimeAgent {
            id: uuid::Uuid::now_v7(),
            name: "runtime-agent".into(),
            full_access: false,
            capabilities: Vec::new(),
            skill_allowlist: Some(vec!["note_take".into()]),
            spending_cap: 5.0,
        };
        let ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent: agent.clone(),
            session_id: uuid::Uuid::now_v7(),
            run_id: uuid::Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::new(KillSwitch::new()),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };

        let resolved_skills = catalog.resolve_for_runtime(&agent, None).unwrap();
        let runner = build_live_runner_with_dependencies(
            &ctx,
            RuntimeRunnerDependencies {
                db: Some(db.legacy_connection().unwrap()),
                resolved_skills,
                tools_config: ToolsConfig::default(),
                trigger_sender: None,
                convergence_profile: "standard".into(),
                monitor_enabled: false,
                monitor_block_on_degraded: false,
                convergence_state_stale_after: std::time::Duration::from_secs(300),
                cost_tracker: None,
            },
            RunnerBuildOptions::default(),
        )
        .unwrap();

        assert!(runner.tool_registry.lookup("skill_note_take").is_some());
        assert!(runner
            .tool_registry
            .lookup("skill_convergence_check")
            .is_some());
        assert!(runner.tool_registry.lookup("skill_git_status").is_none());

        let call = LLMToolCall {
            id: "call-note-take".into(),
            name: "skill_note_take".into(),
            arguments: json!({
                "action": "create",
                "title": "runtime note",
                "content": "policy grants must align with exposure"
            }),
        };
        let exec_ctx = ExecutionContext {
            agent_id: agent.id,
            session_id: ctx.session_id,
            intervention_level: 0,
            session_duration: std::time::Duration::ZERO,
            session_reflection_count: 0,
            is_compaction_flush: false,
        };

        let result = runner
            .tool_executor
            .execute(&call, &runner.tool_registry, &exec_ctx)
            .await
            .unwrap();
        let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        let note_id = output["note_id"].as_str().expect("note id");
        assert_eq!(output["status"], "created");

        let reader = db.read().unwrap();
        let stored = cortex_storage::queries::note_queries::get_note(
            &reader,
            note_id,
            &agent.id.to_string(),
        )
        .unwrap()
        .expect("stored note");
        assert_eq!(stored.title, "runtime note");
    }

    #[tokio::test]
    async fn runtime_runner_registers_and_executes_runtime_visible_external_wasm_skills() {
        let tmp_dir = tempfile::tempdir().unwrap();
        let db_path = tmp_dir.path().join("runtime-external.db");
        let db = crate::db_pool::create_pool(db_path).unwrap();
        {
            let writer = db.writer_for_migrations().await;
            cortex_storage::migrations::run_migrations(&writer).unwrap();
        }

        let managed_dir = tmp_dir.path().join("managed");
        std::fs::create_dir_all(&managed_dir).unwrap();
        let (signing_key, _) = generate_keypair();
        let artifact = SkillArtifact::build(
            SkillManifestSource {
                manifest_schema_version: ghost_skills::artifact::MANIFEST_SCHEMA_VERSION,
                name: "echo".to_string(),
                version: "1.0.0".to_string(),
                publisher: "ghost-test".to_string(),
                description: "external echo".to_string(),
                source_kind: ArtifactSourceKind::Workspace,
                execution_mode: ArtifactExecutionMode::Wasm,
                entrypoint: "module.wasm".to_string(),
                requested_capabilities: Vec::new(),
                declared_privileges: vec!["Pure WASM computation".to_string()],
            },
            BTreeMap::from([("module.wasm".to_string(), echo_module())]),
            &signing_key,
        )
        .unwrap();
        let digest = artifact.artifact_digest().unwrap();
        let artifact_path = managed_dir.join("artifact.ghostskill");
        artifact.write_to_path(&artifact_path).unwrap();

        {
            let writer = db.write().await;
            external_skill_queries::upsert_external_skill_artifact(
                &writer,
                &digest,
                1,
                "echo",
                "1.0.0",
                "ghost-test",
                "external echo",
                "workspace",
                "wasm",
                "module.wasm",
                &artifact_path.display().to_string(),
                &artifact_path.display().to_string(),
                &artifact_path.display().to_string(),
                "{}",
                "[]",
                "[\"Pure WASM computation\"]",
                Some("key-1"),
                256,
            )
            .unwrap();
            external_skill_queries::upsert_external_skill_verification(
                &writer,
                &digest,
                ExternalSkillVerificationStatus::Verified,
                Some("key-1"),
                Some("ghost-test"),
                "{}",
            )
            .unwrap();
            external_skill_queries::upsert_external_skill_quarantine(
                &writer,
                &digest,
                ExternalSkillQuarantineState::Clear,
                None,
                None,
                Some("operator"),
            )
            .unwrap();
            external_skill_queries::upsert_external_skill_install_state(
                &writer,
                &digest,
                "echo",
                "1.0.0",
                ExternalSkillInstallState::Installed,
                Some("operator"),
            )
            .unwrap();
        }

        let catalog = crate::skill_catalog::SkillCatalogService::new(
            Vec::new(),
            Arc::clone(&db),
            crate::config::ExternalSkillsConfig {
                enabled: true,
                execution_enabled: true,
                managed_storage_path: managed_dir.display().to_string(),
                ..crate::config::ExternalSkillsConfig::default()
            },
        )
        .await
        .unwrap();

        let agent = ResolvedRuntimeAgent {
            id: uuid::Uuid::now_v7(),
            name: "external-runtime-agent".into(),
            full_access: false,
            capabilities: Vec::new(),
            skill_allowlist: Some(vec!["echo".into()]),
            spending_cap: 5.0,
        };
        let ctx = RuntimeSafetyContext {
            capability_scope: Vec::new(),
            agent: agent.clone(),
            session_id: uuid::Uuid::now_v7(),
            run_id: uuid::Uuid::now_v7(),
            message_id: None,
            kill_switch: Arc::new(KillSwitch::new()),
            kill_gate: None,
            convergence_profile: "standard".into(),
        };

        let resolved_skills = catalog.resolve_for_runtime(&agent, None).unwrap();
        assert!(resolved_skills
            .visible_skill_names
            .iter()
            .any(|name| name == "echo"));
        assert!(resolved_skills
            .granted_policy_capabilities
            .iter()
            .any(|capability| capability == "skill:echo"));

        let runner = build_live_runner_with_dependencies(
            &ctx,
            RuntimeRunnerDependencies {
                db: Some(db.legacy_connection().unwrap()),
                resolved_skills,
                tools_config: ToolsConfig::default(),
                trigger_sender: None,
                convergence_profile: "standard".into(),
                monitor_enabled: false,
                monitor_block_on_degraded: false,
                convergence_state_stale_after: std::time::Duration::from_secs(300),
                cost_tracker: None,
            },
            RunnerBuildOptions::default(),
        )
        .unwrap();

        assert!(runner.tool_registry.lookup("skill_echo").is_some());

        let result = runner
            .tool_executor
            .execute(
                &LLMToolCall {
                    id: "call-echo".into(),
                    name: "skill_echo".into(),
                    arguments: json!({ "message": "runtime external" }),
                },
                &runner.tool_registry,
                &ExecutionContext {
                    agent_id: agent.id,
                    session_id: ctx.session_id,
                    intervention_level: 0,
                    session_duration: std::time::Duration::ZERO,
                    session_reflection_count: 0,
                    is_compaction_flush: false,
                },
            )
            .await
            .unwrap();
        let output: serde_json::Value = serde_json::from_str(&result.output).unwrap();
        assert_eq!(output, json!({ "message": "runtime external" }));
    }

    fn echo_module() -> Vec<u8> {
        parse_str(
            r#"
            (module
              (memory (export "memory") 2)
              (global $heap (mut i32) (i32.const 1024))
              (func (export "alloc") (param $len i32) (result i32)
                (local $ptr i32)
                global.get $heap
                local.set $ptr
                global.get $heap
                local.get $len
                i32.add
                global.set $heap
                local.get $ptr)
              (func (export "run") (param $input_ptr i32) (param $input_len i32) (result i64)
                local.get $input_ptr
                i64.extend_i32_u
                i64.const 32
                i64.shl
                local.get $input_len
                i64.extend_i32_u
                i64.or))
            "#,
        )
        .unwrap()
    }
}
