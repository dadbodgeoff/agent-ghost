//! `reflection_read` — agent reads its own past reflections.
//!
//! Queries the reflection entries stored by `reflection_write`,
//! filtered by session, chain, or recency. This allows agents to
//! review their self-reflections for meta-cognitive improvement.
//!
//! This is a **read-only, platform-managed** skill.

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that reads past agent reflections.
pub struct ReflectionReadSkill;

impl Skill for ReflectionReadSkill {
    fn name(&self) -> &str {
        "reflection_read"
    }

    fn description(&self) -> &str {
        "Read past self-reflections"
    }

    fn removable(&self) -> bool {
        false
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let agent_id_str = ctx.agent_id.to_string();

        // Determine query mode from input.
        let mode = input
            .get("mode")
            .and_then(|v| v.as_str())
            .unwrap_or("current_session");

        match mode {
            "current_session" => {
                self.read_session_reflections(ctx, &agent_id_str)
            }
            "by_session" => {
                let session_id = input
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| {
                        SkillError::InvalidInput(
                            "mode 'by_session' requires 'session_id' field".into(),
                        )
                    })?;
                self.read_reflections_by_session(ctx, &agent_id_str, session_id)
            }
            "recent" => {
                let limit = input
                    .get("limit")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(10) as u32;
                self.read_recent_reflections(ctx, &agent_id_str, limit)
            }
            other => Err(SkillError::InvalidInput(format!(
                "invalid mode '{other}', must be one of: \
                 current_session, by_session, recent"
            ))),
        }
    }
}

impl ReflectionReadSkill {
    /// Read reflections from the current session.
    fn read_session_reflections(
        &self,
        ctx: &SkillContext<'_>,
        _agent_id: &str,
    ) -> SkillResult {
        let session_id_str = ctx.session_id.to_string();

        let rows = cortex_storage::queries::reflection_queries::query_by_session(
            ctx.db,
            &session_id_str,
        )
        .map_err(|e| SkillError::Storage(format!("query session reflections: {e}")))?;

        let reflections = format_reflection_rows(&rows);

        Ok(serde_json::json!({
            "mode": "current_session",
            "session_id": session_id_str,
            "count": reflections.len(),
            "reflections": reflections,
        }))
    }

    /// Read reflections for a specific session.
    fn read_reflections_by_session(
        &self,
        ctx: &SkillContext<'_>,
        _agent_id: &str,
        session_id: &str,
    ) -> SkillResult {
        let rows = cortex_storage::queries::reflection_queries::query_by_session(
            ctx.db,
            session_id,
        )
        .map_err(|e| SkillError::Storage(format!("query session reflections: {e}")))?;

        let reflections = format_reflection_rows(&rows);

        Ok(serde_json::json!({
            "mode": "by_session",
            "session_id": session_id,
            "count": reflections.len(),
            "reflections": reflections,
        }))
    }

    /// Read the N most recent reflections across all sessions.
    fn read_recent_reflections(
        &self,
        ctx: &SkillContext<'_>,
        _agent_id: &str,
        limit: u32,
    ) -> SkillResult {
        // Query recent reflections across all sessions.
        // The reflection_entries table does not have an agent_id column;
        // reflections are scoped to sessions, not agents directly.
        let rows = query_recent_reflections(ctx.db, limit)?;

        let reflections = format_reflection_rows(&rows);

        Ok(serde_json::json!({
            "mode": "recent",
            "limit": limit,
            "count": reflections.len(),
            "reflections": reflections,
        }))
    }
}

/// Format reflection database rows into JSON for the response.
fn format_reflection_rows(
    rows: &[cortex_storage::queries::reflection_queries::ReflectionRow],
) -> Vec<serde_json::Value> {
    rows.iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "session_id": r.session_id,
                "chain_id": r.chain_id,
                "depth": r.depth,
                "trigger": r.trigger_type,
                "text": r.reflection_text,
                "self_reference_ratio": r.self_reference_ratio,
                "created_at": r.created_at,
            })
        })
        .collect()
}

