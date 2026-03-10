//! Query helpers for speculative context attempts.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone)]
pub struct NewContextAttempt<'a> {
    pub id: &'a str,
    pub agent_id: &'a str,
    pub session_id: &'a str,
    pub turn_id: &'a str,
    pub attempt_kind: &'a str,
    pub content: &'a str,
    pub redacted_content: Option<&'a str>,
    pub status: &'a str,
    pub severity: f64,
    pub confidence: f64,
    pub retrieval_weight: f64,
    pub source_refs: &'a str,
    pub source_hash: Option<&'a [u8]>,
    pub fast_gate_version: i64,
    pub contradicted_by_memory_id: Option<&'a str>,
    pub promotion_candidate: bool,
    pub expires_at: &'a str,
}

#[derive(Debug, Clone)]
pub struct ContextAttemptRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub attempt_kind: String,
    pub content: String,
    pub redacted_content: Option<String>,
    pub status: String,
    pub severity: f64,
    pub confidence: f64,
    pub retrieval_weight: f64,
    pub source_refs: String,
    pub source_hash: Option<Vec<u8>>,
    pub fast_gate_version: i64,
    pub contradicted_by_memory_id: Option<String>,
    pub promotion_candidate: bool,
    pub expires_at: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn insert_attempt(conn: &Connection, attempt: &NewContextAttempt<'_>) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO context_attempts (
            id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
            status, severity, confidence, retrieval_weight, source_refs, source_hash,
            fast_gate_version, contradicted_by_memory_id, promotion_candidate, expires_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)",
        params![
            attempt.id,
            attempt.agent_id,
            attempt.session_id,
            attempt.turn_id,
            attempt.attempt_kind,
            attempt.content,
            attempt.redacted_content,
            attempt.status,
            attempt.severity,
            attempt.confidence,
            attempt.retrieval_weight,
            attempt.source_refs,
            attempt.source_hash,
            attempt.fast_gate_version,
            attempt.contradicted_by_memory_id,
            if attempt.promotion_candidate {
                1_i64
            } else {
                0_i64
            },
            attempt.expires_at,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn get_attempt(conn: &Connection, id: &str) -> CortexResult<Option<ContextAttemptRow>> {
    conn.query_row(
        "SELECT id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
                status, severity, confidence, retrieval_weight, source_refs, source_hash,
                fast_gate_version, contradicted_by_memory_id, promotion_candidate,
                expires_at, created_at, updated_at
         FROM context_attempts
         WHERE id = ?1",
        params![id],
        map_row,
    )
    .optional()
    .map_err(|e| to_storage_err(e.to_string()))
}

pub fn list_retrievable_for_session(
    conn: &Connection,
    session_id: &str,
    now: &str,
    limit: u32,
) -> CortexResult<Vec<ContextAttemptRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
                    status, severity, confidence, retrieval_weight, source_refs, source_hash,
                    fast_gate_version, contradicted_by_memory_id, promotion_candidate,
                    expires_at, created_at, updated_at
             FROM context_attempts
             WHERE session_id = ?1
               AND status = 'retrievable'
               AND expires_at > ?2
             ORDER BY created_at DESC
             LIMIT ?3",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id, now, limit], map_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(rows)
}

pub fn list_recent_for_turn(
    conn: &Connection,
    session_id: &str,
    turn_id: &str,
    attempt_kind: &str,
    limit: u32,
) -> CortexResult<Vec<ContextAttemptRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, turn_id, attempt_kind, content, redacted_content,
                    status, severity, confidence, retrieval_weight, source_refs, source_hash,
                    fast_gate_version, contradicted_by_memory_id, promotion_candidate,
                    expires_at, created_at, updated_at
             FROM context_attempts
             WHERE session_id = ?1 AND turn_id = ?2 AND attempt_kind = ?3
             ORDER BY created_at DESC
             LIMIT ?4",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![session_id, turn_id, attempt_kind, limit], map_row)
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(rows)
}

pub fn update_attempt_status(
    conn: &Connection,
    id: &str,
    status: &str,
    contradicted_by_memory_id: Option<&str>,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE context_attempts
             SET status = ?2,
                 contradicted_by_memory_id = ?3,
                 updated_at = datetime('now')
             WHERE id = ?1",
            params![id, status, contradicted_by_memory_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn expire_due_attempts(conn: &Connection, now: &str, limit: u32) -> CortexResult<usize> {
    let updated = conn
        .execute(
            "UPDATE context_attempts
             SET status = 'expired',
                 updated_at = datetime('now')
             WHERE id IN (
                 SELECT id
                 FROM context_attempts
                 WHERE status IN ('pending', 'retrievable', 'flagged')
                   AND expires_at <= ?1
                 ORDER BY expires_at ASC
                 LIMIT ?2
             )",
            params![now, limit],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated)
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ContextAttemptRow> {
    Ok(ContextAttemptRow {
        id: row.get(0)?,
        agent_id: row.get(1)?,
        session_id: row.get(2)?,
        turn_id: row.get(3)?,
        attempt_kind: row.get(4)?,
        content: row.get(5)?,
        redacted_content: row.get(6)?,
        status: row.get(7)?,
        severity: row.get(8)?,
        confidence: row.get(9)?,
        retrieval_weight: row.get(10)?,
        source_refs: row.get(11)?,
        source_hash: row.get(12)?,
        fast_gate_version: row.get(13)?,
        contradicted_by_memory_id: row.get(14)?,
        promotion_candidate: row.get::<_, i64>(15)? != 0,
        expires_at: row.get(16)?,
        created_at: row.get(17)?,
        updated_at: row.get(18)?,
    })
}
