//! `agent_spawn_safe` — spawn a sub-agent with capability constraints.
//!
//! Creates a delegation record and convergence link for a new child agent.
//! The actual agent spawn happens at the gateway layer using the returned
//! configuration.
//!
//! ## Constraints (per GHOST_PC_CONTROL_PLAN.md §8)
//!
//! - Child capabilities must be a subset of parent capabilities
//! - Child gets half of parent's remaining token budget
//! - Child is limited to a single session per delegation
//! - Child inherits parent's convergence score (can only increase)
//!
//! ## Input
//!
//! | Field                 | Type   | Required | Description                          |
//! |-----------------------|--------|----------|--------------------------------------|
//! | `task`                | string | yes      | Task description for the child       |
//! | `capabilities`        | array  | yes      | Capabilities to grant to child       |
//! | `parent_capabilities` | array  | yes      | Parent's current capabilities        |
//! | `remaining_budget`    | number | yes      | Parent's remaining token budget      |

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct AgentSpawnSafeSkill;

impl Skill for AgentSpawnSafeSkill {
    fn name(&self) -> &str {
        "agent_spawn_safe"
    }

    fn description(&self) -> &str {
        "Spawn a sub-agent with capability constraints and inherited convergence"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        // 1. Check Phase 10 prerequisites
        super::check_delegation_prerequisites(ctx)?;

        // 2. Extract & validate input
        let task_description = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidInput("missing required field 'task'".into()))?;

        if task_description.trim().is_empty() {
            return Err(SkillError::InvalidInput(
                "task description must not be empty".into(),
            ));
        }

