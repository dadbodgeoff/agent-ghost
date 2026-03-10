//! v062: allow cancelled status in live_execution_records.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if live_execution_records_allows_cancelled(conn)? {
        return Ok(());
    }

    conn.execute_batch(
        "ALTER TABLE live_execution_records RENAME TO live_execution_records_legacy_v062;
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
            CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required', 'cancelled'))
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
            state_version,
            status,
            state_json,
            created_at,
            updated_at
         FROM live_execution_records_legacy_v062",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    conn.execute_batch("DROP TABLE live_execution_records_legacy_v062;")
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}

fn live_execution_records_allows_cancelled(conn: &Connection) -> CortexResult<bool> {
    let sql = conn
        .query_row(
            "SELECT sql
             FROM sqlite_master
             WHERE type = 'table' AND name = 'live_execution_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(sql.contains("'cancelled'"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_live_execution_table_to_allow_cancelled_status() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE live_execution_records (
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
                ON live_execution_records(actor_key, operation_id);
            INSERT INTO live_execution_records (
                id, journal_id, operation_id, route_kind, actor_key, state_version, status, state_json
            ) VALUES (
                'exec-1', 'journal-1', 'op-1', 'studio_send_message_stream', 'actor-1', 1, 'running', '{}'
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();

        conn.execute(
            "UPDATE live_execution_records
             SET status = 'cancelled'
             WHERE id = 'exec-1'",
            [],
        )
        .unwrap();

        let status = conn
            .query_row(
                "SELECT status FROM live_execution_records WHERE id = 'exec-1'",
                [],
                |row| row.get::<_, String>(0),
            )
            .unwrap();
        assert_eq!(status, "cancelled");
    }
}
