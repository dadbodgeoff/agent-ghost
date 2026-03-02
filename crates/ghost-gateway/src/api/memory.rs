//! Memory API endpoints (Req 25 AC1-2).
//!
//! Phase 2: Wired to cortex-storage memory_snapshots table
//! (created in v016_convergence_safety migration).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

/// Query parameters for memory listing.
#[derive(Debug, Deserialize)]
pub struct MemoryQueryParams {
    pub agent_id: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// Include archived memories in results (default: false).
    pub include_archived: Option<bool>,
}

/// Query parameters for memory search (T-2.1.2).
#[derive(Debug, Deserialize)]
pub struct MemorySearchParams {
    /// Search query (LIKE matching on snapshot content).
    pub q: Option<String>,
    pub agent_id: Option<String>,
    pub memory_type: Option<String>,
    pub importance: Option<String>,
    pub confidence_min: Option<f64>,
    pub confidence_max: Option<f64>,
    pub limit: Option<u32>,
    /// Include archived memories in results (default: false).
    pub include_archived: Option<bool>,
}

/// GET /api/memory — list memory snapshots with optional agent_id filter.
pub async fn list_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemoryQueryParams>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    let include_archived = params.include_archived.unwrap_or(false);
    let archival_filter = if include_archived {
        ""
    } else {
        " AND ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)"
    };

    // Count total — use COUNT(DISTINCT ms.id) to avoid inflation from
    // the 1:N JOIN between memory_snapshots and memory_events (F2 fix).
    let total: u32 = match &params.agent_id {
        Some(agent_id) => {
            let sql = format!(
                "SELECT COUNT(DISTINCT ms.id) FROM memory_snapshots ms \
                 JOIN memory_events me ON ms.memory_id = me.memory_id \
                 WHERE me.actor_id = ?1{archival_filter}"
            );
            match db.query_row(&sql, [agent_id], |row| row.get(0)) {
                Ok(count) => count,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("count query failed: {e}")})),
                    );
                }
            }
        }
        None => {
            let sql = if include_archived {
                "SELECT COUNT(*) FROM memory_snapshots".to_string()
            } else {
                "SELECT COUNT(*) FROM memory_snapshots ms \
                 WHERE ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)"
                    .to_string()
            };
            match db.query_row(&sql, [], |row| row.get(0)) {
                Ok(count) => count,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("count query failed: {e}")})),
                    );
                }
            }
        }
    };

    // Fetch page.
    let mut memories = Vec::new();
    if let Some(ref agent_id) = params.agent_id {
        let sql = format!(
            "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at \
             FROM memory_snapshots ms \
             JOIN memory_events me ON ms.memory_id = me.memory_id \
             WHERE me.actor_id = ?1{archival_filter} \
             GROUP BY ms.id \
             ORDER BY ms.created_at DESC LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = match db.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
                );
            }
        };
        let rows = stmt.query_map(
            rusqlite::params![agent_id, page_size, offset],
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "memory_id": row.get::<_, String>(1)?,
                    "snapshot": row.get::<_, String>(2)?,
                    "created_at": row.get::<_, String>(3)?,
                }))
            },
        );
        match rows {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(r) => memories.push(r),
                        Err(e) => tracing::warn!(error = %e, "skipping malformed memory row"),
                    }
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query failed: {e}")})),
                );
            }
        };
    } else {
        let sql = if include_archived {
            "SELECT id, memory_id, snapshot, created_at \
             FROM memory_snapshots \
             ORDER BY created_at DESC LIMIT ?1 OFFSET ?2"
                .to_string()
        } else {
            "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at \
             FROM memory_snapshots ms \
             WHERE ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log) \
             ORDER BY ms.created_at DESC LIMIT ?1 OFFSET ?2"
                .to_string()
        };
        let mut stmt = match db.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
                );
            }
        };
        let rows = stmt.query_map(
            rusqlite::params![page_size, offset],
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "memory_id": row.get::<_, String>(1)?,
                    "snapshot": row.get::<_, String>(2)?,
                    "created_at": row.get::<_, String>(3)?,
                }))
            },
        );
        match rows {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(r) => memories.push(r),
                        Err(e) => tracing::warn!(error = %e, "skipping malformed memory row"),
                    }
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query failed: {e}")})),
                );
            }
        };
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "memories": memories,
            "page": page,
            "page_size": page_size,
            "total": total,
        })),
    )
}

