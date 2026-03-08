//! v051: Durable live execution records for replay-safe accepted executions.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS live_execution_records (
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

        CREATE INDEX IF NOT EXISTS idx_live_execution_route_status
            ON live_execution_records(route_kind, status);
        CREATE INDEX IF NOT EXISTS idx_live_execution_actor_operation
            ON live_execution_records(actor_key, operation_id);
        ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
