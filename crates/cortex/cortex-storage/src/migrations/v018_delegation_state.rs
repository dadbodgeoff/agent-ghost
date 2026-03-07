//! Migration v018: Delegation state table for inter-agent messaging (A34 Gap 7).
//!
//! Stores delegation state machine transitions. Append-only with guard:
//! resolved delegations (Completed/Disputed/Rejected) are immutable.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // TABLE: delegation_state
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS delegation_state (
            id              TEXT PRIMARY KEY,
            delegation_id   TEXT NOT NULL,
            sender_id       TEXT NOT NULL,
            recipient_id    TEXT NOT NULL,
            task             TEXT NOT NULL,
            state           TEXT NOT NULL DEFAULT 'Offered',
            offer_message_id TEXT NOT NULL,
            accept_message_id TEXT,
            complete_message_id TEXT,
            result          TEXT,
            dispute_reason  TEXT,
            event_hash      BLOB NOT NULL,
            previous_hash   BLOB NOT NULL,
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_delegation_state_delegation
            ON delegation_state(delegation_id);
        CREATE INDEX IF NOT EXISTS idx_delegation_state_sender
            ON delegation_state(sender_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_delegation_state_recipient
            ON delegation_state(recipient_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_delegation_state_pending
            ON delegation_state(state) WHERE state = 'Offered' OR state = 'Accepted';
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // Append-only guard: resolved delegations are immutable.
    // Only Offered→Accepted/Rejected and Accepted→Completed/Disputed transitions allowed.
    conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS delegation_state_append_guard
        BEFORE UPDATE ON delegation_state
        BEGIN
            SELECT CASE
                WHEN OLD.state IN ('Completed', 'Disputed', 'Rejected')
                THEN RAISE(ABORT, 'SAFETY: resolved delegations are immutable.')
            END;
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_delegation_state_delete
        BEFORE DELETE ON delegation_state
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: delegation_state is append-only. Deletes forbidden.');
        END;
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
