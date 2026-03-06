//! `note_take` — structured note-taking to database.
//!
//! Supports create, read, update, delete, list, and search operations.
//!
//! ## Input
//!
//! | Field     | Type   | Required | Default       | Description                       |
//! |-----------|--------|----------|---------------|-----------------------------------|
//! | `action`  | string | yes      | —             | "create", "read", "update", "delete", "list", "search" |
//! | `title`   | string | create/update | —         | Note title                         |
//! | `content` | string | create/update | —         | Note body                          |
//! | `tags`    | array  | no       | `[]`          | Tags for categorization            |
//! | `note_id` | string | read/update/delete | —    | Note UUID                          |
//! | `query`   | string | search   | —             | Search term                        |
//! | `limit`   | int    | list/search | 50         | Max results                        |
//! | `offset`  | int    | list     | 0             | Pagination offset                  |

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct NoteTakeSkill;

impl Skill for NoteTakeSkill {
    fn name(&self) -> &str {
        "note_take"
    }

    fn description(&self) -> &str {
        "Create, read, update, delete, list, and search structured notes"
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
                    "missing required field 'action' (create|read|update|delete|list|search)"
                        .into(),
                )
            })?;

        let agent_id_str = ctx.agent_id.to_string();
        let session_id_str = ctx.session_id.to_string();

        match action {
            "create" => {
                let title = input
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'title'".into())
                    })?;
                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'content'".into())
                    })?;
                let tags = input
                    .get("tags")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "[]".to_string());

                if title.trim().is_empty() {
                    return Err(SkillError::InvalidInput(
                        "title must not be empty".into(),
                    ));
                }

                let note_id = Uuid::now_v7().to_string();
                cortex_storage::queries::note_queries::insert_note(
                    ctx.db,
                    &note_id,
                    &agent_id_str,
                    &session_id_str,
                    title,
                    content,
                    &tags,
                )
                .map_err(|e| SkillError::Storage(format!("insert note: {e}")))?;

                Ok(serde_json::json!({
                    "status": "created",
                    "note_id": note_id,
                    "title": title,
                }))
            }
            "read" => {
                let note_id = input
                    .get("note_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'note_id'".into())
                    })?;

                let note = cortex_storage::queries::note_queries::get_note(
                    ctx.db, note_id, &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("get note: {e}")))?
                .ok_or_else(|| {
                    SkillError::InvalidInput(format!("note '{note_id}' not found"))
                })?;

                let tags: serde_json::Value =
                    serde_json::from_str(&note.tags).unwrap_or(serde_json::json!([]));

                Ok(serde_json::json!({
                    "note_id": note.id,
                    "title": note.title,
                    "content": note.content,
                    "tags": tags,
                    "created_at": note.created_at,
                    "updated_at": note.updated_at,
                }))
            }
            "update" => {
                let note_id = input
                    .get("note_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'note_id'".into())
                    })?;
                let title = input
                    .get("title")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'title'".into())
                    })?;
                let content = input
                    .get("content")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'content'".into())
                    })?;
                let tags = input
                    .get("tags")
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "[]".to_string());

                let updated = cortex_storage::queries::note_queries::update_note(
                    ctx.db,
                    note_id,
                    &agent_id_str,
                    title,
                    content,
                    &tags,
                )
                .map_err(|e| SkillError::Storage(format!("update note: {e}")))?;

                if !updated {
                    return Err(SkillError::InvalidInput(format!(
                        "note '{note_id}' not found or not owned by this agent"
                    )));
                }

                Ok(serde_json::json!({
                    "status": "updated",
                    "note_id": note_id,
                }))
            }
            "delete" => {
                let note_id = input
                    .get("note_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'note_id'".into())
                    })?;

                let deleted = cortex_storage::queries::note_queries::delete_note(
                    ctx.db, note_id, &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("delete note: {e}")))?;

                if !deleted {
                    return Err(SkillError::InvalidInput(format!(
                        "note '{note_id}' not found or not owned by this agent"
                    )));
                }

                Ok(serde_json::json!({
                    "status": "deleted",
                    "note_id": note_id,
                }))
            }
            "list" => {
                let limit = input
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(50) as u32;
                let offset = input
                    .get("offset")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;

                let notes = cortex_storage::queries::note_queries::list_notes(
                    ctx.db,
                    &agent_id_str,
                    limit,
                    offset,
                )
                .map_err(|e| SkillError::Storage(format!("list notes: {e}")))?;

                let total = cortex_storage::queries::note_queries::count_notes(
                    ctx.db, &agent_id_str,
                )
                .map_err(|e| SkillError::Storage(format!("count notes: {e}")))?;

                let entries: Vec<serde_json::Value> = notes
                    .iter()
                    .map(|n| {
                        let tags: serde_json::Value =
                            serde_json::from_str(&n.tags).unwrap_or(serde_json::json!([]));
                        serde_json::json!({
                            "note_id": n.id,
                            "title": n.title,
                            "tags": tags,
                            "created_at": n.created_at,
                            "updated_at": n.updated_at,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "notes": entries,
                    "total": total,
                    "limit": limit,
                    "offset": offset,
                }))
            }
            "search" => {
                let query = input
                    .get("query")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput("missing required field 'query'".into())
                    })?;
                let limit = input
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(20) as u32;

                let notes = cortex_storage::queries::note_queries::search_notes(
                    ctx.db,
                    &agent_id_str,
                    query,
                    limit,
                )
                .map_err(|e| SkillError::Storage(format!("search notes: {e}")))?;

                let entries: Vec<serde_json::Value> = notes
                    .iter()
                    .map(|n| {
                        let tags: serde_json::Value =
                            serde_json::from_str(&n.tags).unwrap_or(serde_json::json!([]));
                        serde_json::json!({
                            "note_id": n.id,
                            "title": n.title,
                            "tags": tags,
                            "updated_at": n.updated_at,
                        })
                    })
                    .collect();

                Ok(serde_json::json!({
                    "results": entries,
                    "query": query,
                    "count": entries.len(),
                }))
            }
            other => Err(SkillError::InvalidInput(format!(
                "unknown action '{other}', must be one of: create, read, update, delete, list, search"
            ))),
        }
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let action = input.get("action").and_then(|v| v.as_str())?;
        let title = input.get("title").and_then(|v| v.as_str()).unwrap_or("(untitled)");
        match action {
            "create" => Some(format!("Create note: \"{title}\"")),
            "update" => Some(format!("Update note: \"{title}\"")),
            "delete" => {
                let id = input.get("note_id").and_then(|v| v.as_str()).unwrap_or("?");
                Some(format!("Delete note: {id}"))
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
    fn create_and_read_note() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "create",
                    "title": "Test Note",
                    "content": "Hello, world!",
                    "tags": ["test", "example"],
                }),
            )
            .unwrap();

        assert_eq!(result["status"], "created");
        let note_id = result["note_id"].as_str().unwrap();

        let read = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "read",
                    "note_id": note_id,
                }),
            )
            .unwrap();

        assert_eq!(read["title"], "Test Note");
        assert_eq!(read["content"], "Hello, world!");
    }

    #[test]
    fn update_note() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "create",
                    "title": "Original",
                    "content": "Original content",
                }),
            )
            .unwrap();
        let note_id = result["note_id"].as_str().unwrap();

        let update = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "update",
                    "note_id": note_id,
                    "title": "Updated",
                    "content": "Updated content",
                }),
            )
            .unwrap();
        assert_eq!(update["status"], "updated");

        let read = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "read",
                    "note_id": note_id,
                }),
            )
            .unwrap();
        assert_eq!(read["title"], "Updated");
    }

    #[test]
    fn delete_note() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "create",
                    "title": "To Delete",
                    "content": "Will be deleted",
                }),
            )
            .unwrap();
        let note_id = result["note_id"].as_str().unwrap();

        let del = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "delete",
                    "note_id": note_id,
                }),
            )
            .unwrap();
        assert_eq!(del["status"], "deleted");

        let read = NoteTakeSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "read",
                "note_id": note_id,
            }),
        );
        assert!(read.is_err());
    }

    #[test]
    fn list_and_search_notes() {
        let db = test_db();
        let ctx = test_ctx(&db);

        for i in 0..3 {
            NoteTakeSkill
                .execute(
                    &ctx,
                    &serde_json::json!({
                        "action": "create",
                        "title": format!("Note {i}"),
                        "content": format!("Content for note {i}"),
                        "tags": ["batch"],
                    }),
                )
                .unwrap();
        }

        let list = NoteTakeSkill
            .execute(&ctx, &serde_json::json!({"action": "list"}))
            .unwrap();
        assert_eq!(list["total"], 3);

        let search = NoteTakeSkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "action": "search",
                    "query": "Note 1",
                }),
            )
            .unwrap();
        assert!(search["count"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn rejects_empty_title() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = NoteTakeSkill.execute(
            &ctx,
            &serde_json::json!({
                "action": "create",
                "title": "  ",
                "content": "test",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn rejects_unknown_action() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = NoteTakeSkill.execute(
            &ctx,
            &serde_json::json!({"action": "explode"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(NoteTakeSkill.name(), "note_take");
        assert!(NoteTakeSkill.removable());
        assert_eq!(NoteTakeSkill.source(), SkillSource::Bundled);
    }
}
