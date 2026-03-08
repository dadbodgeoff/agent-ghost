//! Migration v048: canonicalize audit_log ownership under migrations.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const BASE_COLUMNS: &[&str] = &[
    "id",
    "timestamp",
    "agent_id",
    "event_type",
    "severity",
    "tool_name",
    "details",
    "session_id",
];
const OPTIONAL_CANONICAL_COLUMNS: &[(&str, &str)] = &[
    ("actor_id", "TEXT"),
    ("operation_id", "TEXT"),
    ("request_id", "TEXT"),
    ("idempotency_key", "TEXT"),
    ("idempotency_status", "TEXT"),
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if has_table(conn, "audit_log")? {
        let columns = table_columns(conn, "audit_log")?;
        let missing = BASE_COLUMNS
            .iter()
            .filter(|column| !columns.contains(**column))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(to_storage_err(format!(
                "unsupported legacy audit_log shape: missing required columns [{}]",
                missing.join(", ")
            )));
        }

        for (column, column_type) in OPTIONAL_CANONICAL_COLUMNS {
            if !columns.contains(*column) {
                let sql = format!("ALTER TABLE audit_log ADD COLUMN {column} {column_type};");
                conn.execute_batch(&sql).map_err(|error| {
                    to_storage_err(format!("v048 audit_log add {column}: {error}"))
                })?;
            }
        }
    } else {
        conn.execute_batch(
            "CREATE TABLE audit_log (
                id TEXT PRIMARY KEY,
                timestamp TEXT NOT NULL,
                agent_id TEXT NOT NULL,
                event_type TEXT NOT NULL,
                severity TEXT NOT NULL DEFAULT 'info',
                tool_name TEXT,
                details TEXT NOT NULL DEFAULT '',
                session_id TEXT,
                actor_id TEXT,
                operation_id TEXT,
                request_id TEXT,
                idempotency_key TEXT,
                idempotency_status TEXT
            );",
        )
        .map_err(|error| to_storage_err(format!("v048 create audit_log: {error}")))?;
    }

    conn.execute(
        "UPDATE audit_log
         SET actor_id = json_extract(details, '$.actor')
         WHERE actor_id IS NULL
           AND json_valid(details)
           AND json_type(details, '$.actor') = 'text'",
        [],
    )
    .map_err(|error| to_storage_err(format!("v048 backfill audit_log.actor_id: {error}")))?;

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
         CREATE INDEX IF NOT EXISTS idx_audit_agent ON audit_log(agent_id);
         CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log(event_type);
         CREATE INDEX IF NOT EXISTS idx_audit_severity ON audit_log(severity);
         CREATE INDEX IF NOT EXISTS idx_audit_log_actor_id ON audit_log(actor_id);
         CREATE INDEX IF NOT EXISTS idx_audit_operation_id ON audit_log(operation_id);
         CREATE INDEX IF NOT EXISTS idx_audit_idempotency_key ON audit_log(idempotency_key);

         CREATE TRIGGER IF NOT EXISTS prevent_audit_log_row_update
         BEFORE UPDATE ON audit_log
         BEGIN
             SELECT RAISE(ABORT, 'SAFETY: audit_log is append-only. Updates forbidden.');
         END;

         CREATE TRIGGER IF NOT EXISTS prevent_audit_log_row_delete
         BEFORE DELETE ON audit_log
         BEGIN
             SELECT RAISE(ABORT, 'SAFETY: audit_log is append-only. Deletes forbidden.');
         END;",
    )
    .map_err(|error| to_storage_err(format!("v048 canonicalize audit_log: {error}")))?;

    Ok(())
}

fn has_table(conn: &Connection, table: &str) -> CortexResult<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_row| Ok(()),
    )
    .map(|_| true)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(other),
    })
    .map_err(|error| to_storage_err(error.to_string()))
}

fn table_columns(conn: &Connection, table: &str) -> CortexResult<BTreeSet<String>> {
    let sql = format!("PRAGMA table_info('{table}')");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows.into_iter().collect())
}
