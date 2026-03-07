//! `email_draft` — draft emails (never auto-send at any autonomy level).
//!
//! Creates structured email drafts stored in the database. The skill
//! NEVER sends emails directly — that is a deliberate safety constraint
//! that cannot be overridden by autonomy level.
//!
//! ## Input
//!
//! | Field       | Type     | Required | Default    | Description             |
//! |-------------|----------|----------|------------|-------------------------|
//! | `action`    | string   | yes      | —          | "draft", "list", "read", "delete" |
//! | `to`        | string[] | draft    | —          | Recipient addresses     |
//! | `subject`   | string   | draft    | —          | Email subject           |
//! | `body`      | string   | draft    | —          | Email body              |
//! | `cc`        | string[] | no       | `[]`       | CC recipients           |
//! | `draft_id`  | string   | read/delete | —       | Draft note ID           |
//! | `limit`     | int      | list     | 50         | Max results             |

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct EmailDraftSkill;

impl Skill for EmailDraftSkill {
    fn name(&self) -> &str {
        "email_draft"
    }

    fn description(&self) -> &str {
        "Draft emails (never auto-send). Supports draft, list, read, delete"
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
                    "missing required field 'action' (draft|list|read|delete)".into(),
                )
            })?;

        let agent_id_str = ctx.agent_id.to_string();
        let session_id_str = ctx.session_id.to_string();

        match action {
            "draft" => {
                let to = input.get("to").ok_or_else(|| {
                    SkillError::InvalidInput("missing required field 'to' (array of emails)".into())
                })?;
                let subject = input
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'subject'".into())
                    })?;
                let body = input.get("body").and_then(|v| v.as_str()).ok_or_else(|| {
                    SkillError::InvalidInput("missing required field 'body'".into())
                })?;
                let empty_cc = serde_json::json!([]);
                let cc = input.get("cc").unwrap_or(&empty_cc);

                // Validate recipients.
                let recipients = to.as_array().ok_or_else(|| {
                    SkillError::InvalidInput("'to' must be an array of email addresses".into())
                })?;
                if recipients.is_empty() {
                    return Err(SkillError::InvalidInput(
                        "'to' must contain at least one recipient".into(),
                    ));
                }
                for r in recipients {
                    let addr = r.as_str().ok_or_else(|| {
                        SkillError::InvalidInput("each recipient must be a string".into())
                    })?;
                    if !addr.contains('@') || !addr.contains('.') {
                        return Err(SkillError::InvalidInput(format!(
                            "invalid email address: '{addr}'"
                        )));
                    }
                }

                if subject.trim().is_empty() {
                    return Err(SkillError::InvalidInput("subject must not be empty".into()));
                }

                // Store draft as a structured note with special tag.
                let draft_content = serde_json::json!({
                    "type": "email_draft",
                    "to": to,
                    "cc": cc,
                    "subject": subject,
                    "body": body,
                });

                let draft_id = Uuid::now_v7().to_string();
                let tags = serde_json::json!(["email_draft"]).to_string();
                cortex_storage::queries::note_queries::insert_note(
                    ctx.db,
                    &draft_id,
                    &agent_id_str,
                    &session_id_str,
                    &format!("Draft: {subject}"),
                    &draft_content.to_string(),
                    &tags,
                )
                .map_err(|e| SkillError::Storage(format!("save draft: {e}")))?;

                Ok(serde_json::json!({
                    "status": "drafted",
                    "draft_id": draft_id,
                    "subject": subject,
                    "to": to,
                    "note": "This draft has NOT been sent. Review and send manually.",
                }))
            }
            "list" => {
                let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as u32;

                let notes = cortex_storage::queries::note_queries::search_notes(
                    ctx.db,
                    &agent_id_str,
                    "email_draft",
                    limit,
                )
                .map_err(|e| SkillError::Storage(format!("list drafts: {e}")))?;

                let drafts: Vec<serde_json::Value> = notes
                    .iter()
                    .map(|n| {
                        let content: serde_json::Value =
                            serde_json::from_str(&n.content).unwrap_or(serde_json::json!({}));
                        serde_json::json!({
                            "draft_id": n.id,
                            "subject": content.get("subject").and_then(|v| v.as_str()).unwrap_or("?"),
                            "to": content.get("to"),
                            "created_at": n.created_at,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "drafts": drafts,
                    "count": drafts.len(),
                }))
            }
            "read" => {
                let draft_id = input
                    .get("draft_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'draft_id'".into())
                    })?;

                let note = cortex_storage::queries::note_queries::get_note(
                    ctx.db,
                    draft_id,
                    &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("read draft: {e}")))?
                .ok_or_else(|| SkillError::InvalidInput(format!("draft '{draft_id}' not found")))?;

                let content: serde_json::Value =
                    serde_json::from_str(&note.content).unwrap_or(serde_json::json!({}));

                Ok(serde_json::json!({
                    "draft_id": note.id,
                    "to": content.get("to"),
                    "cc": content.get("cc"),
                    "subject": content.get("subject"),
                    "body": content.get("body"),
                    "created_at": note.created_at,
                }))
            }
            "delete" => {
                let draft_id = input
                    .get("draft_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'draft_id'".into())
                    })?;

                let deleted = cortex_storage::queries::note_queries::delete_note(
                    ctx.db,
                    draft_id,
                    &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("delete draft: {e}")))?;

                if !deleted {
                    return Err(SkillError::InvalidInput(format!(
                        "draft '{draft_id}' not found or not owned by this agent"
                    )));
                }

                Ok(serde_json::json!({
                    "status": "deleted",
                    "draft_id": draft_id,
                }))
            }
            other => Err(SkillError::InvalidInput(format!(
                "unknown action '{other}', must be one of: draft, list, read, delete"
            ))),
        }
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let action = input.get("action").and_then(|v| v.as_str())?;
        match action {
            "draft" => {
                let subject = input
                    .get("subject")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(no subject)");
                let to = input
                    .get("to")
                    .and_then(|v| v.as_array())
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "?".into());
                Some(format!(
                    "Draft email to [{to}]: \"{subject}\" (will NOT be sent)"
                ))
            }
            "delete" => {
                let id = input
                    .get("draft_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                Some(format!("Delete email draft: {id}"))
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
    fn draft_and_read_email() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = EmailDraftSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "draft",
                    "to": ["alice@example.com"],
                    "subject": "Meeting tomorrow",
                    "body": "Hi Alice, can we meet at 10am?",
                }),
            )
            .unwrap();
        assert_eq!(result["status"], "drafted");
        assert!(result["note"].as_str().unwrap().contains("NOT been sent"));

        let draft_id = result["draft_id"].as_str().unwrap();
        let read = EmailDraftSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "read",
                    "draft_id": draft_id,
                }),
            )
            .unwrap();
        assert_eq!(read["subject"], "Meeting tomorrow");
    }

    #[test]
    fn list_and_delete_drafts() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = EmailDraftSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "draft",
                    "to": ["bob@example.com"],
                    "subject": "Hello",
                    "body": "Hi Bob!",
                }),
            )
            .unwrap();
        let draft_id = result["draft_id"].as_str().unwrap();

        let list = EmailDraftSkill
            .execute(&ctx, &serde_json::json!({"action": "list"}))
            .unwrap();
        assert_eq!(list["count"], 1);

        EmailDraftSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "delete",
                    "draft_id": draft_id,
                }),
            )
            .unwrap();

        let list2 = EmailDraftSkill
            .execute(&ctx, &serde_json::json!({"action": "list"}))
            .unwrap();
        assert_eq!(list2["count"], 0);
    }

    #[test]
    fn rejects_invalid_email() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = EmailDraftSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "draft",
                "to": ["not-an-email"],
                "subject": "Test",
                "body": "Test body",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_empty_recipients() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = EmailDraftSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "draft",
                "to": [],
                "subject": "Test",
                "body": "Test body",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(EmailDraftSkill.name(), "email_draft");
        assert!(EmailDraftSkill.removable());
        assert_eq!(EmailDraftSkill.source(), SkillSource::Bundled);
    }
}
