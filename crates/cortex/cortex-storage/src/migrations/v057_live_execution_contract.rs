//! v057: typed/versioned contract for live_execution_records.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if live_execution_records_is_canonical(conn)? {
        return Ok(());
    }

    conn.execute_batch(
        "ALTER TABLE live_execution_records RENAME TO live_execution_records_legacy_v057;
         DROP INDEX IF EXISTS idx_live_execution_route_status;
         DROP INDEX IF EXISTS idx_live_execution_actor_operation;

         CREATE TABLE live_execution_records (
            id            TEXT PRIMARY KEY,
            journal_id    TEXT NOT NULL UNIQUE,
            operation_id  TEXT NOT NULL UNIQUE,
            route_kind    TEXT NOT NULL,
            actor_key     TEXT NOT NULL,
            state_version INTEGER NOT NULL DEFAULT 0 CHECK(state_version >= 0),
            status        TEXT NOT NULL,
            state_json    TEXT NOT NULL DEFAULT '{}',
            created_at    TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
            CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required'))
         );

         CREATE INDEX idx_live_execution_route_status
            ON live_execution_records(route_kind, status);
         CREATE INDEX idx_live_execution_actor_operation
            ON live_execution_records(actor_key, operation_id);",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    conn.execute_batch(
        "INSERT INTO live_execution_records (
            id,
            journal_id,
            operation_id,
            route_kind,
            actor_key,
            state_version,
            status,
            state_json,
            created_at,
            updated_at
         )
         SELECT
            id,
            journal_id,
            operation_id,
            route_kind,
            actor_key,
            0,
            status,
            state_json,
            created_at,
            updated_at
         FROM live_execution_records_legacy_v057",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    conn.execute_batch("DROP TABLE live_execution_records_legacy_v057;")
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}

fn live_execution_records_is_canonical(conn: &Connection) -> CortexResult<bool> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(live_execution_records)")
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
    fn migrates_legacy_live_execution_rows_to_version_zero() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE live_execution_records (
                id           TEXT PRIMARY KEY,
                journal_id   TEXT NOT NULL UNIQUE,
                operation_id TEXT NOT NULL UNIQUE,
                route_kind   TEXT NOT NULL,
                actor_key    TEXT NOT NULL,
                status       TEXT NOT NULL,
                state_json   TEXT NOT NULL DEFAULT '{}',
                created_at   TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
                CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required'))
            );
            INSERT INTO live_execution_records (
                id, journal_id, operation_id, route_kind, actor_key, status, state_json
            ) VALUES (
                'exec-1', 'journal-1', 'op-1', 'agent_chat', 'actor-1', 'accepted', '{\"legacy\":true}'
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let row = conn
            .query_row(
                "SELECT state_version, status, state_json
                 FROM live_execution_records
                 WHERE id = 'exec-1'",
                [],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(row.0, 0);
        assert_eq!(row.1, "accepted");
        assert_eq!(row.2, "{\"legacy\":true}");
    }

    #[test]
    fn legacy_writer_shape_still_inserts_after_contract_upgrade() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE live_execution_records (
                id           TEXT PRIMARY KEY,
                journal_id   TEXT NOT NULL UNIQUE,
                operation_id TEXT NOT NULL UNIQUE,
                route_kind   TEXT NOT NULL,
                actor_key    TEXT NOT NULL,
                status       TEXT NOT NULL,
                state_json   TEXT NOT NULL DEFAULT '{}',
                created_at   TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at   TEXT NOT NULL DEFAULT (datetime('now')),
                CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required'))
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();

        conn.execute(
            "INSERT INTO live_execution_records (
                id, journal_id, operation_id, route_kind, actor_key, status, state_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "exec-2",
                "journal-2",
                "op-2",
                "oauth_execute_api_call",
                "actor-2",
                "accepted",
                "{}",
            ],
        )
        .unwrap();

        let row = conn
            .query_row(
                "SELECT state_version, status
                 FROM live_execution_records
                 WHERE id = 'exec-2'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, 0);
        assert_eq!(row.1, "accepted");
    }
}
