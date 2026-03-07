//! `cancel_task` — cancel a delegated task.
//!
//! Transitions delegation state: Offered→Rejected or Accepted→Disputed.
//! Only the original sender can cancel.
//!
//! ## Input
//!
//! | Field           | Type   | Required | Description                  |
//! |-----------------|--------|----------|------------------------------|
//! | `delegation_id` | string | yes      | Delegation UUID to cancel    |
//! | `reason`        | string | no       | Reason for cancellation      |

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct CancelTaskSkill;

impl Skill for CancelTaskSkill {
    fn name(&self) -> &str {
        "cancel_task"
    }

    fn description(&self) -> &str {
        "Cancel a delegated task (Offered→Rejected or Accepted→Disputed)"
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

        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("cancelled by sender");

        // 1. Query current delegation state
        let delegation = cortex_storage::queries::delegation_state_queries::query_by_delegation_id(
            ctx.db,
            delegation_id,
        )
        .map_err(|e| SkillError::Storage(format!("query delegation: {e}")))?
        .ok_or_else(|| {
            SkillError::InvalidInput(format!("delegation '{delegation_id}' not found"))
        })?;

        // 2. Validate sender_id matches caller (authorization)
        let agent_id_str = ctx.agent_id.to_string();
        if delegation.sender_id != agent_id_str {
            return Err(SkillError::AuthorizationDenied(
                "only the delegating agent can cancel a task".into(),
            ));
        }

        // 3. Determine the target state based on current state
        let (new_state, dispute_reason) = match delegation.state.as_str() {
            "Offered" => ("Rejected", None),
            "Accepted" => ("Disputed", Some(reason)),
            "Completed" | "Disputed" | "Rejected" => {
                return Err(SkillError::DelegationFailed(format!(
                    "cannot cancel delegation in '{}' state (already resolved)",
                    delegation.state
                )));
            }
            other => {
                return Err(SkillError::DelegationFailed(format!(
                    "unexpected delegation state: '{other}'"
                )));
            }
        };

        // 4. Transition the delegation state
        let updated =
            cortex_storage::queries::delegation_state_queries::transition_by_delegation_id(
                ctx.db,
                delegation_id,
                new_state,
                None, // accept_message_id
                None, // complete_message_id
                None, // result
                dispute_reason,
            )
            .map_err(|e| SkillError::Storage(format!("transition delegation: {e}")))?;

        if !updated {
            return Err(SkillError::DelegationFailed(
                "failed to transition delegation state (immutability guard?)".into(),
            ));
        }

        // 5. Complete the convergence link (no longer active)
        let _ = cortex_storage::queries::convergence_propagation_queries::complete_link(
            ctx.db,
            delegation_id,
        );

        Ok(serde_json::json!({
            "delegation_id": delegation_id,
            "previous_state": delegation.state,
            "new_state": new_state,
            "reason": reason,
        }))
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let id = input.get("delegation_id").and_then(|v| v.as_str())?;
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("no reason given");
        Some(format!("Cancel delegation {id}: \"{reason}\""))
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
    fn cancel_offered_transitions_to_rejected() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let ctx = test_ctx(&db, agent_id);
        let delegation_id = Uuid::now_v7().to_string();

        seed_delegation(
            &db,
            &delegation_id,
            &agent_id.to_string(),
            &Uuid::now_v7().to_string(),
        );

        let result = CancelTaskSkill
            .execute(&ctx, &serde_json::json!({ "delegation_id": delegation_id }))
            .unwrap();

        assert_eq!(result["previous_state"], "Offered");
        assert_eq!(result["new_state"], "Rejected");
    }

    #[test]
    fn cancel_accepted_transitions_to_disputed() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let ctx = test_ctx(&db, agent_id);
        let delegation_id = Uuid::now_v7().to_string();
        let id = Uuid::now_v7().to_string();
        let offer_msg = Uuid::now_v7().to_string();
        let hash = vec![0u8; 32];

        // Insert as Offered, then transition to Accepted
        cortex_storage::queries::delegation_state_queries::insert_delegation(
            &db,
            &id,
            &delegation_id,
            &agent_id.to_string(),
            &Uuid::now_v7().to_string(),
            "test task",
            &offer_msg,
            &hash,
            &hash,
        )
        .unwrap();
        cortex_storage::queries::delegation_state_queries::transition(
            &db,
            &id,
            "Accepted",
            Some(&Uuid::now_v7().to_string()),
            None,
            None,
            None,
        )
        .unwrap();

        let result = CancelTaskSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "delegation_id": delegation_id,
                    "reason": "no longer needed",
                }),
            )
            .unwrap();

        assert_eq!(result["previous_state"], "Accepted");
        assert_eq!(result["new_state"], "Disputed");
        assert_eq!(result["reason"], "no longer needed");
    }

    #[test]
    fn cancel_completed_fails() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let ctx = test_ctx(&db, agent_id);
        let delegation_id = Uuid::now_v7().to_string();
        let id = Uuid::now_v7().to_string();
        let offer_msg = Uuid::now_v7().to_string();
        let hash = vec![0u8; 32];

        // Insert as Offered, transition to Accepted, then to Completed
        cortex_storage::queries::delegation_state_queries::insert_delegation(
            &db,
            &id,
            &delegation_id,
            &agent_id.to_string(),
            &Uuid::now_v7().to_string(),
            "test task",
            &offer_msg,
            &hash,
            &hash,
        )
        .unwrap();
        cortex_storage::queries::delegation_state_queries::transition(
            &db,
            &id,
            "Accepted",
            Some(&Uuid::now_v7().to_string()),
            None,
            None,
            None,
        )
        .unwrap();
        cortex_storage::queries::delegation_state_queries::transition(
            &db,
            &id,
            "Completed",
            None,
            Some(&Uuid::now_v7().to_string()),
            Some("done"),
            None,
        )
        .unwrap();

        let result =
            CancelTaskSkill.execute(&ctx, &serde_json::json!({ "delegation_id": delegation_id }));
        assert!(result.is_err());
    }

    #[test]
    fn cancel_requires_sender_authorization() {
        let db = test_db();
        let sender = Uuid::now_v7();
        let other_agent = Uuid::now_v7();
        let ctx = test_ctx(&db, other_agent);
        let delegation_id = Uuid::now_v7().to_string();

        seed_delegation(
            &db,
            &delegation_id,
            &sender.to_string(),
            &Uuid::now_v7().to_string(),
        );

        let result =
            CancelTaskSkill.execute(&ctx, &serde_json::json!({ "delegation_id": delegation_id }));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code(), "AUTHORIZATION_DENIED");
    }

    #[test]
    fn cancel_missing_delegation_id() {
        let db = test_db();
        let ctx = test_ctx(&db, Uuid::now_v7());

        let result = CancelTaskSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(CancelTaskSkill.name(), "cancel_task");
        assert!(CancelTaskSkill.removable());
        assert_eq!(CancelTaskSkill.source(), SkillSource::Bundled);
    }
}
