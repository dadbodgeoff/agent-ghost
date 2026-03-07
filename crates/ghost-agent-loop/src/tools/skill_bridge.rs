//! SkillBridge — connects the ghost-skills system to the ToolExecutor.
//!
//! Converts registered `Skill` instances into `RegisteredTool` entries
//! so the LLM can discover and invoke them as tools during the agent loop.

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
    pub intervention_level: u8,
    pub session_duration: Duration,
    pub session_reflection_count: u32,
    pub is_compaction_flush: bool,
}

/// Bridge between the Skill system and the ToolRegistry/ToolExecutor.
///
/// Holds the shared skill map, a DB connection for building `SkillContext`,
/// and the convergence profile name.
pub struct SkillBridge {
    skills: Arc<HashMap<String, Box<dyn Skill>>>,
    db: Arc<Mutex<Connection>>,
    convergence_profile: String,
}

impl SkillBridge {
    pub fn new(
        skills: Arc<HashMap<String, Box<dyn Skill>>>,
        db: Arc<Mutex<Connection>>,
        convergence_profile: String,
    ) -> Self {
        Self {
            skills,
            db,
            convergence_profile,
        }
    }

    /// Generate `RegisteredTool` entries for all skills.
    ///
    /// Each skill becomes a tool with a `skill_` prefix (e.g. `skill_note_take`).
    pub fn registered_tools(&self) -> Vec<RegisteredTool> {
        self.skills
            .iter()
            .map(|(name, skill)| {
                let tool_name = format!("skill_{name}");
                RegisteredTool {
                    name: tool_name.clone(),
                    description: skill.description().to_string(),
                    schema: ToolSchema {
                        name: tool_name,
                        description: skill.description().to_string(),
                        parameters: skill.parameters_schema(),
                    },
                    capability: format!("skill:{name}"),
                    // Safety skills (not removable) are always visible (level 5 = never hidden).
                    // Other skills are visible up to intervention level 3.
                    hidden_at_level: if skill.removable() { 3 } else { 5 },
                    timeout_secs: 30,
                }
            })
            .collect()
    }

    /// Execute a skill by name, constructing a `SkillContext` from the
    /// given `ExecutionContext`.
    pub fn execute(
        &self,
        skill_name: &str,
        input: &serde_json::Value,
        exec_ctx: &ExecutionContext,
    ) -> Result<serde_json::Value, SkillError> {
        let skill = self.skills.get(skill_name).ok_or_else(|| {
            SkillError::Internal(format!("skill '{skill_name}' not found in bridge"))
        })?;

        let db = self
            .db
            .lock()
            .map_err(|_| SkillError::Storage("DB lock poisoned".into()))?;

        let ctx = SkillContext {
            db: &db,
            agent_id: exec_ctx.agent_id,
            session_id: exec_ctx.session_id,
            convergence_profile: &self.convergence_profile,
        };

        skill.execute(&ctx, input)
    }

    /// Check if the bridge has a skill registered under the given name.
    pub fn has_skill(&self, skill_name: &str) -> bool {
        self.skills.contains_key(skill_name)
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
                .skills
                .get(skill_name)
                .map_or(false, |s| !s.removable());
            if !is_safety && !allowlist.iter().any(|a| a == skill_name) {
                continue;
            }
        }

        registry.register(tool);
        count += 1;
    }
    tracing::info!(skill_count = count, "Skills registered as LLM tools");
}
