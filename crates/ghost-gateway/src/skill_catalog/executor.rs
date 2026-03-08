use std::sync::Arc;
use std::time::Duration;

use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::engine::{CorpPolicy, PolicyDecision, PolicyEngine};
use rusqlite::Connection;

use super::dto::ExecuteSkillResponseDto;
use super::service::{SkillCatalogError, SkillCatalogService};
use crate::db_pool::DbPool;
use crate::runtime_safety::ResolvedRuntimeAgent;

#[derive(Debug, thiserror::Error)]
pub enum SkillCatalogExecutionError {
    #[error(transparent)]
    Catalog(#[from] SkillCatalogError),
    #[error("db pool error: {0}")]
    DbPool(String),
    #[error("db lock poisoned")]
    DbLockPoisoned,
    #[error("policy denied: {0}")]
    PolicyDenied(String),
    #[error("policy escalation required: {0}")]
    PolicyEscalation(String),
    #[error(transparent)]
    Skill(#[from] ghost_skills::skill::SkillError),
}

#[derive(Clone)]
pub struct SkillCatalogExecutor {
    catalog: Arc<SkillCatalogService>,
    db: Arc<DbPool>,
    convergence_profile: String,
}

impl SkillCatalogExecutor {
    pub fn new(
        catalog: Arc<SkillCatalogService>,
        db: Arc<DbPool>,
        convergence_profile: String,
    ) -> Self {
        Self {
            catalog,
            db,
            convergence_profile,
        }
    }

    pub fn execute(
        &self,
        skill_name: &str,
        agent: &ResolvedRuntimeAgent,
        session_id: uuid::Uuid,
        input: &serde_json::Value,
    ) -> Result<ExecuteSkillResponseDto, SkillCatalogExecutionError> {
        let db = self
            .db
            .legacy_connection()
            .map_err(|e| SkillCatalogExecutionError::DbPool(e.to_string()))?;
        let db = db
            .lock()
            .map_err(|_| SkillCatalogExecutionError::DbLockPoisoned)?;
        self.execute_with_connection(&db, skill_name, agent, session_id, input)
    }

    pub fn execute_with_connection(
        &self,
        conn: &Connection,
        skill_name: &str,
        agent: &ResolvedRuntimeAgent,
        session_id: uuid::Uuid,
        input: &serde_json::Value,
    ) -> Result<ExecuteSkillResponseDto, SkillCatalogExecutionError> {
        let resolved = self.catalog.resolve_for_execute(skill_name, agent)?;
        self.ensure_policy_permitted(
            &resolved.definition.policy_capability,
            skill_name,
            agent,
            session_id,
            input,
        )?;

        let ctx = self.skill_context(conn, agent.id, session_id);

        let result = resolved.skill.execute(&ctx, input)?;
        Ok(ExecuteSkillResponseDto {
            skill: skill_name.to_string(),
            result,
        })
    }

    fn skill_context<'a>(
        &'a self,
        db: &'a Connection,
        agent_id: uuid::Uuid,
        session_id: uuid::Uuid,
    ) -> ghost_skills::skill::SkillContext<'a> {
        ghost_skills::skill::SkillContext {
            db,
            agent_id,
            session_id,
            convergence_profile: &self.convergence_profile,
        }
    }

    fn ensure_policy_permitted(
        &self,
        policy_capability: &str,
        skill_name: &str,
        agent: &ResolvedRuntimeAgent,
        session_id: uuid::Uuid,
        input: &serde_json::Value,
    ) -> Result<(), SkillCatalogExecutionError> {
        let mut policy = PolicyEngine::new(CorpPolicy::new());
        for capability in &agent.capabilities {
            policy.grant_capability(agent.id, capability.clone());
        }
        policy.grant_capability(agent.id, policy_capability.to_string());

        let call = ToolCall {
            tool_name: format!("skill_{skill_name}"),
            arguments: input.clone(),
            capability: policy_capability.to_string(),
            is_compaction_flush: false,
        };
        let ctx = PolicyContext {
            agent_id: agent.id,
            session_id,
            intervention_level: 0,
            session_duration: Duration::ZERO,
            session_denial_count: 0,
            is_compaction_flush: false,
            session_reflection_count: 0,
        };

        match policy.evaluate(&call, &ctx) {
            PolicyDecision::Permit => Ok(()),
            PolicyDecision::Deny(feedback) => {
                Err(SkillCatalogExecutionError::PolicyDenied(feedback.reason))
            }
            PolicyDecision::Escalate(reason) => {
                Err(SkillCatalogExecutionError::PolicyEscalation(reason))
            }
        }
    }
}
