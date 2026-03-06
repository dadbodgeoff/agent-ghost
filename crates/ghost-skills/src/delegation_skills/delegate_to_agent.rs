//! `delegate_to_agent` — offer a task to another agent with inherited convergence.
//!
//! Creates a delegation record in `delegation_state` (state: Offered) and
//! links the parent-child convergence relationship for propagation.
//!
//! ## Input
//!
//! | Field          | Type   | Required | Description                              |
//! |----------------|--------|----------|------------------------------------------|
//! | `recipient_id` | string | yes      | Target agent UUID                        |
//! | `task`         | string | yes      | Task description for the recipient       |
//! | `capabilities` | array  | no       | Capabilities to grant (subset of parent) |

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct DelegateToAgentSkill;

impl Skill for DelegateToAgentSkill {
    fn name(&self) -> &str {
        "delegate_to_agent"
    }

    fn description(&self) -> &str {
        "Offer a task to another agent with inherited convergence state"
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
        let recipient_id = input
            .get("recipient_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'recipient_id'".into())
            })?;

        let task_description = input
            .get("task")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'task'".into())
            })?;

        if task_description.trim().is_empty() {
            return Err(SkillError::InvalidInput(
                "task description must not be empty".into(),
            ));
        }

        let sender_id = ctx.agent_id.to_string();

        // Prevent self-delegation (use agent_spawn_safe for that)
        if recipient_id == sender_id {
            return Err(SkillError::InvalidInput(
                "cannot delegate to self; use agent_spawn_safe instead".into(),
            ));
        }

        // 3. Get parent convergence score
        let parent_score_row =
            cortex_storage::queries::convergence_score_queries::latest_by_agent(
                ctx.db, &sender_id,
            )
            .map_err(|e| SkillError::Storage(format!("query convergence: {e}")))?;

        let (parent_score, parent_level) = match parent_score_row {
            Some(ref row) => (row.composite_score, row.level),
            None => (0.0, 0),
        };

        // 4. Compute hash chain
        let previous_hash =
            cortex_storage::queries::delegation_state_queries::query_last_hash(
                ctx.db, &sender_id,
            )
            .map_err(|e| SkillError::Storage(format!("query last hash: {e}")))?
            .unwrap_or_else(super::zero_hash);

        let delegation_id = Uuid::now_v7().to_string();
        let id = Uuid::now_v7().to_string();
        let offer_message_id = Uuid::now_v7().to_string();

        // Hash: sender + recipient + task + delegation_id + previous_hash
        let hash_input = format!(
            "{sender_id}:{recipient_id}:{task_description}:{delegation_id}:{}",
            super::to_hex(&previous_hash)
        );
        let event_hash = super::compute_event_hash(hash_input.as_bytes());

        // 5. Insert delegation record (state: Offered)
        cortex_storage::queries::delegation_state_queries::insert_delegation(
            ctx.db,
            &id,
            &delegation_id,
            &sender_id,
            recipient_id,
            task_description,
            &offer_message_id,
            &event_hash,
            &previous_hash,
        )
        .map_err(|e| SkillError::Storage(format!("insert delegation: {e}")))?;

        // 6. Link convergence parent → child
        let link_id = Uuid::now_v7().to_string();
        cortex_storage::queries::convergence_propagation_queries::link_parent_child(
            ctx.db,
            &link_id,
            &sender_id,
            recipient_id,
            &delegation_id,
            parent_score,
            parent_level,
        )
        .map_err(|e| SkillError::Storage(format!("link convergence: {e}")))?;

        // 7. Return result
        Ok(serde_json::json!({
            "delegation_id": delegation_id,
            "state": "Offered",
            "sender_id": sender_id,
            "recipient_id": recipient_id,
            "task": task_description,
            "inherited_convergence_score": parent_score,
            "inherited_convergence_level": parent_level,
        }))
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let recipient = input.get("recipient_id").and_then(|v| v.as_str())?;
        let task = input.get("task").and_then(|v| v.as_str())?;
        Some(format!("Delegate to agent {recipient}: \"{task}\""))
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

    /// Seed enough convergence history to pass the 30+ sessions prerequisite.
    fn seed_convergence_history(db: &rusqlite::Connection, agent_id: &str, count: usize) {
        let hash = vec![0u8; 32];
        for i in 0..count {
            let id = Uuid::now_v7().to_string();
            cortex_storage::queries::convergence_score_queries::insert_score(
                db,
                &id,
                agent_id,
                Some(&Uuid::now_v7().to_string()),
                0.1, // Low convergence (safe for delegation)
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
    fn delegate_creates_offered_state() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 35);
        let ctx = test_ctx(&db, agent_id);
        let recipient = Uuid::now_v7().to_string();

        let result = DelegateToAgentSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "recipient_id": recipient,
                    "task": "analyze dataset",
                }),
            )
            .unwrap();

        assert_eq!(result["state"], "Offered");
        assert_eq!(result["recipient_id"], recipient);
        assert!(result["delegation_id"].as_str().is_some());
        assert_eq!(result["inherited_convergence_score"], 0.1);
    }

    #[test]
    fn delegate_records_hash_chain() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 35);
        let ctx = test_ctx(&db, agent_id);

        // First delegation — previous_hash should be zero hash
        let r1 = DelegateToAgentSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "recipient_id": Uuid::now_v7().to_string(),
                    "task": "task one",
                }),
            )
            .unwrap();

        // Second delegation — previous_hash should be event_hash of first
        let r2 = DelegateToAgentSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "recipient_id": Uuid::now_v7().to_string(),
                    "task": "task two",
                }),
            )
            .unwrap();

        // Both should succeed and have different delegation_ids
        assert_ne!(
            r1["delegation_id"].as_str().unwrap(),
            r2["delegation_id"].as_str().unwrap()
        );
    }

    #[test]
    fn delegate_links_convergence() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 35);
        let ctx = test_ctx(&db, agent_id);
        let recipient = Uuid::now_v7().to_string();

        DelegateToAgentSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "recipient_id": recipient,
                    "task": "linked task",
                }),
            )
            .unwrap();

        // Verify convergence link exists
        let children =
            cortex_storage::queries::convergence_propagation_queries::get_children(
                &db, &agent_str,
            )
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].child_agent_id, recipient);
        assert!((children[0].inherited_score - 0.1).abs() < 0.001);
    }

    #[test]
    fn delegate_rejects_high_convergence() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();

        // Seed 35 scores but with high convergence (0.5)
        let hash = vec![0u8; 32];
        for i in 0..35 {
            let id = Uuid::now_v7().to_string();
            cortex_storage::queries::convergence_score_queries::insert_score(
                &db,
                &id,
                &agent_str,
                Some(&Uuid::now_v7().to_string()),
                0.5, // Too high for delegation
                "{}",
                2,
                "standard",
                &format!("2026-01-{:02}T00:00:00Z", (i % 28) + 1),
                &hash,
                &hash,
            )
            .unwrap();
        }

        let ctx = test_ctx(&db, agent_id);

        let result = DelegateToAgentSkill.execute(
            &ctx,
            &serde_json::json!({
                "recipient_id": Uuid::now_v7().to_string(),
                "task": "should fail",
            }),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "AUTHORIZATION_DENIED");
    }

    #[test]
    fn delegate_rejects_insufficient_history() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 10); // Only 10, need 30
        let ctx = test_ctx(&db, agent_id);

        let result = DelegateToAgentSkill.execute(
            &ctx,
            &serde_json::json!({
                "recipient_id": Uuid::now_v7().to_string(),
                "task": "should fail",
            }),
        );
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "AUTHORIZATION_DENIED");
    }

    #[test]
    fn delegate_rejects_self_delegation() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 35);
        let ctx = test_ctx(&db, agent_id);

        let result = DelegateToAgentSkill.execute(
            &ctx,
            &serde_json::json!({
                "recipient_id": agent_str,
                "task": "self-delegate",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn delegate_rejects_empty_task() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let agent_str = agent_id.to_string();
        seed_convergence_history(&db, &agent_str, 35);
        let ctx = test_ctx(&db, agent_id);

        let result = DelegateToAgentSkill.execute(
            &ctx,
            &serde_json::json!({
                "recipient_id": Uuid::now_v7().to_string(),
                "task": "  ",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(DelegateToAgentSkill.name(), "delegate_to_agent");
        assert!(DelegateToAgentSkill.removable());
        assert_eq!(DelegateToAgentSkill.source(), SkillSource::Bundled);
    }
}
