//! Migration v020: Add actor_id column to audit_log table.
//!
//! Tracks which authenticated user (JWT `sub` claim) performed each
//! state-changing API action. Enables per-user audit trails in
//! multi-user deployments.
//!
//! Ref: ADE_DESIGN_PLAN §17.1, §17.2.1, tasks.md T-1.1.7

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // Ensure audit_log table exists before ALTER. In production the table is
    // created by ghost-audit's AuditQueryEngine::ensure_table(), but during
    // migration-only contexts (tests, fresh DBs where the gateway hasn't
    // called ensure_table() yet) it may not exist. We replicate the canonical
    // schema here with CREATE TABLE IF NOT EXISTS so the ALTER is safe.
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS audit_log (
            id TEXT PRIMARY KEY,
            timestamp TEXT NOT NULL,
            agent_id TEXT NOT NULL,
            event_type TEXT NOT NULL,
            severity TEXT NOT NULL DEFAULT 'info',
            tool_name TEXT,
            details TEXT NOT NULL DEFAULT '',
            session_id TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_timestamp ON audit_log(timestamp);
        CREATE INDEX IF NOT EXISTS idx_audit_agent ON audit_log(agent_id);
        CREATE INDEX IF NOT EXISTS idx_audit_event_type ON audit_log(event_type);
        CREATE INDEX IF NOT EXISTS idx_audit_severity ON audit_log(severity);
    ")
    .map_err(|e| to_storage_err(format!("v020 ensure audit_log: {e}")))?;

    // Add actor_id column to audit_log. NULL for pre-existing entries
    // and for actions performed in no-auth mode.
    //
    // SQLite does not support ADD COLUMN IF NOT EXISTS, so we check
    // whether the column already exists first (idempotency guard for
    // DBs where ghost-audit's ensure_table() already ran a future
    // version that includes actor_id, or if this migration is re-run
    // after a partial failure that committed the ALTER but not the
    // schema_version row).
    let has_actor_id: bool = conn
        .prepare("SELECT 1 FROM pragma_table_info('audit_log') WHERE name = 'actor_id'")
        .and_then(|mut stmt| stmt.exists([]))
        .unwrap_or(false);

    if !has_actor_id {
        conn.execute_batch("
            ALTER TABLE audit_log ADD COLUMN actor_id TEXT;
        ")
        .map_err(|e| to_storage_err(format!("v020 actor_id: {e}")))?;
    }

    // Index for querying audit entries by actor.
    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_audit_log_actor_id
            ON audit_log(actor_id);
    ")
    .map_err(|e| to_storage_err(format!("v020 actor_id index: {e}")))?;

    Ok(())
}
