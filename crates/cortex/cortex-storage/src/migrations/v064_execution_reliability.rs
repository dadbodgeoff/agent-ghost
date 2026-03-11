//! v064: strengthen live execution semantics and add attempts/steps tables.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

const CANONICAL_STATUS_CHECK: &str = "'accepted', 'preparing', 'running', 'cancel_requested', 'cancelled', 'completed', 'recovery_required', 'needs_review', 'failed'";

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if !live_execution_records_is_v064(conn)? {
        conn.execute_batch(&format!(
            "ALTER TABLE live_execution_records RENAME TO live_execution_records_legacy_v064;
             DROP INDEX IF EXISTS idx_live_execution_route_status;
             DROP INDEX IF EXISTS idx_live_execution_actor_operation;

             CREATE TABLE live_execution_records (
                id            TEXT PRIMARY KEY,
                journal_id    TEXT NOT NULL UNIQUE,
                operation_id  TEXT NOT NULL UNIQUE,
                route_kind    TEXT NOT NULL,
                actor_key     TEXT NOT NULL,
                state_version INTEGER NOT NULL DEFAULT 0 CHECK(state_version >= 0),
                attempt       INTEGER NOT NULL DEFAULT 0 CHECK(attempt >= 0),
                status        TEXT NOT NULL,
                state_json    TEXT NOT NULL DEFAULT '{{}}',
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at    TEXT NOT NULL DEFAULT (datetime('now')),
                CHECK(status IN ({CANONICAL_STATUS_CHECK}))
             );

             CREATE INDEX idx_live_execution_route_status
                ON live_execution_records(route_kind, status);
             CREATE INDEX idx_live_execution_actor_operation
                ON live_execution_records(actor_key, operation_id);"
        ))
        .map_err(|error| to_storage_err(error.to_string()))?;

        conn.execute_batch(
            "INSERT INTO live_execution_records (
                id,
                journal_id,
                operation_id,
                route_kind,
                actor_key,
                state_version,
                attempt,
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
                0,
                status,
                state_json,
                created_at,
                updated_at
             FROM live_execution_records_legacy_v064",
        )
        .map_err(|error| to_storage_err(error.to_string()))?;

        conn.execute_batch("DROP TABLE live_execution_records_legacy_v064;")
            .map_err(|error| to_storage_err(error.to_string()))?;
    }

    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS execution_attempts (
            execution_id   TEXT NOT NULL,
            attempt        INTEGER NOT NULL CHECK(attempt >= 0),
            owner_token    TEXT,
            lease_epoch    INTEGER,
            status         TEXT NOT NULL CHECK(status IN ('preparing', 'running', 'completed', 'cancelled', 'recovery_required', 'needs_review', 'failed')),
            started_at     TEXT NOT NULL DEFAULT (datetime('now')),
            ended_at       TEXT,
            failure_class  TEXT,
            failure_detail TEXT,
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (execution_id, attempt)
        );

        CREATE INDEX IF NOT EXISTS idx_execution_attempts_status
            ON execution_attempts(status, updated_at);

        CREATE TABLE IF NOT EXISTS execution_steps (
            execution_id      TEXT NOT NULL,
            attempt           INTEGER NOT NULL CHECK(attempt >= 0),
            step_seq          INTEGER NOT NULL CHECK(step_seq >= 0),
            step_kind         TEXT NOT NULL,
            step_fingerprint  TEXT NOT NULL,
            tool_name         TEXT,
            reliability_class TEXT NOT NULL DEFAULT 'unsupported_exact_once',
            status            TEXT NOT NULL CHECK(status IN ('started', 'committed', 'failed', 'cancelled')),
            request_json      TEXT,
            result_json       TEXT,
            started_at        TEXT NOT NULL DEFAULT (datetime('now')),
            ended_at          TEXT,
            PRIMARY KEY (execution_id, attempt, step_seq)
        );

        CREATE INDEX IF NOT EXISTS idx_execution_steps_lookup
            ON execution_steps(execution_id, attempt, status, step_seq);
        CREATE INDEX IF NOT EXISTS idx_execution_steps_fingerprint
            ON execution_steps(execution_id, attempt, step_kind, step_fingerprint);
        ",
    )
    .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(())
}

fn live_execution_records_is_v064(conn: &Connection) -> CortexResult<bool> {
    let table_sql = conn
        .query_row(
            "SELECT sql
             FROM sqlite_master
             WHERE type = 'table' AND name = 'live_execution_records'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|error| to_storage_err(error.to_string()))?;

    Ok(table_sql.contains("attempt")
        && table_sql.contains("cancel_requested")
        && table_sql.contains("needs_review")
        && table_sql.contains("failed"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_live_execution_records_to_v064_and_creates_reliability_tables() {
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
                CHECK(status IN ('accepted', 'running', 'completed', 'recovery_required', 'cancelled'))
            );
            INSERT INTO live_execution_records (
                id, journal_id, operation_id, route_kind, actor_key, state_version, status, state_json
            ) VALUES (
                'exec-1', 'journal-1', 'op-1', 'agent_chat', 'actor-1', 1, 'accepted', '{}'
            );",
        )
        .unwrap();

        migrate(&conn).unwrap();

        let row = conn
            .query_row(
                "SELECT attempt, status
                 FROM live_execution_records
                 WHERE id = 'exec-1'",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, 0);
        assert_eq!(row.1, "accepted");

        conn.execute(
            "INSERT INTO execution_attempts (execution_id, attempt, status)
             VALUES ('exec-1', 0, 'running')",
            [],
        )
        .unwrap();

        conn.execute(
            "INSERT INTO execution_steps (
                execution_id, attempt, step_seq, step_kind, step_fingerprint, reliability_class, status
             ) VALUES (
                'exec-1', 0, 1, 'tool_call', 'fp-1', 'replay_safe', 'started'
             )",
            [],
        )
        .unwrap();
    }
}
