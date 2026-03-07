//! CRDT state inspection endpoint (T-2.1.3).
//!
//! Reconstructs an agent's CRDT state from the memory_events delta log,
//! returning the raw deltas and signature verification status.
//!
//! Ref: ADE_DESIGN_PLAN §5.3.2, tasks.md T-2.1.3

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for CRDT state endpoint.
#[derive(Debug, Deserialize)]
pub struct CrdtQueryParams {
    /// Optional memory_id filter — if omitted, returns all deltas for agent.
    pub memory_id: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// GET /api/state/crdt/:agent_id — reconstruct CRDT state from memory_events.
///
/// Returns the delta log for the specified agent, along with chain
/// integrity status (hash chain verification on the event_hash/previous_hash columns).
pub async fn get_crdt_state(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<CrdtQueryParams>,
) -> ApiResult<serde_json::Value> {
    let limit = params.limit.unwrap_or(100).min(500);
    let offset = params.offset.unwrap_or(0);

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::internal(&format!("db pool: {e}")))?;

    // Build query based on whether memory_id filter is provided.
    let (query, count_query) = if params.memory_id.is_some() {
        (
            "SELECT event_id, memory_id, event_type, delta, actor_id, recorded_at, \
                    hex(event_hash) as event_hash_hex, hex(previous_hash) as prev_hash_hex \
             FROM memory_events \
             WHERE actor_id = ?1 AND memory_id = ?2 \
             ORDER BY recorded_at ASC \
             LIMIT ?3 OFFSET ?4",
            "SELECT COUNT(*) FROM memory_events WHERE actor_id = ?1 AND memory_id = ?2",
        )
    } else {
        (
            "SELECT event_id, memory_id, event_type, delta, actor_id, recorded_at, \
                    hex(event_hash) as event_hash_hex, hex(previous_hash) as prev_hash_hex \
             FROM memory_events \
             WHERE actor_id = ?1 \
             ORDER BY recorded_at ASC \
             LIMIT ?2 OFFSET ?3",
            "SELECT COUNT(*) FROM memory_events WHERE actor_id = ?1",
        )
    };

    // Count total.
    let total: u32 = if let Some(ref mid) = params.memory_id {
        db.query_row(count_query, rusqlite::params![&agent_id, mid], |row| {
            row.get(0)
        })
        .map_err(|e| ApiError::db_error("crdt_count", e))?
    } else {
        db.query_row(count_query, rusqlite::params![&agent_id], |row| row.get(0))
            .map_err(|e| ApiError::db_error("crdt_count", e))?
    };

    // Fetch deltas.
    let mut stmt = db
        .prepare(query)
        .map_err(|e| ApiError::db_error("crdt_prepare", e))?;

    let rows = if let Some(ref mid) = params.memory_id {
        stmt.query_map(
            rusqlite::params![&agent_id, mid, limit, offset],
            map_delta_row,
        )
    } else {
        stmt.query_map(rusqlite::params![&agent_id, limit, offset], map_delta_row)
    };

    let mut deltas = Vec::new();
    match rows {
        Ok(rows) => {
            for row in rows {
                match row {
                    Ok(r) => deltas.push(r),
                    Err(e) => tracing::warn!(error = %e, "skipping malformed crdt delta row"),
                }
            }
        }
        Err(e) => return Err(ApiError::db_error("crdt_query", e)),
    }

    // Verify hash chain integrity on the fetched deltas.
    let chain_valid = verify_delta_chain(&deltas);

    Ok(Json(serde_json::json!({
        "agent_id": agent_id,
        "deltas": deltas,
        "total": total,
        "limit": limit,
        "offset": offset,
        "chain_valid": chain_valid,
    })))
}

fn map_delta_row(row: &rusqlite::Row) -> rusqlite::Result<serde_json::Value> {
    Ok(serde_json::json!({
        "event_id": row.get::<_, i64>(0)?,
        "memory_id": row.get::<_, String>(1)?,
        "event_type": row.get::<_, String>(2)?,
        "delta": row.get::<_, String>(3)?,
        "actor_id": row.get::<_, String>(4)?,
        "recorded_at": row.get::<_, String>(5)?,
        "event_hash": row.get::<_, Option<String>>(6)?.unwrap_or_default(),
        "previous_hash": row.get::<_, Option<String>>(7)?.unwrap_or_default(),
    }))
}

/// Verify that the event_hash/previous_hash chain is consistent.
/// Returns true if all links are valid (or if there are 0-1 events).
fn verify_delta_chain(deltas: &[serde_json::Value]) -> bool {
    if deltas.len() <= 1 {
        return true;
    }
    for i in 1..deltas.len() {
        let prev_hash = deltas[i - 1].get("event_hash").and_then(|v| v.as_str());
        let curr_prev = deltas[i].get("previous_hash").and_then(|v| v.as_str());
        if prev_hash != curr_prev {
            return false;
        }
    }
    true
}
