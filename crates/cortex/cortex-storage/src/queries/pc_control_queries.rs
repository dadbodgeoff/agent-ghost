//! Queries for the `pc_control_actions` audit table (Phase 9).
//!
//! Every PC control action — executed or blocked — is logged here for
//! post-incident analysis and safety auditing.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

/// A row from the `pc_control_actions` table.
#[derive(Debug, Clone)]
pub struct PcControlActionRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub skill_name: String,
    pub action_type: String,
    pub input_json: String,
    pub result_json: String,
    pub target_app: Option<String>,
    pub coordinates: Option<String>,
    pub blocked: bool,
    pub block_reason: Option<String>,
    pub created_at: String,
}

/// Insert a PC control action record.
pub fn insert_action(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    skill_name: &str,
    action_type: &str,
    input_json: &str,
    result_json: &str,
    target_app: Option<&str>,
    coordinates: Option<&str>,
    blocked: bool,
    block_reason: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO pc_control_actions
         (id, agent_id, session_id, skill_name, action_type, input_json, result_json,
          target_app, coordinates, blocked, block_reason)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            agent_id,
            session_id,
            skill_name,
            action_type,
            input_json,
            result_json,
            target_app,
            coordinates,
            blocked as i32,
            block_reason,
        ],
    )
    .map_err(|e| to_storage_err(format!("insert pc_control_action: {e}")))?;
    Ok(())
}

/// List PC control actions for an agent, ordered by most recent first.
pub fn list_actions(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<PcControlActionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, skill_name, action_type,
                    input_json, result_json, target_app, coordinates,
                    blocked, block_reason, created_at
             FROM pc_control_actions
             WHERE agent_id = ?1
             ORDER BY created_at DESC
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| to_storage_err(format!("prepare list_actions: {e}")))?;

    let rows = stmt
        .query_map(params![agent_id, limit, offset], map_row)
        .map_err(|e| to_storage_err(format!("query list_actions: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(format!("collect list_actions: {e}")))?;

    Ok(rows)
}

/// List blocked PC control actions for an agent.
pub fn list_blocked_actions(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
) -> CortexResult<Vec<PcControlActionRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, skill_name, action_type,
                    input_json, result_json, target_app, coordinates,
                    blocked, block_reason, created_at
             FROM pc_control_actions
             WHERE agent_id = ?1 AND blocked = 1
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|e| to_storage_err(format!("prepare list_blocked: {e}")))?;

    let rows = stmt
        .query_map(params![agent_id, limit], map_row)
        .map_err(|e| to_storage_err(format!("query list_blocked: {e}")))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(format!("collect list_blocked: {e}")))?;

    Ok(rows)
}

/// Count actions in a session for a specific skill (for budget tracking).
pub fn count_actions_in_session(
    conn: &Connection,
    session_id: &str,
    skill_name: &str,
) -> CortexResult<u32> {
    conn.query_row(
        "SELECT COUNT(*) FROM pc_control_actions
         WHERE session_id = ?1 AND skill_name = ?2 AND blocked = 0",
        params![session_id, skill_name],
        |row| row.get(0),
    )
    .map_err(|e| to_storage_err(format!("count_actions_in_session: {e}")))
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PcControlActionRow> {
    Ok(PcControlActionRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        skill_name: row.get(3)?,
        action_type: row.get(4)?,
        input_json: row.get(5)?,
        result_json: row.get(6)?,
        target_app: row.get(7)?,
        coordinates: row.get(8)?,
        blocked: row.get::<_, i32>(9)? != 0,
        block_reason: row.get(10)?,
        created_at: row.get(11)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        crate::migrations::run_migrations(&db).unwrap();
        db
    }

    #[test]
    fn insert_and_list_actions() {
        let db = test_db();

        insert_action(
            &db,
            "action-1",
            "agent-1",
            "session-1",
            "mouse_click",
            "mouse_click",
            r#"{"x":100,"y":200}"#,
            r#"{"status":"ok"}"#,
            Some("Firefox"),
            Some("100,200"),
            false,
            None,
        )
        .unwrap();

        insert_action(
            &db,
            "action-2",
            "agent-1",
            "session-1",
            "mouse_click",
            "mouse_click",
            r#"{"x":300,"y":400}"#,
            "{}",
            Some("Terminal"),
            Some("300,400"),
            true,
            Some("App not in allowlist"),
        )
        .unwrap();

        let all = list_actions(&db, "agent-1", 50, 0).unwrap();
        assert_eq!(all.len(), 2);

        let blocked = list_blocked_actions(&db, "agent-1", 50).unwrap();
        assert_eq!(blocked.len(), 1);
        assert!(blocked[0].blocked);
        assert_eq!(
            blocked[0].block_reason.as_deref(),
            Some("App not in allowlist")
        );
    }

    #[test]
    fn count_actions_in_session() {
        let db = test_db();

        for i in 0..5 {
            insert_action(
                &db,
                &format!("a-{i}"),
                "agent-1",
                "session-1",
                "keyboard_type",
                "keyboard_type",
                "{}",
                "{}",
                None,
                None,
                false,
                None,
            )
            .unwrap();
        }

        // Add one blocked action (should not count).
        insert_action(
            &db,
            "a-blocked",
            "agent-1",
            "session-1",
            "keyboard_type",
            "keyboard_type",
            "{}",
            "{}",
            None,
            None,
            true,
            Some("blocked"),
        )
        .unwrap();

        let count = super::count_actions_in_session(&db, "session-1", "keyboard_type").unwrap();
        assert_eq!(count, 5);
    }

    #[test]
    fn different_sessions_counted_separately() {
        let db = test_db();

        insert_action(
            &db,
            "a-1",
            "agent-1",
            "s1",
            "mouse_move",
            "mouse_move",
            "{}",
            "{}",
            None,
            None,
            false,
            None,
        )
        .unwrap();
        insert_action(
            &db,
            "a-2",
            "agent-1",
            "s2",
            "mouse_move",
            "mouse_move",
            "{}",
            "{}",
            None,
            None,
            false,
            None,
        )
        .unwrap();

        assert_eq!(
            super::count_actions_in_session(&db, "s1", "mouse_move").unwrap(),
            1
        );
        assert_eq!(
            super::count_actions_in_session(&db, "s2", "mouse_move").unwrap(),
            1
        );
    }
}