/// Query the N most recent reflections across all sessions.
fn query_recent_reflections(
    db: &rusqlite::Connection,
    limit: u32,
) -> Result<Vec<cortex_storage::queries::reflection_queries::ReflectionRow>, SkillError> {
    let mut stmt = db
        .prepare(
            "SELECT id, session_id, chain_id, depth, trigger_type, \
                    reflection_text, self_reference_ratio, created_at \
             FROM reflection_entries \
             ORDER BY created_at DESC \
             LIMIT ?1",
        )
        .map_err(|e| SkillError::Storage(format!("prepare recent reflections: {e}")))?;

    let rows = stmt
        .query_map(rusqlite::params![limit], |row| {
            Ok(cortex_storage::queries::reflection_queries::ReflectionRow {
                id: row.get(0)?,
                session_id: row.get(1)?,
                chain_id: row.get(2)?,
                depth: row.get(3)?,
                trigger_type: row.get(4)?,
                reflection_text: row.get(5)?,
                self_reference_ratio: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| SkillError::Storage(format!("query recent reflections: {e}")))?;

    let mut result = Vec::new();
    for row in rows {
        match row {
            Ok(r) => result.push(r),
            Err(e) => {
                tracing::warn!(error = %e, "Skipping malformed reflection row");
            }
        }
    }

    Ok(result)
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

    fn insert_test_reflection(
        db: &rusqlite::Connection,
        _agent_id: &str,
        session_id: &str,
        text: &str,
    ) {
        let reflection_id = Uuid::now_v7().to_string();
        let chain_id = Uuid::now_v7().to_string();
        cortex_storage::queries::reflection_queries::insert_reflection(
            db,
            &reflection_id,
            session_id,
            &chain_id,
            0,
            "Scheduled",
            text,
            0.1,
            &[0u8; 32],
            &[0u8; 32],
        )
        .unwrap();
    }

    #[test]
    fn reads_current_session_reflections() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        insert_test_reflection(
            &db,
            &agent_id.to_string(),
            &session_id.to_string(),
            "First reflection on task progress.",
        );
        insert_test_reflection(
            &db,
            &agent_id.to_string(),
            &session_id.to_string(),
            "Second reflection on approach quality.",
        );

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id,
            convergence_profile: "standard",
        };

        let result = ReflectionReadSkill.execute(
            &ctx,
            &serde_json::json!({"mode": "current_session"}),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["mode"], "current_session");
        assert_eq!(val["count"], 2);

        let reflections = val["reflections"].as_array().unwrap();
        assert_eq!(reflections.len(), 2);
    }

    #[test]
    fn reads_by_specific_session() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session1 = Uuid::now_v7();
        let session2 = Uuid::now_v7();

        insert_test_reflection(
            &db,
            &agent_id.to_string(),
            &session1.to_string(),
            "Session 1 reflection.",
        );
        insert_test_reflection(
            &db,
            &agent_id.to_string(),
            &session2.to_string(),
            "Session 2 reflection.",
        );

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id: Uuid::now_v7(), // Different from both
            convergence_profile: "standard",
        };

        let result = ReflectionReadSkill.execute(
            &ctx,
            &serde_json::json!({
                "mode": "by_session",
                "session_id": session1.to_string(),
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["count"], 1);
    }

    #[test]
    fn reads_recent_across_sessions() {
        let db = test_db();
        let agent_id = Uuid::now_v7();

        for i in 0..5 {
            let session = Uuid::now_v7();
            insert_test_reflection(
                &db,
                &agent_id.to_string(),
                &session.to_string(),
                &format!("Reflection {i}"),
            );
        }

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        };

        let result = ReflectionReadSkill.execute(
            &ctx,
            &serde_json::json!({"mode": "recent", "limit": 3}),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["count"], 3);
    }

    #[test]
    fn by_session_requires_session_id() {
        let db = test_db();
        let ctx = SkillContext {
            db: &db,
            agent_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        };

        let result = ReflectionReadSkill.execute(
            &ctx,
            &serde_json::json!({"mode": "by_session"}),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("session_id"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn invalid_mode_returns_error() {
        let db = test_db();
        let ctx = SkillContext {
            db: &db,
            agent_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        };

        let result = ReflectionReadSkill.execute(
            &ctx,
            &serde_json::json!({"mode": "invalid_mode"}),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_is_not_removable() {
        assert!(!ReflectionReadSkill.removable());
    }
}
