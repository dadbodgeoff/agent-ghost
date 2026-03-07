//! `check_task_status` — query the status of a delegated task.
//!
//! Read-only skill that queries the local `delegation_state` table
//! and optionally merges remote A2A task status.
//!
//! ## Input
//!
//! | Field            | Type   | Required | Description                   |
//! |------------------|--------|----------|-------------------------------|
//! | `delegation_id`  | string | yes      | Delegation UUID to query      |
//! | `remote_status`  | object | no       | Pre-fetched remote task status|

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct CheckTaskStatusSkill;

impl Skill for CheckTaskStatusSkill {
    fn name(&self) -> &str {
        "check_task_status"
    }

    fn description(&self) -> &str {
        "Query the status of a delegated task"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let delegation_id = input
            .get("delegation_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'delegation_id'".into())
            })?;

        // Query local delegation state
        let delegation = cortex_storage::queries::delegation_state_queries::query_by_delegation_id(
            ctx.db,
            delegation_id,
        )
        .map_err(|e| SkillError::Storage(format!("query delegation: {e}")))?
        .ok_or_else(|| {
            SkillError::InvalidInput(format!("delegation '{delegation_id}' not found"))
        })?;

        // Check if caller is the sender (authorization)
        let agent_id_str = ctx.agent_id.to_string();
        if delegation.sender_id != agent_id_str {
            return Err(SkillError::AuthorizationDenied(
                "only the delegating agent can check task status".into(),
            ));
        }

        // Build response with local state
        let mut result = serde_json::json!({
            "delegation_id": delegation.delegation_id,
            "sender_id": delegation.sender_id,
            "recipient_id": delegation.recipient_id,
            "task": delegation.task,
            "state": delegation.state,
            "created_at": delegation.created_at,
        });

        // Merge pre-fetched remote status if provided
        if let Some(remote) = input.get("remote_status") {
            result["remote_status"] = remote.clone();
        }

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let id = input.get("delegation_id").and_then(|v| v.as_str())?;
        Some(format!("Check status of delegation {id}"))
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

    fn seed_delegation(
        db: &rusqlite::Connection,
        delegation_id: &str,
        sender_id: &str,
        recipient_id: &str,
    ) {
        let id = Uuid::now_v7().to_string();
        let offer_msg = Uuid::now_v7().to_string();
        let hash = vec![0u8; 32];
        cortex_storage::queries::delegation_state_queries::insert_delegation(
            db,
            &id,
            delegation_id,
            sender_id,
            recipient_id,
            "test task",
            &offer_msg,
            &hash,
            &hash,
        )
        .unwrap();
    }

    #[test]
    fn check_returns_local_state() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let ctx = test_ctx(&db, agent_id);
        let delegation_id = Uuid::now_v7().to_string();
        let recipient_id = Uuid::now_v7().to_string();

        seed_delegation(&db, &delegation_id, &agent_id.to_string(), &recipient_id);

        let result = CheckTaskStatusSkill
            .execute(&ctx, &serde_json::json!({ "delegation_id": delegation_id }))
            .unwrap();

        assert_eq!(result["state"], "Offered");
        assert_eq!(result["delegation_id"], delegation_id);
        assert_eq!(result["recipient_id"], recipient_id);
    }

    #[test]
    fn check_merges_remote_status() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let ctx = test_ctx(&db, agent_id);
        let delegation_id = Uuid::now_v7().to_string();
        let recipient_id = Uuid::now_v7().to_string();

        seed_delegation(&db, &delegation_id, &agent_id.to_string(), &recipient_id);

        let result = CheckTaskStatusSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "delegation_id": delegation_id,
                    "remote_status": { "a2a_status": "working", "progress": 0.5 },
                }),
            )
            .unwrap();

        assert_eq!(result["state"], "Offered");
        assert_eq!(result["remote_status"]["a2a_status"], "working");
    }

    #[test]
    fn check_not_found() {
        let db = test_db();
        let ctx = test_ctx(&db, Uuid::now_v7());

        let result = CheckTaskStatusSkill
            .execute(&ctx, &serde_json::json!({ "delegation_id": "nonexistent" }));
        assert!(result.is_err());
    }

    #[test]
    fn check_rejects_non_sender() {
        let db = test_db();
        let sender_id = Uuid::now_v7();
        let other_agent = Uuid::now_v7();
        let ctx = test_ctx(&db, other_agent);
        let delegation_id = Uuid::now_v7().to_string();

        seed_delegation(
            &db,
            &delegation_id,
            &sender_id.to_string(),
            &Uuid::now_v7().to_string(),
        );

        let result = CheckTaskStatusSkill
            .execute(&ctx, &serde_json::json!({ "delegation_id": delegation_id }));
        assert!(result.is_err());
    }

    #[test]
    fn check_missing_delegation_id() {
        let db = test_db();
        let ctx = test_ctx(&db, Uuid::now_v7());

        let result = CheckTaskStatusSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(CheckTaskStatusSkill.name(), "check_task_status");
        assert!(CheckTaskStatusSkill.removable());
        assert_eq!(CheckTaskStatusSkill.source(), SkillSource::Bundled);
    }
}
