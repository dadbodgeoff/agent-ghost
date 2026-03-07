//! Migration v017: Convergence core tables
//! 6 new tables, all append-only with triggers and hash chain columns.
//! goal_proposals has UPDATE exception for unresolved proposals only (AC10).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    // TABLE 1: itp_events
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS itp_events (
            id              TEXT PRIMARY KEY,
            session_id      TEXT NOT NULL,
            event_type      TEXT NOT NULL,
            sender          TEXT,
            timestamp       TEXT NOT NULL,
            sequence_number INTEGER NOT NULL DEFAULT 0,
            content_hash    TEXT,
            content_length  INTEGER,
            privacy_level   TEXT NOT NULL DEFAULT 'standard',
            latency_ms      INTEGER,
            token_count     INTEGER,
            event_hash      BLOB NOT NULL,
            previous_hash   BLOB NOT NULL,
            attributes      TEXT DEFAULT '{}',
            created_at      TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_itp_events_session
            ON itp_events(session_id, sequence_number);
        CREATE INDEX IF NOT EXISTS idx_itp_events_timestamp
            ON itp_events(timestamp);
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // TABLE 2: convergence_scores
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS convergence_scores (
            id                  TEXT PRIMARY KEY,
            agent_id            TEXT NOT NULL,
            session_id          TEXT,
            composite_score     REAL NOT NULL,
            signal_scores       TEXT NOT NULL,
            level               INTEGER NOT NULL,
            profile             TEXT NOT NULL DEFAULT 'standard',
            computed_at         TEXT NOT NULL,
            event_hash          BLOB NOT NULL,
            previous_hash       BLOB NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_convergence_scores_agent
            ON convergence_scores(agent_id, computed_at);
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // TABLE 3: intervention_history
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS intervention_history (
            id                  TEXT PRIMARY KEY,
            agent_id            TEXT NOT NULL,
            session_id          TEXT NOT NULL,
            intervention_level  INTEGER NOT NULL,
            previous_level      INTEGER NOT NULL,
            trigger_score       REAL NOT NULL,
            trigger_signals     TEXT NOT NULL,
            action_type         TEXT NOT NULL,
            action_details      TEXT DEFAULT '{}',
            acknowledged        INTEGER DEFAULT 0,
            acknowledged_at     TEXT,
            event_hash          BLOB NOT NULL,
            previous_hash       BLOB NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_intervention_agent
            ON intervention_history(agent_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_intervention_level
            ON intervention_history(intervention_level, created_at);
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // TABLE 4: goal_proposals
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS goal_proposals (
            id                  TEXT PRIMARY KEY,
            agent_id            TEXT NOT NULL,
            session_id          TEXT NOT NULL,
            proposer_type       TEXT NOT NULL,
            operation           TEXT NOT NULL,
            target_type         TEXT NOT NULL,
            content             TEXT NOT NULL,
            cited_memory_ids    TEXT NOT NULL DEFAULT '[]',
            decision            TEXT,
            resolved_at         TEXT,
            resolver            TEXT,
            flags               TEXT DEFAULT '[]',
            dimension_scores    TEXT DEFAULT '{}',
            denial_reason       TEXT,
            event_hash          BLOB NOT NULL,
            previous_hash       BLOB NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_goal_proposals_agent
            ON goal_proposals(agent_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_goal_proposals_pending
            ON goal_proposals(decision) WHERE decision = 'HumanReviewRequired';
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // TABLE 5: reflection_entries
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS reflection_entries (
            id                   TEXT PRIMARY KEY,
            session_id           TEXT NOT NULL,
            chain_id             TEXT NOT NULL,
            depth                INTEGER NOT NULL,
            trigger_type         TEXT NOT NULL,
            reflection_text      TEXT NOT NULL,
            self_references      TEXT NOT NULL DEFAULT '[]',
            self_reference_ratio REAL NOT NULL DEFAULT 0.0,
            event_hash           BLOB NOT NULL,
            previous_hash        BLOB NOT NULL,
            created_at           TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_reflection_session
            ON reflection_entries(session_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_reflection_chain
            ON reflection_entries(chain_id, depth);
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // TABLE 6: boundary_violations
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS boundary_violations (
            id                  TEXT PRIMARY KEY,
            session_id          TEXT NOT NULL,
            violation_type      TEXT NOT NULL,
            severity            REAL NOT NULL,
            trigger_text_hash   TEXT NOT NULL,
            matched_patterns    TEXT NOT NULL DEFAULT '[]',
            action_taken        TEXT NOT NULL,
            convergence_score   REAL,
            intervention_level  INTEGER,
            event_hash          BLOB NOT NULL,
            previous_hash       BLOB NOT NULL,
            created_at          TEXT NOT NULL DEFAULT (datetime('now'))
        );
        CREATE INDEX IF NOT EXISTS idx_boundary_session
            ON boundary_violations(session_id, created_at);
        CREATE INDEX IF NOT EXISTS idx_boundary_type
            ON boundary_violations(violation_type, severity);
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    // APPEND-ONLY TRIGGERS for all 6 tables
    // goal_proposals has a special UPDATE exception for unresolved proposals (AC10)
    let fully_protected = [
        "itp_events",
        "convergence_scores",
        "intervention_history",
        "reflection_entries",
        "boundary_violations",
    ];

    for table in &fully_protected {
        conn.execute_batch(&format!(
            "CREATE TRIGGER IF NOT EXISTS prevent_{table}_update
             BEFORE UPDATE ON {table}
             BEGIN
                 SELECT RAISE(ABORT, 'SAFETY: {table} is append-only. Updates forbidden.');
             END;

             CREATE TRIGGER IF NOT EXISTS prevent_{table}_delete
             BEFORE DELETE ON {table}
             BEGIN
                 SELECT RAISE(ABORT, 'SAFETY: {table} is append-only. Deletes forbidden.');
             END;"
        ))
        .map_err(|e| to_storage_err(e.to_string()))?;
    }

    // goal_proposals: UPDATE only allowed on unresolved proposals (AC10)
    conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS goal_proposals_append_guard
        BEFORE UPDATE ON goal_proposals
        BEGIN
            SELECT CASE WHEN OLD.resolved_at IS NOT NULL
                THEN RAISE(ABORT, 'SAFETY: resolved proposals are immutable.')
            END;
        END;

        CREATE TRIGGER IF NOT EXISTS prevent_goal_proposals_delete
        BEFORE DELETE ON goal_proposals
        BEGIN
            SELECT RAISE(ABORT, 'SAFETY: goal_proposals is append-only. Deletes forbidden.');
        END;
    ",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
