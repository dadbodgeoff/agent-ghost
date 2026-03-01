//! Migration v019: Intervention state table + missing indexes.
//!
//! - Creates `intervention_state` table for convergence-monitor state persistence
//!   across restarts (Req 9 AC2, AC8). Without this table, the monitor's
//!   `reconstruct_state()` silently falls back to L0 for all agents on every restart.
//! - Adds missing index on `memory_events(memory_id)` for JOIN performance in
//!   the gateway memory API.
//! - Adds missing index on `itp_events(event_type)` for calibration count queries
//!   in the convergence monitor's `reconstruct_state()`.

use rusqlite::Connection;
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // TABLE: intervention_state
    // Stores the convergence monitor's per-agent intervention state machine.
    // NOT append-only — this is a mutable state table that gets UPDATEd
    // on every intervention level change. One row per agent.
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS intervention_state (
            agent_id                TEXT PRIMARY KEY,
            level                   INTEGER NOT NULL DEFAULT 0,
            consecutive_normal      INTEGER NOT NULL DEFAULT 0,
            cooldown_until          TEXT,
            ack_required            INTEGER NOT NULL DEFAULT 0,
            hysteresis_count        INTEGER NOT NULL DEFAULT 0,
            de_escalation_credits   INTEGER NOT NULL DEFAULT 0,
            updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
        );
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Missing index: memory_events.memory_id (used in JOIN by gateway memory API)
    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_memory_events_memory_id
            ON memory_events(memory_id);
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Missing index: itp_events.event_type (used in calibration count GROUP BY)
    conn.execute_batch("
        CREATE INDEX IF NOT EXISTS idx_itp_events_event_type
            ON itp_events(event_type);
    ")
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
