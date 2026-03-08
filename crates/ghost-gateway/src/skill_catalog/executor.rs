use std::sync::Arc;
use std::time::Duration;

use ghost_agent_loop::tools::skill_bridge::{ExecutionContext, SkillHandlerEnvironment};
use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::engine::{CorpPolicy, PolicyDecision, PolicyEngine};
use ghost_skills::sandbox::native_sandbox::{NativeContainmentMode, NativeSandbox};
use rusqlite::Connection;

use super::dto::ExecuteSkillResponseDto;
use super::service::{
    ResolvedSkill, ResolvedSkillMetadata, SkillCatalogError, SkillCatalogService,
};
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
    #[error("native sandbox denied execution: {0}")]
    NativeSandbox(String),
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
        let resolved = self.catalog.resolve_for_execute(skill_name, agent)?;
        self.ensure_policy_permitted(
            &resolved.metadata.policy_capability,
            skill_name,
            agent,
            session_id,
            input,
        )?;
        let native_sandbox = self.ensure_native_sandbox(&resolved.metadata, skill_name)?;

        if resolved.compiled_skill.is_some()
            && native_sandbox
                .as_ref()
                .is_some_and(|sandbox| sandbox.mode() == NativeContainmentMode::ReadOnly)
        {
            let conn = self
                .db
                .read()
                .map_err(|error| SkillCatalogExecutionError::DbPool(error.to_string()))?;
            return self.execute_resolved_with_connection(
                &conn, skill_name, agent, session_id, input, resolved,
            );
        }

        let legacy = self
            .db
            .legacy_connection()
            .map_err(|e| SkillCatalogExecutionError::DbPool(e.to_string()))?;
        let conn = legacy
            .lock()
            .map_err(|_| SkillCatalogExecutionError::DbLockPoisoned)?;
        self.execute_resolved_with_connection(&conn, skill_name, agent, session_id, input, resolved)
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
            &resolved.metadata.policy_capability,
            skill_name,
            agent,
            session_id,
            input,
        )?;
        self.ensure_native_sandbox(&resolved.metadata, skill_name)?;
        self.execute_resolved_with_connection(conn, skill_name, agent, session_id, input, resolved)
    }

    fn execute_resolved_with_connection(
        &self,
        conn: &Connection,
        skill_name: &str,
        agent: &ResolvedRuntimeAgent,
        session_id: uuid::Uuid,
        input: &serde_json::Value,
        resolved: ResolvedSkill,
    ) -> Result<ExecuteSkillResponseDto, SkillCatalogExecutionError> {
        let result = if let Some(skill) = resolved.compiled_skill {
            let ctx = ghost_skills::skill::SkillContext {
                db: conn,
                agent_id: agent.id,
                session_id,
                convergence_profile: &self.convergence_profile,
            };
            skill.execute(&ctx, input)?
        } else {
            let environment = SkillHandlerEnvironment {
                db: self
                    .db
                    .legacy_connection()
                    .map_err(|e| SkillCatalogExecutionError::DbPool(e.to_string()))?,
                convergence_profile: self.convergence_profile.clone(),
            };
            let exec_ctx = ExecutionContext {
                agent_id: agent.id,
                session_id,
                intervention_level: 0,
                session_duration: Duration::ZERO,
                session_reflection_count: 0,
                is_compaction_flush: false,
            };
            resolved.handler.execute(&environment, input, &exec_ctx)?
        };
        Ok(ExecuteSkillResponseDto {
            skill: skill_name.to_string(),
            result,
        })
    }

    fn ensure_native_sandbox(
        &self,
        metadata: &ResolvedSkillMetadata,
        skill_name: &str,
    ) -> Result<Option<NativeSandbox>, SkillCatalogExecutionError> {
        let Some(profile) = metadata.native_containment.as_ref() else {
            return Ok(None);
        };

        let sandbox = NativeSandbox::from_profile(profile)
            .map_err(|error| SkillCatalogExecutionError::NativeSandbox(error.to_string()))?;
        let required_capability = match profile.mode {
            NativeContainmentMode::ReadOnly => "db_read",
            NativeContainmentMode::Transactional => "db_write",
            NativeContainmentMode::HostInteraction => "host_interaction",
        };
        sandbox
            .validate_tool_call(skill_name, required_capability)
            .map_err(|error| SkillCatalogExecutionError::NativeSandbox(error.to_string()))?;
        Ok(Some(sandbox))
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
