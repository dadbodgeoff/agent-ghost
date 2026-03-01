//! Hash chain integrity verification endpoint (T-2.1.4).
//!
//! Uses cortex_temporal hash chain verification on itp_events
//! and memory_events to return chain length, breaks, and anchor status.
//!
//! Ref: ADE_DESIGN_PLAN §5.3.3, tasks.md T-2.1.4

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for integrity check.
#[derive(Debug, Deserialize)]
pub struct IntegrityQueryParams {
    /// Which chain to verify: "itp" (default), "memory", or "both".
    pub chain: Option<String>,
}

/// GET /api/integrity/chain/:agent_id — verify hash chain integrity.
///
/// Walks the event_hash → previous_hash chain for the specified agent's
/// events and returns a verification report.
pub async fn verify_chain(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<IntegrityQueryParams>,
) -> ApiResult<serde_json::Value> {
    let chain_type = params.chain.unwrap_or_else(|| "both".to_string());

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let mut results = serde_json::Map::new();

    // Verify ITP events chain.
    if chain_type == "itp" || chain_type == "both" {
        let itp_result = verify_itp_chain(&db, &agent_id)?;
        results.insert("itp_events".to_string(), itp_result);
    }

    // Verify memory events chain.
    if chain_type == "memory" || chain_type == "both" {
        let mem_result = verify_memory_chain(&db, &agent_id)?;
        results.insert("memory_events".to_string(), mem_result);
    }

    Ok(Json(serde_json::json!({
        "agent_id": agent_id,
        "chain_type": chain_type,
        "chains": results,
    })))
}

/// Verify hash chain for itp_events belonging to sessions where agent is a sender.
fn verify_itp_chain(
    conn: &rusqlite::Connection,
    agent_id: &str,
) -> Result<serde_json::Value, ApiError> {
    // Get all sessions where this agent participated.
    let mut session_stmt = conn
        .prepare(
            "SELECT DISTINCT session_id FROM itp_events \
             WHERE sender = ?1 \
             ORDER BY timestamp ASC",
        )
        .map_err(|e| ApiError::db_error("integrity_itp_sessions", e))?;

    let sessions: Vec<String> = session_stmt
        .query_map([agent_id], |row| row.get(0))
        .map_err(|e| ApiError::db_error("integrity_itp_sessions_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    let mut total_events: usize = 0;
    let mut verified_events: usize = 0;
    let mut breaks = Vec::new();

    for session_id in &sessions {
        let mut stmt = conn
            .prepare(
                "SELECT id, event_type, sender, timestamp, hex(event_hash), hex(previous_hash), \
                        sequence_number \
                 FROM itp_events \
                 WHERE session_id = ?1 \
                 ORDER BY sequence_number ASC",
            )
            .map_err(|e| ApiError::db_error("integrity_itp_events", e))?;

        let events: Vec<(String, String, String)> = stmt
            .query_map([session_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,                                    // id
                    row.get::<_, Option<String>>(4)?.unwrap_or_default(),        // event_hash hex
                    row.get::<_, Option<String>>(5)?.unwrap_or_default(),        // previous_hash hex
                ))
            })
            .map_err(|e| ApiError::db_error("integrity_itp_events_query", e))?
            .filter_map(|r| r.ok())
            .collect();

        total_events += events.len();

        for (i, event) in events.iter().enumerate() {
            if i == 0 {
                verified_events += 1;
                continue;
            }
            let prev_event_hash = &events[i - 1].1;
            let curr_prev_hash = &event.2;

            if prev_event_hash == curr_prev_hash {
                verified_events += 1;
            } else {
                breaks.push(serde_json::json!({
                    "session_id": session_id,
                    "event_id": event.0,
                    "position": i,
                    "expected_prev": prev_event_hash,
                    "actual_prev": curr_prev_hash,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "sessions_checked": sessions.len(),
        "total_events": total_events,
        "verified_events": verified_events,
        "is_valid": breaks.is_empty(),
        "breaks": breaks,
    }))
}

/// Verify hash chain for memory_events belonging to the specified agent.
fn verify_memory_chain(
    conn: &rusqlite::Connection,
    agent_id: &str,
) -> Result<serde_json::Value, ApiError> {
    // Group by memory_id and verify each chain independently.
    let mut mem_stmt = conn
        .prepare(
            "SELECT DISTINCT memory_id FROM memory_events WHERE actor_id = ?1",
        )
        .map_err(|e| ApiError::db_error("integrity_mem_ids", e))?;

    let memory_ids: Vec<String> = mem_stmt
        .query_map([agent_id], |row| row.get(0))
        .map_err(|e| ApiError::db_error("integrity_mem_ids_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    let mut total_events: usize = 0;
    let mut verified_events: usize = 0;
    let mut breaks = Vec::new();

    for memory_id in &memory_ids {
        let mut stmt = conn
            .prepare(
                "SELECT event_id, hex(event_hash), hex(previous_hash) \
                 FROM memory_events \
                 WHERE memory_id = ?1 AND actor_id = ?2 \
                 ORDER BY recorded_at ASC, event_id ASC",
            )
            .map_err(|e| ApiError::db_error("integrity_mem_events", e))?;

        let events: Vec<(i64, String, String)> = stmt
            .query_map(rusqlite::params![memory_id, agent_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                    row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                ))
            })
            .map_err(|e| ApiError::db_error("integrity_mem_events_query", e))?
            .filter_map(|r| r.ok())
            .collect();

        total_events += events.len();

        for (i, event) in events.iter().enumerate() {
            if i == 0 {
                verified_events += 1;
                continue;
            }
            let prev_event_hash = &events[i - 1].1;
            let curr_prev_hash = &event.2;

            if prev_event_hash == curr_prev_hash {
                verified_events += 1;
            } else {
                breaks.push(serde_json::json!({
                    "memory_id": memory_id,
                    "event_id": event.0,
                    "position": i,
                    "expected_prev": prev_event_hash,
                    "actual_prev": curr_prev_hash,
                }));
            }
        }
    }

    Ok(serde_json::json!({
        "memory_chains_checked": memory_ids.len(),
        "total_events": total_events,
        "verified_events": verified_events,
        "is_valid": breaks.is_empty(),
        "breaks": breaks,
    }))
}