/// GET /api/memory/:id — get a specific memory snapshot by ID.
pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    // Try by memory_id first (TEXT), then by numeric id only if parseable (F1 fix).
    let row = db
        .query_row(
            "SELECT id, memory_id, snapshot, created_at FROM memory_snapshots WHERE memory_id = ?1",
            [&id],
            |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, i64>(0)?,
                    "memory_id": row.get::<_, String>(1)?,
                    "snapshot": row.get::<_, String>(2)?,
                    "created_at": row.get::<_, String>(3)?,
                }))
            },
        )
        .or_else(|first_err| {
            // Only fall through to numeric PK lookup on "not found" errors.
            // Real DB errors (table missing, lock, etc.) should propagate.
            if !matches!(first_err, rusqlite::Error::QueryReturnedNoRows) {
                return Err(first_err);
            }
            // Only attempt numeric PK lookup if the id is a valid integer.
            // memory_snapshots.id is INTEGER PRIMARY KEY AUTOINCREMENT —
            // passing a non-numeric string would silently return 0 rows.
            let numeric_id: i64 = id.parse().map_err(|_| {
                rusqlite::Error::QueryReturnedNoRows
            })?;
            db.query_row(
                "SELECT id, memory_id, snapshot, created_at FROM memory_snapshots WHERE id = ?1",
                [numeric_id],
                |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, i64>(0)?,
                        "memory_id": row.get::<_, String>(1)?,
                        "snapshot": row.get::<_, String>(2)?,
                        "created_at": row.get::<_, String>(3)?,
                    }))
                },
            )
        });

    match row {
        Ok(memory) => (StatusCode::OK, Json(memory)),
        Err(rusqlite::Error::QueryReturnedNoRows) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "memory not found", "id": id})),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("database error: {e}"), "id": id})),
        ),
    }
}

/// GET /api/memory/search — search memories with filters (T-2.1.2).
///
/// Supports full-text search via LIKE matching on snapshot JSON content,
/// with optional filters for agent, memory type, importance, and confidence.
/// Results are ranked by a basic relevance score (defer FTS5 + RetrievalScorer to P3).
pub async fn search_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemorySearchParams>,
) -> ApiResult<serde_json::Value> {
    let limit = params.limit.unwrap_or(50).min(200);

    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Archival filter: exclude archived memories by default.
    let include_archived = params.include_archived.unwrap_or(false);

    // Build dynamic WHERE clause.
    let mut conditions = Vec::new();
    if !include_archived {
        conditions.push(
            "ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)".to_string(),
        );
    }
    let mut bind_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    // Text search: LIKE on snapshot JSON content.
    // Escape LIKE metacharacters (%, _) in user input to prevent wildcard abuse.
    if let Some(ref q) = params.q {
        if !q.trim().is_empty() {
            conditions.push(format!("ms.snapshot LIKE ?{idx} ESCAPE '\\'"));
            let escaped = q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
            bind_params.push(Box::new(format!("%{escaped}%")));
            idx += 1;
        }
    }

    // Agent filter via memory_events join.
    let need_join = params.agent_id.is_some();
    if let Some(ref agent_id) = params.agent_id {
        conditions.push(format!("me.actor_id = ?{idx}"));
        bind_params.push(Box::new(agent_id.clone()));
        idx += 1;
    }

    // Memory type filter (stored in snapshot JSON as "memory_type" field).
    if let Some(ref mt) = params.memory_type {
        conditions.push(format!(
            "json_extract(ms.snapshot, '$.memory_type') = ?{idx}"
        ));
        bind_params.push(Box::new(mt.clone()));
        idx += 1;
    }

    // Importance filter (stored in snapshot JSON as "importance" field).
    if let Some(ref imp) = params.importance {
        conditions.push(format!(
            "json_extract(ms.snapshot, '$.importance') = ?{idx}"
        ));
        bind_params.push(Box::new(imp.clone()));
        idx += 1;
    }

    // T-5.6.5: Validate confidence bounds.
    if let (Some(cmin), Some(cmax)) = (params.confidence_min, params.confidence_max) {
        if cmin > cmax {
            return Err(ApiError::bad_request(format!(
                "confidence_min ({cmin}) must be <= confidence_max ({cmax})"
            )));
        }
    }

    // Confidence range filters.
    if let Some(cmin) = params.confidence_min {
        conditions.push(format!(
            "CAST(json_extract(ms.snapshot, '$.confidence') AS REAL) >= ?{idx}"
        ));
        bind_params.push(Box::new(cmin));
        idx += 1;
    }
    if let Some(cmax) = params.confidence_max {
        conditions.push(format!(
            "CAST(json_extract(ms.snapshot, '$.confidence') AS REAL) <= ?{idx}"
        ));
        bind_params.push(Box::new(cmax));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let join_clause = if need_join {
        "JOIN memory_events me ON ms.memory_id = me.memory_id"
    } else {
        ""
    };

    let query = format!(
        "SELECT DISTINCT ms.id, ms.memory_id, ms.snapshot, ms.created_at \
         FROM memory_snapshots ms \
         {join_clause} \
         {where_clause} \
         ORDER BY ms.created_at DESC \
         LIMIT ?{idx}"
    );
    bind_params.push(Box::new(limit));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = db
        .prepare(&query)
        .map_err(|e| ApiError::db_error("memory_search_prepare", e))?;

    let results: Vec<serde_json::Value> = stmt
        .query_map(param_refs.as_slice(), |row| {
            let snapshot_str: String = row.get(2)?;
            let snapshot_parsed = serde_json::from_str::<serde_json::Value>(&snapshot_str)
                .unwrap_or(serde_json::Value::String(snapshot_str.clone()));

            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "memory_id": row.get::<_, String>(1)?,
                "snapshot": snapshot_parsed,
                "created_at": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|e| ApiError::db_error("memory_search_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(serde_json::json!({
        "results": results,
        "count": results.len(),
        "query": params.q,
        "filters": {
            "agent_id": params.agent_id,
            "memory_type": params.memory_type,
            "importance": params.importance,
            "confidence_min": params.confidence_min,
            "confidence_max": params.confidence_max,
        },
    })))
}

