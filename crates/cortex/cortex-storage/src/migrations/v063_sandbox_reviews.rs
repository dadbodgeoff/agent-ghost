//! v063: durable sandbox review queue for interactive runtime approvals.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS sandbox_review_requests (
            id                TEXT PRIMARY KEY,
            agent_id          TEXT NOT NULL,
            session_id        TEXT NOT NULL,
            execution_id      TEXT,
            route_kind        TEXT,
            tool_name         TEXT NOT NULL,
            violation_reason  TEXT NOT NULL,
            sandbox_mode      TEXT NOT NULL,
            status            TEXT NOT NULL
                              CHECK(status IN ('pending', 'approved', 'rejected', 'expired')),
            resolution_note   TEXT,
            resolved_by       TEXT,
            requested_at      TEXT NOT NULL DEFAULT (datetime('now')),
            resolved_at       TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_sandbox_reviews_status
            ON sandbox_review_requests(status, requested_at DESC);
        CREATE INDEX IF NOT EXISTS idx_sandbox_reviews_agent
            ON sandbox_review_requests(agent_id, requested_at DESC);
        CREATE INDEX IF NOT EXISTS idx_sandbox_reviews_execution
            ON sandbox_review_requests(execution_id);",
    )
    .map_err(|error| to_storage_err(format!("v063 sandbox reviews: {error}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_sandbox_review_queue_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let count = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sandbox_review_requests'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
