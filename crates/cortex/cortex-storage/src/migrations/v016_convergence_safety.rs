//! Migration v016: Convergence safety foundation
//! - Creates base event tables (memory_events, memory_audit_log) if not present
//! - Append-only triggers on event/audit tables
//! - Hash chain columns (event_hash, previous_hash)
//! - Genesis block marker

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // Create base tables if they don't exist (for fresh databases)
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS memory_events (
            event_id    INTEGER PRIMARY KEY AUTOINCREMENT,
            memory_id   TEXT NOT NULL,
            event_type  TEXT NOT NULL,
            delta       TEXT NOT NULL DEFAULT '{}',
            actor_id    TEXT NOT NULL DEFAULT 'system',
            recorded_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS memory_audit_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            memory_id   TEXT NOT NULL,
            operation   TEXT NOT NULL,
            timestamp   TEXT NOT NULL DEFAULT (datetime('now')),
            details     TEXT
        );

        CREATE TABLE IF NOT EXISTS memory_snapshots (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            memory_id   TEXT NOT NULL,
            snapshot     TEXT NOT NULL,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // PART 1: Hash chain columns on memory_events
    conn.execute_batch(
        "
        ALTER TABLE memory_events ADD COLUMN event_hash BLOB NOT NULL DEFAULT x'';
        ALTER TABLE memory_events ADD COLUMN previous_hash BLOB NOT NULL DEFAULT x'';
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // PART 2: Snapshot integrity column
    conn.execute_batch(
        "
        ALTER TABLE memory_snapshots ADD COLUMN state_hash BLOB;
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // PART 3: Append-only triggers
    conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS prevent_memory_events_update
        BEFORE UPDATE ON memory_events
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_events is append-only. Updates forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_memory_events_delete
        BEFORE DELETE ON memory_events
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_events is append-only. Deletes forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_audit_log_update
        BEFORE UPDATE ON memory_audit_log
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_audit_log is append-only. Updates forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_audit_log_delete
        BEFORE DELETE ON memory_audit_log
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_audit_log is append-only. Deletes forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_snapshots_update
        BEFORE UPDATE ON memory_snapshots
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_snapshots is append-only. Updates forbidden.');
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_snapshots_delete
        BEFORE DELETE ON memory_snapshots
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: memory_snapshots is append-only. Deletes forbidden.');
        END;
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // PART 4: Genesis block marker
    conn.execute(
        "INSERT INTO memory_audit_log (memory_id, operation, timestamp, details)
         VALUES ('__GENESIS__', 'CHAIN_GENESIS', datetime('now'),
                 'Hash chain era begins. Events before this point are pre-chain.')",
        [],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
