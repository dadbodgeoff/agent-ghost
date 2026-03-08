//! Migration v045: operation journal and audit provenance columns.

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

fn has_column(conn: &Connection, table: &str, column: &str) -> CortexResult<bool> {
    let sql = format!("SELECT 1 FROM pragma_table_info('{table}') WHERE name = ?1 LIMIT 1");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;
    let exists = stmt
        .query_row([column], |_row| Ok(()))
        .map(|_| true)
        .or_else(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => Ok(false),
            other => Err(other),
        })
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(exists)
}

fn has_table(conn: &Connection, table: &str) -> CortexResult<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_row| Ok(()),
    )
    .map(|_| true)
    .or_else(|err| match err {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(other),
    })
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS operation_journal (
            id TEXT PRIMARY KEY,
            actor_key TEXT NOT NULL,
            method TEXT NOT NULL,
            route_template TEXT NOT NULL,
            operation_id TEXT NOT NULL,
            request_id TEXT,
            idempotency_key TEXT NOT NULL,
            request_fingerprint TEXT NOT NULL,
            request_body TEXT NOT NULL DEFAULT 'null',
            status TEXT NOT NULL CHECK(status IN ('in_progress', 'committed')),
            response_status_code INTEGER,
            response_body TEXT,
            response_content_type TEXT,
            created_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            committed_at TEXT,
            lease_expires_at TEXT
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_operation_journal_actor_key_idempotency
            ON operation_journal(actor_key, idempotency_key);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_operation_journal_operation_id
            ON operation_journal(operation_id);
        CREATE INDEX IF NOT EXISTS idx_operation_journal_status_lease
            ON operation_journal(status, lease_expires_at);
        CREATE INDEX IF NOT EXISTS idx_operation_journal_fingerprint
            ON operation_journal(request_fingerprint);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    if !has_table(conn, "audit_log")? || has_column(conn, "audit_log", "operation_id")? {
        return Ok(());
    }

    conn.execute_batch(
        "ALTER TABLE audit_log ADD COLUMN operation_id TEXT;
         ALTER TABLE audit_log ADD COLUMN request_id TEXT;
         ALTER TABLE audit_log ADD COLUMN idempotency_key TEXT;
         ALTER TABLE audit_log ADD COLUMN idempotency_status TEXT;
         CREATE INDEX IF NOT EXISTS idx_audit_operation_id
             ON audit_log(operation_id);
         CREATE INDEX IF NOT EXISTS idx_audit_idempotency_key
             ON audit_log(idempotency_key);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