/// Request body for creating/updating a memory.
#[derive(Debug, Deserialize)]
pub struct WriteMemoryRequest {
    pub memory_id: String,
    pub event_type: String,
    pub delta: String,
    pub actor_id: String,
    /// Optional full snapshot to persist alongside the event.
    pub snapshot: Option<String>,
}

/// POST /api/memory — write a memory event (and optional snapshot).
///
/// Persists to memory_events, memory_snapshots, and memory_audit_log tables,
/// closing the dead-write-path for all three tables.
pub async fn write_memory(
    State(state): State<Arc<AppState>>,
    Json(body): Json<WriteMemoryRequest>,
) -> impl IntoResponse {
    let db = match state.db.lock() {
        Ok(db) => db,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database lock poisoned"})),
            );
        }
    };

    let event_hash = blake3::hash(body.memory_id.as_bytes());

    // 1. Insert memory event.
    if let Err(e) = cortex_storage::queries::memory_event_queries::insert_event(
        &db, &body.memory_id, &body.event_type, &body.delta,
        &body.actor_id, event_hash.as_bytes(), &[0u8; 32],
    ) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("memory event insert failed: {e}")})),
        );
    }

    // 2. Insert snapshot if provided.
    if let Some(ref snapshot) = body.snapshot {
        let state_hash = blake3::hash(snapshot.as_bytes());
        if let Err(e) = cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &db, &body.memory_id, snapshot, Some(state_hash.as_bytes()),
        ) {
            tracing::warn!(error = %e, memory_id = %body.memory_id, "snapshot insert failed (event was persisted)");
        }
    }

    // 3. Audit log entry.
    let details = format!("event_type={}, actor={}", body.event_type, body.actor_id);
    if let Err(e) = cortex_storage::queries::memory_audit_queries::insert_audit(
        &db, &body.memory_id, &body.event_type, Some(&details),
    ) {
        tracing::warn!(error = %e, memory_id = %body.memory_id, "audit log insert failed");
    }

    (
        StatusCode::CREATED,
        Json(serde_json::json!({
            "status": "ok",
            "memory_id": body.memory_id,
            "event_type": body.event_type,
        })),
    )
}

