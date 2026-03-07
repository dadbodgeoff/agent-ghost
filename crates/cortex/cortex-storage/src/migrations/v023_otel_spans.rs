//! Migration v023: Create otel_spans table for OpenTelemetry trace storage.
//!
//! Stores spans emitted by the agent loop and gateway for trace visualization.
//! Retention managed by GHOST_TRACE_RETENTION_DAYS (default 7).
//!
//! Ref: tasks.md T-3.1.3, §7.2, §17.2.1

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS otel_spans (
            span_id         TEXT PRIMARY KEY,
            trace_id        TEXT NOT NULL,
            parent_span_id  TEXT,
            operation_name  TEXT NOT NULL,
            start_time      TEXT NOT NULL,
            end_time        TEXT,
            attributes      TEXT NOT NULL DEFAULT '{}',
            status          TEXT NOT NULL DEFAULT 'ok',
            session_id      TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_otel_session
            ON otel_spans(session_id);
        CREATE INDEX IF NOT EXISTS idx_otel_trace
            ON otel_spans(trace_id);
    ",
    )
    .map_err(|e| to_storage_err(format!("v023 otel_spans: {e}")))?;

    Ok(())
}
