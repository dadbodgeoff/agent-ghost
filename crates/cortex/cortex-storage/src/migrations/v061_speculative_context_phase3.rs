//! Migration v061: Speculative context phase 3 promotion controls.
//!
//! Adds:
//! - `fact_candidate` attempt kind support
//! - `promote` job type support for upgraded databases
//! - `context_attempt_promotion` linkage records

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "PRAGMA foreign_keys = OFF;

        ALTER TABLE context_attempt_validation RENAME TO context_attempt_validation_v060;
        ALTER TABLE context_attempt_jobs RENAME TO context_attempt_jobs_v060;
        ALTER TABLE context_attempts RENAME TO context_attempts_v060;

        DROP INDEX IF EXISTS idx_context_attempt_validation_attempt;
        DROP INDEX IF EXISTS idx_context_attempt_validation_gate;
        DROP INDEX IF EXISTS idx_context_attempt_jobs_due;
        DROP INDEX IF EXISTS idx_context_attempt_jobs_attempt;
        DROP INDEX IF EXISTS idx_context_attempts_session_status_expiry;
        DROP INDEX IF EXISTS idx_context_attempts_agent_session_expiry;
        DROP INDEX IF EXISTS idx_context_attempts_promotion;
        DROP INDEX IF EXISTS idx_context_attempts_turn_kind;

        CREATE TABLE context_attempts (
            id TEXT PRIMARY KEY,
            agent_id TEXT NOT NULL,
            session_id TEXT NOT NULL,
            turn_id TEXT NOT NULL,
            attempt_kind TEXT NOT NULL CHECK (
                attempt_kind IN ('summary', 'fact_candidate')
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

        INSERT INTO context_attempts (
            id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
            status, severity, confidence, retrieval_weight, source_refs, source_hash,
            fast_gate_version, contradicted_by_memory_id, promotion_candidate,
            expires_at, created_at, updated_at
        )
        SELECT
            id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
            status, severity, confidence, retrieval_weight, source_refs, source_hash,
            fast_gate_version, contradicted_by_memory_id, promotion_candidate,
            expires_at, created_at, updated_at
        FROM context_attempts_v060;

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

        INSERT INTO context_attempt_validation (
            id, attempt_id, gate_name, decision, reason, score, details_json, created_at
        )
        SELECT
            id, attempt_id, gate_name, decision, reason, score, details_json, created_at
        FROM context_attempt_validation_v060;

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
            ON context_attempt_jobs(attempt_id, job_type, created_at);

        INSERT INTO context_attempt_jobs (
            id, attempt_id, job_type, status, retry_count, last_error, run_after, created_at, updated_at
        )
        SELECT
            id, attempt_id, job_type, status, retry_count, last_error, run_after, created_at, updated_at
        FROM context_attempt_jobs_v060;

        CREATE TABLE context_attempt_promotion (
            id TEXT PRIMARY KEY,
            attempt_id TEXT NOT NULL REFERENCES context_attempts(id) ON DELETE CASCADE,
            promoted_memory_id TEXT NOT NULL,
            promotion_type TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX idx_context_attempt_promotion_attempt
            ON context_attempt_promotion(attempt_id, created_at);
        CREATE INDEX idx_context_attempt_promotion_memory
            ON context_attempt_promotion(promoted_memory_id, created_at);

        DROP TABLE context_attempt_validation_v060;
        DROP TABLE context_attempt_jobs_v060;
        DROP TABLE context_attempts_v060;

        PRAGMA foreign_keys = ON;",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(())
}
