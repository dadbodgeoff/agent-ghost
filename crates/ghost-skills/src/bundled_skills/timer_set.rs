//! `timer_set` — set and query reminders/timers.
//!
//! Timers are stored in the database. Supports set, list, check (due),
//! and cancel operations.
//!
//! ## Input
//!
//! | Field      | Type   | Required | Default    | Description                    |
//! |------------|--------|----------|------------|--------------------------------|
//! | `action`   | string | yes      | —          | "set", "list", "check", "cancel" |
//! | `label`    | string | set      | —          | Timer description              |
//! | `fire_at`  | string | set      | —          | ISO 8601 datetime to fire      |
//! | `timer_id` | string | cancel   | —          | Timer UUID to cancel           |
//! | `status`   | string | list     | all        | Filter: "pending", "fired", "cancelled" |
//! | `limit`    | int    | list     | 50         | Max results                    |

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct TimerSetSkill;

impl Skill for TimerSetSkill {
    fn name(&self) -> &str {
        "timer_set"
    }

    fn description(&self) -> &str {
        "Set, list, check, and cancel reminders/timers"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput(
                    "missing required field 'action' (set|list|check|cancel)".into(),
                )
            })?;

        let agent_id_str = ctx.agent_id.to_string();
        let session_id_str = ctx.session_id.to_string();

        match action {
            "set" => {
                let label = input
                    .get("label")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'label'".into())
                    })?;
                let fire_at = input
                    .get("fire_at")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput(
                            "missing required field 'fire_at' (ISO 8601 datetime)".into(),
                        )
                    })?;

                if label.trim().is_empty() {
                    return Err(SkillError::InvalidInput(
                        "label must not be empty".into(),
                    ));
                }

                // Validate ISO 8601 format (basic check).
                if chrono::DateTime::parse_from_rfc3339(fire_at).is_err()
                    && chrono::NaiveDateTime::parse_from_str(fire_at, "%Y-%m-%dT%H:%M:%S").is_err()
                    && chrono::NaiveDateTime::parse_from_str(fire_at, "%Y-%m-%d %H:%M:%S").is_err()
                {
                    return Err(SkillError::InvalidInput(format!(
                        "invalid fire_at format: '{fire_at}'. Expected ISO 8601 \
                         (e.g., '2026-03-01T15:00:00Z' or '2026-03-01 15:00:00')"
                    )));
                }

                let timer_id = Uuid::now_v7().to_string();
                cortex_storage::queries::timer_queries::insert_timer(
                    ctx.db,
                    &timer_id,
                    &agent_id_str,
                    &session_id_str,
                    label,
                    fire_at,
                )
                .map_err(|e| SkillError::Storage(format!("insert timer: {e}")))?;

                Ok(serde_json::json!({
                    "status": "set",
                    "timer_id": timer_id,
                    "label": label,
                    "fire_at": fire_at,
                }))
            }
            "list" => {
                let status_filter = input.get("status").and_then(|v| v.as_str());
                let limit = input
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50) as u32;

                let timers = cortex_storage::queries::timer_queries::list_timers(
                    ctx.db,
                    &agent_id_str,
                    status_filter,
                    limit,
                )
                .map_err(|e| SkillError::Storage(format!("list timers: {e}")))?;

                let entries: Vec<serde_json::Value> = timers
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "timer_id": t.id,
                            "label": t.label,
                            "fire_at": t.fire_at,
                            "status": t.status,
                            "created_at": t.created_at,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "timers": entries,
                    "count": entries.len(),
                }))
            }
            "check" => {
                let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
                let due = cortex_storage::queries::timer_queries::pending_due(
                    ctx.db,
                    &agent_id_str,
                    &now,
                )
                .map_err(|e| SkillError::Storage(format!("check timers: {e}")))?;

                // Mark due timers as fired.
                for timer in &due {
                    let _ = cortex_storage::queries::timer_queries::fire_timer(
                        ctx.db,
                        &timer.id,
                        &agent_id_str,
                    );
                }

                let entries: Vec<serde_json::Value> = due
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "timer_id": t.id,
                            "label": t.label,
                            "fire_at": t.fire_at,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "due": entries,
                    "count": entries.len(),
                    "checked_at": now,
                }))
            }
            "cancel" => {
                let timer_id = input
                    .get("timer_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'timer_id'".into())
                    })?;

                let cancelled = cortex_storage::queries::timer_queries::cancel_timer(
                    ctx.db,
                    timer_id,
                    &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("cancel timer: {e}")))?;

                if !cancelled {
                    return Err(SkillError::InvalidInput(format!(
                        "timer '{timer_id}' not found, not pending, or not owned by this agent"
                    )));
                }

                Ok(serde_json::json!({
                    "status": "cancelled",
                    "timer_id": timer_id,
                }))
            }
            other => Err(SkillError::InvalidInput(format!(
                "unknown action '{other}', must be one of: set, list, check, cancel"
            ))),
        }
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let action = input.get("action").and_then(|v| v.as_str())?;
        match action {
            "set" => {
                let label = input.get("label").and_then(|v| v.as_str()).unwrap_or("?");
                let fire_at = input.get("fire_at").and_then(|v| v.as_str()).unwrap_or("?");
                Some(format!("Set timer: \"{label}\" at {fire_at}"))
            }
            "cancel" => {
                let id = input.get("timer_id").and_then(|v| v.as_str()).unwrap_or("?");
                Some(format!("Cancel timer: {id}"))
            }
            _ => None,
        }
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

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        }
    }

    #[test]
    fn set_and_list_timer() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = TimerSetSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "set",
                    "label": "Stand up",
                    "fire_at": "2026-12-31T23:59:59Z",
                }),
            )
            .unwrap();
        assert_eq!(result["status"], "set");

        let list = TimerSetSkill
            .execute(&ctx, &serde_json::json!({"action": "list"}))
            .unwrap();
        assert_eq!(list["count"], 1);
        assert_eq!(list["timers"][0]["label"], "Stand up");
    }

    #[test]
    fn cancel_timer() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = TimerSetSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "set",
                    "label": "To cancel",
                    "fire_at": "2026-12-31T23:59:59Z",
                }),
            )
            .unwrap();
        let timer_id = result["timer_id"].as_str().unwrap();

        let cancel = TimerSetSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "cancel",
                    "timer_id": timer_id,
                }),
            )
            .unwrap();
        assert_eq!(cancel["status"], "cancelled");

        // Cannot cancel again.
        let again = TimerSetSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "cancel",
                "timer_id": timer_id,
            }),
        );
        assert!(again.is_err());
    }

    #[test]
    fn check_fires_due_timers() {
        let db = test_db();
        let ctx = test_ctx(&db);

        // Set a timer in the past.
        TimerSetSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "set",
                    "label": "Already due",
                    "fire_at": "2020-01-01T00:00:00Z",
                }),
            )
            .unwrap();

        let check = TimerSetSkill
            .execute(&ctx, &serde_json::json!({"action": "check"}))
            .unwrap();
        assert_eq!(check["count"], 1);
        assert_eq!(check["due"][0]["label"], "Already due");

        // Check again — should be empty (already fired).
        let check2 = TimerSetSkill
            .execute(&ctx, &serde_json::json!({"action": "check"}))
            .unwrap();
        assert_eq!(check2["count"], 0);
    }

    #[test]
    fn rejects_invalid_fire_at() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = TimerSetSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "set",
                "label": "Bad time",
                "fire_at": "not-a-date",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(TimerSetSkill.name(), "timer_set");
        assert!(TimerSetSkill.removable());
        assert_eq!(TimerSetSkill.source(), SkillSource::Bundled);
    }
}
