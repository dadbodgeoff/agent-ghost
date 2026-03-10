//! Migration v060: Speculative context phase 1 tables.
//!
//! Phase 1 scope:
//! - context_attempts
//! - context_attempt_validation
//! - context_attempt_jobs
//! - bounded retrieval + worker indexes

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE context_attempts (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            attempt_kind TEXT NOT NULL CHECK (
                attempt_kind IN ('summary')
            ),
            content TEXT NOT NULL,
            redacted_content TEXT,
            status TEXT NOT NULL CHECK (
                status IN ('pending', 'retrievable', 'flagged', 'blocked', 'promoted', 'expired')
            ),
            severity REAL NOT NULL DEFAULT 0.0,
            confidence REAL NOT NULL DEFAULT 0.0,
            retrieval_weight REAL NOT NULL DEFAULT 0.0,
            source_refs TEXT NOT NULL,
            source_hash BLOB,
            fast_gate_version INTEGER NOT NULL DEFAULT 1,
            contradicted_by_memory_id TEXT,
            promotion_candidate INTEGER NOT NULL DEFAULT 0,
            expires_at TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX idx_context_attempts_session_status_expiry
            ON context_attempts(session_id, status, expires_at, created_at DESC);
        CREATE INDEX idx_context_attempts_agent_session_expiry
            ON context_attempts(agent_id, session_id, expires_at);
        CREATE INDEX idx_context_attempts_promotion
            ON context_attempts(promotion_candidate, status, created_at);
        CREATE INDEX idx_context_attempts_turn_kind
            ON context_attempts(session_id, turn_id, attempt_kind, created_at DESC);

        CREATE TABLE context_attempt_validation (
            id TEXT PRIMARY KEY,
            attempt_id TEXT NOT NULL REFERENCES context_attempts(id) ON DELETE CASCADE,
            gate_name TEXT NOT NULL,
            decision TEXT NOT NULL CHECK (
                decision IN ('passed', 'flagged', 'blocked', 'deferred')
            ),
            reason TEXT,
            score REAL,
            details_json TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX idx_context_attempt_validation_attempt
            ON context_attempt_validation(attempt_id, created_at);
        CREATE INDEX idx_context_attempt_validation_gate
            ON context_attempt_validation(gate_name, decision, created_at);

        CREATE TABLE context_attempt_jobs (
            id TEXT PRIMARY KEY,
            attempt_id TEXT NOT NULL REFERENCES context_attempts(id) ON DELETE CASCADE,
            job_type TEXT NOT NULL CHECK (
                job_type IN ('deep_validate', 'promote', 'expire')
            ),
            status TEXT NOT NULL CHECK (
                status IN ('pending', 'running', 'succeeded', 'failed', 'dead_letter')
            ),
            retry_count INTEGER NOT NULL DEFAULT 0,
            last_error TEXT,
            run_after TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX idx_context_attempt_jobs_due
            ON context_attempt_jobs(job_type, status, run_after, created_at);
        CREATE INDEX idx_context_attempt_jobs_attempt
            ON context_attempt_jobs(attempt_id, job_type, created_at);",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