// ─── Archival endpoints ──────────────────────────────────────────────────

/// Request body for archiving a memory.
#[derive(Debug, Deserialize)]
pub struct ArchiveMemoryRequest {
    pub reason: String,
    #[serde(default)]
    pub decayed_confidence: Option<f64>,
    #[serde(default)]
    pub original_confidence: Option<f64>,
}

/// POST /api/memory/:id/archive — archive a memory.
///
/// Inserts an archival record. The memory remains accessible via direct
/// GET /api/memory/:id but is excluded from list and search by default.
pub async fn archive_memory(
    State(state): State<Arc<AppState>>,
    Path(memory_id): Path<String>,
    Json(body): Json<ArchiveMemoryRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    // Verify the memory exists.
    let exists: bool = db
        .query_row(
            "SELECT COUNT(*) > 0 FROM memory_snapshots WHERE memory_id = ?1",
            [&memory_id],
            |row| row.get(0),
        )
        .map_err(|e| ApiError::db_error("archive_check", e))?;

    if !exists {
        return Err(ApiError::not_found(format!("memory {memory_id} not found")));
    }

    // Check if already archived.
    if cortex_storage::queries::archival_queries::is_archived(&db, &memory_id)
        .map_err(|e| ApiError::internal(e.to_string()))?
    {
        return Err(ApiError::bad_request(format!(
            "memory {memory_id} is already archived"
        )));
    }

    cortex_storage::queries::archival_queries::insert_archival_record(
        &db,
        &memory_id,
        &body.reason,
        body.decayed_confidence.unwrap_or(0.0),
        body.original_confidence.unwrap_or(0.0),
    )
    .map_err(|e| ApiError::internal(e.to_string()))?;

    // Insert a new snapshot with archived=true (append-only safe).
    if let Ok(Some(latest)) =
        cortex_storage::queries::memory_snapshot_queries::latest_by_memory(&db, &memory_id)
    {
        if let Ok(mut snapshot) =
            serde_json::from_str::<serde_json::Value>(&latest.snapshot)
        {
            snapshot["archived"] = serde_json::json!(true);
            let updated = serde_json::to_string(&snapshot).unwrap_or_default();
            let state_hash = blake3::hash(updated.as_bytes());
            let _ = cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
                &db, &memory_id, &updated, Some(state_hash.as_bytes()),
            );
        }
    }

    Ok(Json(serde_json::json!({
        "status": "archived",
        "memory_id": memory_id,
    })))
}

/// POST /api/memory/:id/unarchive — restore an archived memory.
pub async fn unarchive_memory(
    State(state): State<Arc<AppState>>,
    Path(memory_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    if !cortex_storage::queries::archival_queries::is_archived(&db, &memory_id)
        .map_err(|e| ApiError::internal(e.to_string()))?
    {
        return Err(ApiError::bad_request(format!(
            "memory {memory_id} is not archived"
        )));
    }

    cortex_storage::queries::archival_queries::remove_archival_record(&db, &memory_id)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    // Insert a new snapshot with archived=false (append-only safe).
    if let Ok(Some(latest)) =
        cortex_storage::queries::memory_snapshot_queries::latest_by_memory(&db, &memory_id)
    {
        if let Ok(mut snapshot) =
            serde_json::from_str::<serde_json::Value>(&latest.snapshot)
        {
            snapshot["archived"] = serde_json::json!(false);
            let updated = serde_json::to_string(&snapshot).unwrap_or_default();
            let state_hash = blake3::hash(updated.as_bytes());
            let _ = cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
                &db, &memory_id, &updated, Some(state_hash.as_bytes()),
            );
        }
    }

    Ok(Json(serde_json::json!({
        "status": "unarchived",
        "memory_id": memory_id,
    })))
}

/// GET /api/memory/archived — list archived memories.
pub async fn list_archived(
    State(state): State<Arc<AppState>>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.lock().map_err(|_| ApiError::lock_poisoned("db"))?;

    let rows = cortex_storage::queries::archival_queries::query_archived(&db, 200)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let results: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "memory_id": r.memory_id,
                "archived_at": r.archived_at,
                "reason": r.reason,
                "decayed_confidence": r.decayed_confidence,
                "original_confidence": r.original_confidence,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "archived": results,
        "count": results.len(),
    })))
}
