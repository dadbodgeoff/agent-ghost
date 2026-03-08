//! Migration v056: typed workflow execution contract with journal ownership.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if workflow_executions_is_canonical(conn)? {
        return Ok(());
    }

    conn.execute_batch(
        "ALTER TABLE workflow_executions RENAME TO workflow_executions_legacy_v056;

         CREATE TABLE workflow_executions (
            id                    TEXT PRIMARY KEY,
            workflow_id           TEXT,
            workflow_name         TEXT,
            journal_id            TEXT,
            operation_id          TEXT,
            owner_token           TEXT,
            lease_epoch           INTEGER,
            state_version         INTEGER NOT NULL DEFAULT 0 CHECK(state_version >= 0),
            status                TEXT NOT NULL DEFAULT 'recovery_required' CHECK(status IN ('running', 'completed', 'failed', 'recovery_required')),
            current_step_index    INTEGER,
            current_node_id       TEXT,
            recovery_action       TEXT,
            state                 TEXT NOT NULL DEFAULT '{}',
            final_response_status INTEGER,
            final_response_body   TEXT,
            started_at            TEXT NOT NULL DEFAULT (datetime('now')),
            completed_at          TEXT,
            updated_at            TEXT NOT NULL DEFAULT (datetime('now'))
         );

         CREATE UNIQUE INDEX idx_workflow_executions_journal_id
            ON workflow_executions(journal_id)
            WHERE journal_id IS NOT NULL;
         CREATE UNIQUE INDEX idx_workflow_executions_operation_id
            ON workflow_executions(operation_id)
            WHERE operation_id IS NOT NULL;
         CREATE INDEX idx_workflow_executions_workflow_status
            ON workflow_executions(workflow_id, status, updated_at);",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    conn.execute(
        "INSERT INTO workflow_executions (
            id,
            workflow_id,
            workflow_name,
            journal_id,
            operation_id,
            owner_token,
            lease_epoch,
            state_version,
            status,
            current_step_index,
            current_node_id,
            recovery_action,
            state,
            final_response_status,
            final_response_body,
            started_at,
            completed_at,
            updated_at
         )
         SELECT
            id,
            CASE WHEN json_valid(state) THEN json_extract(state, '$.workflow_id') END,
            CASE WHEN json_valid(state) THEN json_extract(state, '$.workflow_name') END,
            NULL,
            NULL,
            NULL,
            NULL,
            0,
            'recovery_required',
            NULL,
            NULL,
            'legacy_state_upgrade_required',
            state,
            NULL,
            NULL,
            COALESCE(
                CASE WHEN json_valid(state) THEN json_extract(state, '$.started_at') END,
                updated_at
            ),
            CASE WHEN json_valid(state) THEN json_extract(state, '$.completed_at') END,
            updated_at
         FROM workflow_executions_legacy_v056",
        [],
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    conn.execute_batch("DROP TABLE workflow_executions_legacy_v056;")
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}

fn workflow_executions_is_canonical(conn: &Connection) -> CortexResult<bool> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(workflow_executions)")
        .map_err(|error| to_storage_err(error.to_string()))?;
    let columns = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(columns.iter().any(|column| column == "state_version"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_legacy_workflow_execution_rows_to_recovery_required() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workflow_executions (
                id TEXT PRIMARY KEY,
                state TEXT NOT NULL DEFAULT '{}',
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );
            INSERT INTO workflow_executions (id, state, updated_at)
            VALUES (
                'exec-1',
                '{\"execution_id\":\"exec-1\",\"workflow_id\":\"wf-1\",\"workflow_name\":\"Legacy wf\",\"started_at\":\"2026-03-01T00:00:00Z\"}',
                '2026-03-01T00:00:00Z'
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let row: (String, i64, String, String, String) = conn
            .query_row(
                "SELECT workflow_id, state_version, status, recovery_action, started_at
                 FROM workflow_executions
                 WHERE id = 'exec-1'",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(row.0, "wf-1");
        assert_eq!(row.1, 0);
        assert_eq!(row.2, "recovery_required");
        assert_eq!(row.3, "legacy_state_upgrade_required");
        assert_eq!(row.4, "2026-03-01T00:00:00Z");
    }

    #[test]
    fn legacy_writer_shape_still_inserts_after_contract_upgrade() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE workflow_executions (
                id TEXT PRIMARY KEY,
                state TEXT NOT NULL DEFAULT '{}',
                updated_at TEXT NOT NULL DEFAULT (datetime('now'))
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();
        conn.execute(
            "INSERT INTO workflow_executions (id, state, updated_at)
             VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "exec-legacy",
                "{\"execution_id\":\"exec-legacy\"}",
                "2026-03-02T00:00:00Z"
            ],
        )
        .unwrap();

        let row: (i64, String) = conn
            .query_row(
                "SELECT state_version, status
                 FROM workflow_executions
                 WHERE id = 'exec-legacy'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();

        assert_eq!(row.0, 0);
        assert_eq!(row.1, "recovery_required");
    }
}