        let requested_caps: Vec<String> = input
            .get("capabilities")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'capabilities' (array of strings)".into(),
                )
            })?;

        let parent_caps: Vec<String> = input
            .get("parent_capabilities")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'parent_capabilities' (array of strings)".into(),
                )
            })?;

        let remaining_budget = input
            .get("remaining_budget")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'remaining_budget' (number)".into(),
                )
            })?;

        // 3. Validate capabilities subset
        let invalid_caps: Vec<&String> = requested_caps
            .iter()
            .filter(|cap| !parent_caps.contains(cap))
            .collect();

        if !invalid_caps.is_empty() {
            return Err(SkillError::InvalidInput(format!(
                "requested capabilities not in parent set: {:?}",
                invalid_caps
            )));
        }

        // 4. Calculate child budget (half of parent's remaining)
        let child_budget = remaining_budget / 2.0;

        // 5. Get parent convergence score
        let sender_id = ctx.agent_id.to_string();
        let parent_score_row =
            cortex_storage::queries::convergence_score_queries::latest_by_agent(ctx.db, &sender_id)
                .map_err(|e| SkillError::Storage(format!("query convergence: {e}")))?;

        let (parent_score, parent_level) = match parent_score_row {
            Some(ref row) => (row.composite_score, row.level),
            None => (0.0, 0),
        };

        // 6. Create delegation record (self-delegation variant)
        let child_agent_id = Uuid::now_v7().to_string();
        let delegation_id = Uuid::now_v7().to_string();
        let id = Uuid::now_v7().to_string();
        let offer_message_id = Uuid::now_v7().to_string();

        let previous_hash =
            cortex_storage::queries::delegation_state_queries::query_last_hash(ctx.db, &sender_id)
                .map_err(|e| SkillError::Storage(format!("query last hash: {e}")))?
                .unwrap_or_else(super::zero_hash);

        let hash_input = format!(
            "{sender_id}:spawn:{child_agent_id}:{task_description}:{delegation_id}:{}",
            super::to_hex(&previous_hash)
        );
        let event_hash = super::compute_event_hash(hash_input.as_bytes());

        cortex_storage::queries::delegation_state_queries::insert_delegation(
            ctx.db,
            &id,
            &delegation_id,
            &sender_id,
            &child_agent_id,
            task_description,
            &offer_message_id,
            &event_hash,
            &previous_hash,
        )
        .map_err(|e| SkillError::Storage(format!("insert delegation: {e}")))?;

        // 7. Link convergence parent → child
        let link_id = Uuid::now_v7().to_string();
        cortex_storage::queries::convergence_propagation_queries::link_parent_child(
            ctx.db,
            &link_id,
            &sender_id,
            &child_agent_id,
            &delegation_id,
            parent_score,
            parent_level,
        )
        .map_err(|e| SkillError::Storage(format!("link convergence: {e}")))?;

        // 8. Return child configuration for the gateway to apply
        Ok(serde_json::json!({
            "delegation_id": delegation_id,
            "child_agent_id": child_agent_id,
            "child_config": {
                "capabilities": requested_caps,
                "spending_cap": child_budget,
                "session_limit": 1,
                "initial_convergence_score": parent_score,
                "initial_convergence_level": parent_level,
            },
            "parent_remaining_budget": remaining_budget - child_budget,
        }))
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let task = input.get("task").and_then(|v| v.as_str())?;
        let caps = input
            .get("capabilities")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0);
        Some(format!(
            "Spawn sub-agent for \"{task}\" with {caps} capabilities"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection, agent_id: Uuid) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id,
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        }
    }

    fn seed_convergence_history(db: &rusqlite::Connection, agent_id: &str, count: usize) {
        let hash = vec![0u8; 32];
        for i in 0..count {
            let id = Uuid::now_v7().to_string();
            cortex_storage::queries::convergence_score_queries::insert_score(
                db,
                &id,
                agent_id,
                Some(&Uuid::now_v7().to_string()),
                0.1,
                "{}",
                0,
                "standard",
                &format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1),
                &hash,
                &hash,
            )
            .unwrap();
        }
    }

    #[test]
    fn spawn_validates_capability_subset() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill.execute(
            &ctx,
            &serde_json::json!({
                "task": "test",
                "capabilities": ["read", "write", "admin"],
                "parent_capabilities": ["read", "write"],
                "remaining_budget": 100.0,
            }),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "INVALID_INPUT");
    }

    #[test]
    fn spawn_calculates_half_budget() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "task": "analyze data",
                    "capabilities": ["read"],
                    "parent_capabilities": ["read", "write"],
                    "remaining_budget": 100.0,
                }),
            )
            .unwrap();

        assert_eq!(result["child_config"]["spending_cap"], 50.0);
        assert_eq!(result["parent_remaining_budget"], 50.0);
    }

    #[test]
    fn spawn_sets_single_session_limit() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "task": "one-shot task",
                    "capabilities": ["read"],
                    "parent_capabilities": ["read"],
                    "remaining_budget": 50.0,
                }),
            )
            .unwrap();

        assert_eq!(result["child_config"]["session_limit"], 1);
    }

    #[test]
    fn spawn_inherits_convergence() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "task": "child task",
                    "capabilities": ["read"],
                    "parent_capabilities": ["read"],
                    "remaining_budget": 80.0,
                }),
            )
            .unwrap();

        assert_eq!(result["child_config"]["initial_convergence_score"], 0.1);
        assert_eq!(result["child_config"]["initial_convergence_level"], 0);
    }

    #[test]
    fn spawn_rejects_empty_task() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill.execute(
            &ctx,
            &serde_json::json!({
                "task": "  ",
                "capabilities": ["read"],
                "parent_capabilities": ["read"],
                "remaining_budget": 50.0,
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn spawn_creates_delegation_record() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        seed_convergence_history(&db, &agent_id.to_string(), 35);
        let ctx = test_ctx(&db, agent_id);

        let result = AgentSpawnSafeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "task": "spawned task",
                    "capabilities": ["read"],
                    "parent_capabilities": ["read", "write"],
                    "remaining_budget": 100.0,
                }),
            )
            .unwrap();

        let delegation_id = result["delegation_id"].as_str().unwrap();

        // Verify delegation exists in DB
        let delegation = cortex_storage::queries::delegation_state_queries::query_by_delegation_id(
            &db,
            delegation_id,
        )
        .unwrap();
        assert!(delegation.is_some());
        assert_eq!(delegation.unwrap().state, "Offered");
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(AgentSpawnSafeSkill.name(), "agent_spawn_safe");
        assert!(AgentSpawnSafeSkill.removable());
        assert_eq!(AgentSpawnSafeSkill.source(), SkillSource::Bundled);
    }
}
