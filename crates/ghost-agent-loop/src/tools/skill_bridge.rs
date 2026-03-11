//! SkillBridge — connects the ghost-skills system to the ToolExecutor.
//!
//! Converts resolved skill handlers into `RegisteredTool` entries so the LLM
//! can discover and invoke them as tools during the agent loop.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use ghost_llm::provider::ToolSchema;
use ghost_skills::skill::{Skill, SkillContext, SkillError};
use rusqlite::Connection;
use uuid::Uuid;

use super::registry::{RegisteredTool, ToolRegistry};

/// Per-call execution context threaded from RunContext into tool dispatch.
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub execution_id: Option<String>,
    pub route_kind: Option<String>,
    pub interactive: bool,
    pub intervention_level: u8,
    pub session_duration: Duration,
    pub session_reflection_count: u32,
    pub is_compaction_flush: bool,
}

/// Shared execution environment available to all skill handlers.
#[derive(Clone)]
pub struct SkillHandlerEnvironment {
    pub db: Arc<Mutex<Connection>>,
    pub convergence_profile: String,
}

/// Runtime skill handler abstraction shared by compiled and external skills.
pub trait SkillHandler: Send + Sync {
    fn description(&self) -> String;
    fn parameters_schema(&self) -> serde_json::Value;
    fn removable(&self) -> bool;
    fn execute(
        &self,
        env: &SkillHandlerEnvironment,
        input: &serde_json::Value,
        exec_ctx: &ExecutionContext,
    ) -> Result<serde_json::Value, SkillError>;
}

/// Adapter exposing a compiled `Skill` through the generic handler seam.
pub struct CompiledSkillHandler {
    skill: Arc<dyn Skill>,
}

impl CompiledSkillHandler {
    pub fn new(skill: Arc<dyn Skill>) -> Self {
        Self { skill }
    }
}

impl SkillHandler for CompiledSkillHandler {
    fn description(&self) -> String {
        self.skill.description().to_string()
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.skill.parameters_schema()
    }

    fn removable(&self) -> bool {
        self.skill.removable()
    }

    fn execute(
        &self,
        env: &SkillHandlerEnvironment,
        input: &serde_json::Value,
        exec_ctx: &ExecutionContext,
    ) -> Result<serde_json::Value, SkillError> {
        let db = env
            .db
            .lock()
            .map_err(|_| SkillError::Storage("DB lock poisoned".into()))?;

        let ctx = SkillContext {
            db: &db,
            agent_id: exec_ctx.agent_id,
            session_id: exec_ctx.session_id,
            convergence_profile: &env.convergence_profile,
        };

        self.skill.execute(&ctx, input)
    }
}

/// Bridge between resolved skill handlers and the ToolRegistry/ToolExecutor.
pub struct SkillBridge {
    handlers: Arc<HashMap<String, Arc<dyn SkillHandler>>>,
    env: SkillHandlerEnvironment,
}

impl SkillBridge {
    pub fn new(
        handlers: Arc<HashMap<String, Arc<dyn SkillHandler>>>,
        db: Arc<Mutex<Connection>>,
        convergence_profile: String,
    ) -> Self {
        Self {
            handlers,
            env: SkillHandlerEnvironment {
                db,
                convergence_profile,
            },
        }
    }

    /// Generate `RegisteredTool` entries for all resolved skill handlers.
    ///
    /// Each skill becomes a tool with a `skill_` prefix (e.g. `skill_note_take`).
    pub fn registered_tools(&self) -> Vec<RegisteredTool> {
        self.handlers
            .iter()
            .map(|(name, handler)| {
                let tool_name = format!("skill_{name}");
                RegisteredTool {
                    name: tool_name.clone(),
                    description: handler.description(),
                    schema: ToolSchema {
                        name: tool_name,
                        description: handler.description(),
                        parameters: handler.parameters_schema(),
                    },
                    capability: format!("skill:{name}"),
                    // Safety skills (not removable) are always visible (level 5 = never hidden).
                    // Other skills are visible up to intervention level 3.
                    hidden_at_level: if handler.removable() { 3 } else { 5 },
                    timeout_secs: 30,
                }
            })
            .collect()
    }

    /// Execute a resolved skill handler by name.
    pub fn execute(
        &self,
        skill_name: &str,
        input: &serde_json::Value,
        exec_ctx: &ExecutionContext,
    ) -> Result<serde_json::Value, SkillError> {
        let handler = self.handlers.get(skill_name).ok_or_else(|| {
            SkillError::Internal(format!("skill '{skill_name}' not found in bridge"))
        })?;
        handler.execute(&self.env, input, exec_ctx)
    }

    /// Check if the bridge has a skill registered under the given name.
    pub fn has_skill(&self, skill_name: &str) -> bool {
        self.handlers.contains_key(skill_name)
    }
}

/// Register skills from the bridge into the tool registry.
///
/// If `skill_allowlist` is `Some`, only skills in the list (plus
/// non-removable safety skills) are registered. If `None`, all skills
/// are registered.
pub fn register_skills(
    bridge: &SkillBridge,
    registry: &mut ToolRegistry,
    skill_allowlist: Option<&[String]>,
) {
    let mut count = 0usize;
    for tool in bridge.registered_tools() {
        let skill_name = tool.name.strip_prefix("skill_").unwrap_or(&tool.name);

        if let Some(allowlist) = skill_allowlist {
            let is_safety = bridge
                .handlers
                .get(skill_name)
                .is_some_and(|handler| !handler.removable());
            if !is_safety && !allowlist.iter().any(|a| a == skill_name) {
                continue;
            }
        }

        registry.register(tool);
        count += 1;
    }
    tracing::info!(skill_count = count, "Skills registered as LLM tools");
}
