//! Session API endpoints.
//!
//! Runtime sessions are still derived from `itp_events`, but the public API
//! treats them as a first-class contract with stable cursor, detail, replay,
//! bookmark, and branch semantics.

use std::collections::BTreeSet;
use std::sync::Arc;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::OperationContext;
use crate::cost::tracker::CostTracker;
use crate::runtime_safety::parse_or_stable_uuid;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

const CREATE_BOOKMARK_ROUTE_TEMPLATE: &str = "/api/sessions/:id/bookmarks";
const DELETE_BOOKMARK_ROUTE_TEMPLATE: &str = "/api/sessions/:id/bookmarks/:bookmark_id";
const BRANCH_SESSION_ROUTE_TEMPLATE: &str = "/api/sessions/:id/branch";
fn session_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SessionQueryParams {
    /// Optional agent filter — include only sessions containing events sent by this agent.
    pub agent_id: Option<String>,
    /// Opaque cursor encoded from `(last_event_at, session_id)`.
    pub cursor: Option<String>,
    /// Items per page (default 50, max 200).
    pub limit: Option<u32>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SessionEventParams {
    /// Return events with sequence numbers strictly greater than this checkpoint.
    pub after_sequence_number: Option<i64>,
    /// Maximum number of events to return (default 100, max 500).
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct RuntimeSessionSummary {
    pub session_id: String,
    pub agent_ids: Vec<String>,
    pub started_at: String,
    pub last_event_at: String,
    pub event_count: i64,
    pub chain_valid: bool,
    pub cumulative_cost: f64,
    pub branched_from: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuntimeSessionsResponse {
    pub data: Vec<RuntimeSessionSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuntimeSessionDetailResponse {
    pub session: RuntimeSessionSummary,
    pub bookmark_count: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionEvent {
    pub id: String,
    pub event_type: String,
    pub sender: Option<String>,
    pub timestamp: String,
    pub sequence_number: i64,
    pub content_hash: Option<String>,
    pub content_length: Option<i64>,
    pub privacy_level: String,
    pub latency_ms: Option<i64>,
    pub token_count: Option<i64>,
    pub event_hash: String,
    pub previous_hash: String,
    pub attributes: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionEventsResponse {
    pub session_id: String,
    pub events: Vec<SessionEvent>,
    pub total: u32,
    pub limit: u32,
    pub has_more: bool,
    pub next_after_sequence_number: Option<i64>,
    pub chain_valid: bool,
    pub cumulative_cost: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionBookmark {
    pub id: String,
    pub session_id: String,
    pub sequence_number: i64,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionBookmarksResponse {
    pub bookmarks: Vec<SessionBookmark>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreateBookmarkResponse {
    pub bookmark: SessionBookmark,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeleteBookmarkResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BranchSessionResponse {
    pub session: RuntimeSessionSummary,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateBookmarkRequest {
    pub id: Option<String>,
    pub sequence_number: i64,
    pub label: String,
}

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BranchRequest {
    pub from_sequence_number: i64,
}

#[derive(Debug, Serialize, Deserialize)]
struct SessionListCursor {
    last_event_at: String,
    session_id: String,
}

#[derive(Debug, Clone)]
struct RuntimeSessionSummaryRow {
    session_id: String,
    started_at: String,
    last_event_at: String,
    event_count: i64,
    agent_ids_csv: Option<String>,
    branched_from: Option<String>,
}

#[derive(Debug)]
struct SessionMetrics {
    chain_valid: bool,
    cumulative_cost: f64,
}

/// GET /api/sessions — list runtime sessions with deterministic cursor ordering.
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionQueryParams>,
) -> ApiResult<RuntimeSessionsResponse> {
    let db = state.db.read()?;
    let cursor = params
        .cursor
        .as_deref()
        .map(parse_session_list_cursor)
        .transpose()?;
    let limit = params.limit.unwrap_or(50).min(200) as i64;
    let total_count: u64 = if let Some(agent_id) = params.agent_id.as_ref() {
        db.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM itp_events WHERE sender = ?1",
            [agent_id],
            |row| row.get(0),
        )?
    } else {
        db.query_row(
            "SELECT COUNT(DISTINCT session_id) FROM itp_events",
            [],
            |row| row.get(0),
        )?
    };

    let mut rows = Vec::new();
    match (cursor, params.agent_id.as_ref()) {
        (Some(cursor), Some(agent_id)) => {
            let mut stmt = db.prepare(
                "SELECT e.session_id,
                        MIN(e.timestamp) AS started_at,
                        MAX(e.timestamp) AS last_event_at,
                        COUNT(*) AS event_count,
                        GROUP_CONCAT(DISTINCT COALESCE(e.sender, 'unknown')) AS agent_ids_csv,
                        sb.source_session_id AS branched_from
                 FROM itp_events AS e
                 LEFT JOIN session_branches AS sb
                   ON sb.session_id = e.session_id
                 WHERE e.session_id IN (
                     SELECT DISTINCT session_id FROM itp_events WHERE sender = ?1
                 )
                 GROUP BY e.session_id, sb.source_session_id
                 HAVING MAX(e.timestamp) < ?2
                     OR (MAX(e.timestamp) = ?2 AND e.session_id < ?3)
                 ORDER BY MAX(e.timestamp) DESC, e.session_id DESC
                 LIMIT ?4",
            )?;
            let mapped = stmt.query_map(
                params![agent_id, cursor.last_event_at, cursor.session_id, limit + 1],
                runtime_session_summary_row_from_sql,
            )?;
            for row in mapped {
                rows.push(row?);
            }
        }
        (Some(cursor), None) => {
            let mut stmt = db.prepare(
                "SELECT e.session_id,
                        MIN(e.timestamp) AS started_at,
                        MAX(e.timestamp) AS last_event_at,
                        COUNT(*) AS event_count,
                        GROUP_CONCAT(DISTINCT COALESCE(e.sender, 'unknown')) AS agent_ids_csv,
                        sb.source_session_id AS branched_from
                 FROM itp_events AS e
                 LEFT JOIN session_branches AS sb
                   ON sb.session_id = e.session_id
                 GROUP BY e.session_id, sb.source_session_id
                 HAVING MAX(e.timestamp) < ?1
                     OR (MAX(e.timestamp) = ?1 AND e.session_id < ?2)
                 ORDER BY MAX(e.timestamp) DESC, e.session_id DESC
                 LIMIT ?3",
            )?;
            let mapped = stmt.query_map(
                params![cursor.last_event_at, cursor.session_id, limit + 1],
                runtime_session_summary_row_from_sql,
            )?;
            for row in mapped {
                rows.push(row?);
            }
        }
        (None, Some(agent_id)) => {
            let mut stmt = db.prepare(
                "SELECT e.session_id,
                        MIN(e.timestamp) AS started_at,
                        MAX(e.timestamp) AS last_event_at,
                        COUNT(*) AS event_count,
                        GROUP_CONCAT(DISTINCT COALESCE(e.sender, 'unknown')) AS agent_ids_csv,
                        sb.source_session_id AS branched_from
                 FROM itp_events AS e
                 LEFT JOIN session_branches AS sb
                   ON sb.session_id = e.session_id
                 WHERE e.session_id IN (
                     SELECT DISTINCT session_id FROM itp_events WHERE sender = ?1
                 )
                 GROUP BY e.session_id, sb.source_session_id
                 ORDER BY MAX(e.timestamp) DESC, e.session_id DESC
                 LIMIT ?2",
            )?;
            let mapped = stmt.query_map(
                params![agent_id, limit + 1],
                runtime_session_summary_row_from_sql,
            )?;
            for row in mapped {
                rows.push(row?);
            }
        }
        (None, None) => {
            let mut stmt = db.prepare(
                "SELECT e.session_id,
                        MIN(e.timestamp) AS started_at,
                        MAX(e.timestamp) AS last_event_at,
                        COUNT(*) AS event_count,
                        GROUP_CONCAT(DISTINCT COALESCE(e.sender, 'unknown')) AS agent_ids_csv,
                        sb.source_session_id AS branched_from
                 FROM itp_events AS e
                 LEFT JOIN session_branches AS sb
                   ON sb.session_id = e.session_id
                 GROUP BY e.session_id, sb.source_session_id
                 ORDER BY MAX(e.timestamp) DESC, e.session_id DESC
                 LIMIT ?1",
            )?;
            let mapped =
                stmt.query_map(params![limit + 1], runtime_session_summary_row_from_sql)?;
            for row in mapped {
                rows.push(row?);
            }
        }
    }

    let has_more = rows.len() > limit as usize;
    rows.truncate(limit as usize);

    let mut data = Vec::with_capacity(rows.len());
    for row in &rows {
        data.push(hydrate_runtime_session_summary(
            &db,
            &state.cost_tracker,
            row,
        )?);
    }

    let next_cursor = if has_more {
        rows.last().map(encode_session_list_cursor)
    } else {
        None
    };

    Ok(Json(RuntimeSessionsResponse {
        data,
        next_cursor,
        has_more,
        total_count,
    }))
}

/// GET /api/sessions/:id — load canonical runtime session summary data.
pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> ApiResult<RuntimeSessionDetailResponse> {
    let db = state.db.read()?;
    let session = load_runtime_session_summary(&db, &state.cost_tracker, &session_id)?
        .ok_or_else(|| ApiError::not_found(format!("runtime session {session_id}")))?;
    let bookmark_count: u64 = db.query_row(
        "SELECT COUNT(*) FROM session_bookmarks WHERE session_id = ?1",
        [session_id],
        |row| row.get(0),
    )?;

    Ok(Json(RuntimeSessionDetailResponse {
        session,
        bookmark_count,
    }))
}

/// PII redaction patterns (T-2.1.1 — regex-based, NER deferred to P3).
static PII_PATTERNS: std::sync::LazyLock<Vec<(regex::Regex, &'static str)>> =
    std::sync::LazyLock::new(|| {
        vec![
            (
                regex::Regex::new(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Z|a-z]{2,}\b").unwrap(),
                "[EMAIL]",
            ),
            (
                regex::Regex::new(r"\b\d{3}[-.]?\d{3}[-.]?\d{4}\b").unwrap(),
                "[PHONE]",
            ),
            (
                regex::Regex::new(r"\b\d{3}-\d{2}-\d{4}\b").unwrap(),
                "[SSN]",
            ),
        ]
    });

fn redact_pii(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in PII_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// GET /api/sessions/:id/events — list runtime session events by sequence number.
pub async fn session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<SessionEventParams>,
) -> ApiResult<SessionEventsResponse> {
    let db = state.db.read()?;
    let total: u32 = db.query_row(
        "SELECT COUNT(*) FROM itp_events WHERE session_id = ?1",
        [&session_id],
        |row| row.get(0),
    )?;
    if total == 0 {
        return Err(ApiError::not_found(format!("runtime session {session_id}")));
    }

    let after_sequence_number = params.after_sequence_number.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(500) as i64;
    let mut stmt = db.prepare(
        "SELECT id, event_type, sender, timestamp, sequence_number,
                content_hash, content_length, privacy_level,
                latency_ms, token_count,
                hex(event_hash) AS event_hash_hex,
                hex(previous_hash) AS prev_hash_hex,
                attributes
         FROM itp_events
         WHERE session_id = ?1
           AND sequence_number > ?2
         ORDER BY sequence_number ASC
         LIMIT ?3",
    )?;

    let mut events = Vec::new();
    let rows = stmt.query_map(
        params![&session_id, after_sequence_number, limit + 1],
        |row| {
            let event_hash_hex: String = row.get::<_, Option<String>>(10)?.unwrap_or_default();
            let prev_hash_hex: String = row.get::<_, Option<String>>(11)?.unwrap_or_default();
            let attributes: Option<String> = row.get(12)?;
            let mut event = SessionEvent {
                id: row.get::<_, String>(0)?,
                event_type: row.get::<_, String>(1)?,
                sender: row.get::<_, Option<String>>(2)?,
                timestamp: row.get::<_, String>(3)?,
                sequence_number: row.get::<_, i64>(4)?,
                content_hash: row.get::<_, Option<String>>(5)?,
                content_length: row.get::<_, Option<i64>>(6)?,
                privacy_level: row.get::<_, String>(7)?,
                latency_ms: row.get::<_, Option<i64>>(8)?,
                token_count: row.get::<_, Option<i64>>(9)?,
                event_hash: event_hash_hex,
                previous_hash: prev_hash_hex,
                attributes: attributes
                    .as_deref()
                    .and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
                    .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
            };

            let raw = event.attributes.to_string();
            if !raw.is_empty() {
                let redacted = redact_pii(&raw);
                event.attributes = serde_json::from_str::<serde_json::Value>(&redacted)
                    .unwrap_or_else(|_| {
                        serde_json::json!({
                            "_redaction_error": "[PII_REDACTION_FAILED]"
                        })
                    });
            }

            Ok(event)
        },
    )?;

    for row in rows {
        events.push(row?);
    }

    let has_more = events.len() > limit as usize;
    events.truncate(limit as usize);
    let next_after_sequence_number = if has_more {
        events.last().map(|event| event.sequence_number)
    } else {
        None
    };
    let metrics = compute_session_metrics(&db, &state.cost_tracker, &session_id)?;

    Ok(Json(SessionEventsResponse {
        session_id,
        events,
        total,
        limit: limit as u32,
        has_more,
        next_after_sequence_number,
        chain_valid: metrics.chain_valid,
        cumulative_cost: metrics.cumulative_cost,
    }))
}

/// GET /api/sessions/:id/bookmarks — list bookmarks for a session.
pub async fn list_bookmarks(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> ApiResult<SessionBookmarksResponse> {
    let db = state.db.read()?;
    ensure_runtime_session_exists(&db, &session_id)?;

    let mut stmt = db.prepare(
        "SELECT id, session_id, sequence_number, label, created_at
         FROM session_bookmarks
         WHERE session_id = ?1
         ORDER BY sequence_number ASC, created_at ASC",
    )?;
    let rows = stmt.query_map([&session_id], session_bookmark_from_sql)?;
    let mut bookmarks = Vec::new();
    for row in rows {
        bookmarks.push(row?);
    }

    Ok(Json(SessionBookmarksResponse { bookmarks }))
}

/// POST /api/sessions/:id/bookmarks — create a bookmark at a concrete sequence number.
pub async fn create_bookmark(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(session_id): Path<String>,
    Json(body): Json<CreateBookmarkRequest>,
) -> Response {
    let actor = session_actor(claims.as_ref().map(|claims| &claims.0));
    let session_id_for_body = session_id.clone();
    let bookmark_id = body
        .id
        .clone()
        .or_else(|| operation_context.operation_id.clone())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let request_body = serde_json::json!({
        "session_id": session_id_for_body,
        "bookmark": body,
    });
    let sequence_number = body.sequence_number;
    let label = body.label.clone();
    let bookmark_id_for_insert = bookmark_id.clone();
    let session_id_for_insert = session_id.clone();

    let db = state.db.write().await;
    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_BOOKMARK_ROUTE_TEMPLATE,
        &request_body,
        move |conn| {
            ensure_runtime_session_exists(conn, &session_id_for_insert)?;
            ensure_session_sequence_exists(conn, &session_id_for_insert, sequence_number)?;

            conn.execute(
                "INSERT INTO session_bookmarks (id, session_id, sequence_number, label)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    bookmark_id_for_insert,
                    session_id_for_insert,
                    sequence_number,
                    label
                ],
            )?;

            let bookmark =
                load_session_bookmark(conn, &session_id_for_insert, &bookmark_id_for_insert)?
                    .ok_or_else(|| ApiError::internal("bookmark insert did not persist"))?;
            Ok((
                StatusCode::CREATED,
                serde_json::to_value(CreateBookmarkResponse { bookmark }).unwrap_or_else(|_| {
                    serde_json::json!({
                        "bookmark": {
                            "id": bookmark_id_for_insert,
                            "session_id": session_id_for_insert,
                            "sequence_number": sequence_number,
                            "label": label,
                        }
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &session_id,
                "create_session_bookmark",
                "info",
                actor,
                "created",
                serde_json::json!({
                    "session_id": session_id,
                    "bookmark_id": bookmark_id,
                    "sequence_number": sequence_number,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// DELETE /api/sessions/:id/bookmarks/:bookmark_id — remove a bookmark.
pub async fn delete_bookmark(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path((session_id, bookmark_id)): Path<(String, String)>,
) -> Response {
    let actor = session_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({
        "session_id": session_id,
        "bookmark_id": bookmark_id,
    });
    let session_id_for_delete = session_id.clone();
    let bookmark_id_for_delete = bookmark_id.clone();
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_BOOKMARK_ROUTE_TEMPLATE,
        &request_body,
        move |conn| {
            let deleted = conn.execute(
                "DELETE FROM session_bookmarks WHERE id = ?1 AND session_id = ?2",
                params![bookmark_id_for_delete, session_id_for_delete],
            )?;
            if deleted == 0 {
                return Err(ApiError::not_found(format!(
                    "bookmark {bookmark_id_for_delete} for session {session_id_for_delete}"
                )));
            }

            Ok((
                StatusCode::OK,
                serde_json::to_value(DeleteBookmarkResponse {
                    status: "deleted".to_string(),
                })
                .unwrap_or_else(|_| serde_json::json!({ "status": "deleted" })),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &session_id,
                "delete_session_bookmark",
                "info",
                actor,
                "deleted",
                serde_json::json!({
                    "session_id": session_id,
                    "bookmark_id": bookmark_id,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/sessions/:id/branch — branch a new session from a persisted checkpoint.
pub async fn branch_session(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(session_id): Path<String>,
    Json(body): Json<BranchRequest>,
) -> Response {
    let actor = session_actor(claims.as_ref().map(|claims| &claims.0));
    let new_session_id = operation_context
        .operation_id
        .clone()
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let request_body = serde_json::json!({
        "session_id": session_id,
        "from_sequence_number": body.from_sequence_number,
    });
    let from_sequence_number = body.from_sequence_number;
    let new_session_id_for_insert = new_session_id.clone();
    let session_id_for_insert = session_id.clone();
    let cost_tracker = Arc::clone(&state.cost_tracker);

    let db = state.db.write().await;
    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        BRANCH_SESSION_ROUTE_TEMPLATE,
        &request_body,
        move |conn| {
            ensure_runtime_session_exists(conn, &session_id_for_insert)?;
            ensure_session_sequence_exists(conn, &session_id_for_insert, from_sequence_number)?;

            let copied = conn.execute(
                "INSERT INTO itp_events (
                     id,
                     session_id,
                     event_type,
                     sender,
                     timestamp,
                     sequence_number,
                     content_hash,
                     content_length,
                     privacy_level,
                     latency_ms,
                     token_count,
                     event_hash,
                     previous_hash,
                     attributes
                 )
                 SELECT hex(randomblob(16)),
                        ?1,
                        event_type,
                        sender,
                        timestamp,
                        sequence_number,
                        content_hash,
                        content_length,
                        privacy_level,
                        latency_ms,
                        token_count,
                        event_hash,
                        previous_hash,
                        attributes
                 FROM itp_events
                 WHERE session_id = ?2 AND sequence_number <= ?3
                 ORDER BY sequence_number ASC",
                params![
                    new_session_id_for_insert,
                    session_id_for_insert,
                    from_sequence_number
                ],
            )?;

            if copied == 0 {
                return Err(ApiError::bad_request(format!(
                    "branch checkpoint {from_sequence_number} produced no events"
                )));
            }

            conn.execute(
                "INSERT INTO session_branches (session_id, source_session_id, source_sequence_number)
                 VALUES (?1, ?2, ?3)",
                params![
                    new_session_id_for_insert,
                    session_id_for_insert,
                    from_sequence_number
                ],
            )?;

            let session =
                load_runtime_session_summary(conn, &cost_tracker, &new_session_id_for_insert)?
                    .ok_or_else(|| ApiError::internal("branched session was not materialized"))?;
            Ok((
                StatusCode::CREATED,
                serde_json::to_value(BranchSessionResponse { session }).unwrap_or_else(|_| {
                    serde_json::json!({
                        "session": {
                            "session_id": new_session_id_for_insert,
                            "branched_from": session_id_for_insert,
                            "event_count": copied,
                        }
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                &session_id,
                "branch_session",
                "info",
                actor,
                "branched",
                serde_json::json!({
                    "session_id": session_id,
                    "branched_session_id": new_session_id,
                    "from_sequence_number": from_sequence_number,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// WP9-L: Client heartbeat endpoint.
pub async fn session_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    let active_runtime_session = if let (Some(tracker), Ok(session_uuid)) = (
        state.itp_session_tracker.as_ref(),
        uuid::Uuid::parse_str(&session_id),
    ) {
        tracker.touch(session_uuid).await.is_some()
    } else {
        false
    };

    if !active_runtime_session {
        match session_exists_for_heartbeat(&state, &session_id).await {
            Ok(true) => {}
            Ok(false) => {
                return ApiError::not_found(format!("session {session_id} not found"))
                    .into_response();
            }
            Err(error) => return error.into_response(),
        }
    }

    state
        .client_heartbeats
        .insert(session_id.clone(), std::time::Instant::now());
    (StatusCode::NO_CONTENT, "").into_response()
}

async fn session_exists_for_heartbeat(
    state: &AppState,
    session_id: &str,
) -> Result<bool, ApiError> {
    let db = state.db.read()?;
    let studio_session = cortex_storage::queries::studio_chat_queries::get_session(&db, session_id)
        .map_err(|error| ApiError::db_error("heartbeat_studio_session_lookup", error))?;
    if studio_session.is_some() {
        return Ok(true);
    }

    let runtime_session_exists: i64 = db.query_row(
        "SELECT EXISTS(SELECT 1 FROM itp_events WHERE session_id = ?1 LIMIT 1)",
        params![session_id],
        |row| row.get(0),
    )?;
    Ok(runtime_session_exists != 0)
}

fn runtime_session_summary_row_from_sql(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RuntimeSessionSummaryRow> {
    Ok(RuntimeSessionSummaryRow {
        session_id: row.get::<_, String>(0)?,
        started_at: row.get::<_, String>(1)?,
        last_event_at: row.get::<_, String>(2)?,
        event_count: row.get::<_, i64>(3)?,
        agent_ids_csv: row.get::<_, Option<String>>(4)?,
        branched_from: row.get::<_, Option<String>>(5)?,
    })
}

fn session_bookmark_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<SessionBookmark> {
    Ok(SessionBookmark {
        id: row.get::<_, String>(0)?,
        session_id: row.get::<_, String>(1)?,
        sequence_number: row.get::<_, i64>(2)?,
        label: row.get::<_, String>(3)?,
        created_at: row.get::<_, String>(4)?,
    })
}

fn parse_session_list_cursor(value: &str) -> Result<SessionListCursor, ApiError> {
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| ApiError::bad_request("invalid runtime session cursor"))?;
    serde_json::from_slice::<SessionListCursor>(&decoded)
        .map_err(|_| ApiError::bad_request("invalid runtime session cursor"))
}

fn encode_session_list_cursor(row: &RuntimeSessionSummaryRow) -> String {
    let cursor = SessionListCursor {
        last_event_at: row.last_event_at.clone(),
        session_id: row.session_id.clone(),
    };
    let encoded = serde_json::to_vec(&cursor).unwrap_or_default();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(encoded)
}

fn normalize_agent_ids(value: Option<String>) -> Vec<String> {
    let mut deduped = BTreeSet::new();
    if let Some(value) = value {
        for agent in value
            .split(',')
            .map(str::trim)
            .filter(|agent| !agent.is_empty())
        {
            deduped.insert(agent.to_string());
        }
    }
    deduped.into_iter().collect()
}

fn ensure_runtime_session_exists(conn: &Connection, session_id: &str) -> Result<(), ApiError> {
    let exists: i64 = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM itp_events WHERE session_id = ?1 LIMIT 1)",
        params![session_id],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(ApiError::not_found(format!("runtime session {session_id}")));
    }
    Ok(())
}

fn ensure_session_sequence_exists(
    conn: &Connection,
    session_id: &str,
    sequence_number: i64,
) -> Result<(), ApiError> {
    let exists: i64 = conn.query_row(
        "SELECT EXISTS(
             SELECT 1
             FROM itp_events
             WHERE session_id = ?1 AND sequence_number = ?2
             LIMIT 1
         )",
        params![session_id, sequence_number],
        |row| row.get(0),
    )?;
    if exists == 0 {
        return Err(ApiError::bad_request(format!(
            "sequence_number {sequence_number} not found in session {session_id}"
        )));
    }
    Ok(())
}

fn load_session_bookmark(
    conn: &Connection,
    session_id: &str,
    bookmark_id: &str,
) -> Result<Option<SessionBookmark>, ApiError> {
    conn.query_row(
        "SELECT id, session_id, sequence_number, label, created_at
         FROM session_bookmarks
         WHERE session_id = ?1 AND id = ?2",
        params![session_id, bookmark_id],
        session_bookmark_from_sql,
    )
    .optional()
    .map_err(ApiError::from)
}

fn load_runtime_session_summary(
    conn: &Connection,
    cost_tracker: &CostTracker,
    session_id: &str,
) -> Result<Option<RuntimeSessionSummary>, ApiError> {
    let row = conn
        .query_row(
            "SELECT e.session_id,
                    MIN(e.timestamp) AS started_at,
                    MAX(e.timestamp) AS last_event_at,
                    COUNT(*) AS event_count,
                    GROUP_CONCAT(DISTINCT COALESCE(e.sender, 'unknown')) AS agent_ids_csv,
                    sb.source_session_id AS branched_from
             FROM itp_events AS e
             LEFT JOIN session_branches AS sb
               ON sb.session_id = e.session_id
             WHERE e.session_id = ?1
             GROUP BY e.session_id, sb.source_session_id",
            params![session_id],
            runtime_session_summary_row_from_sql,
        )
        .optional()?;

    row.map(|row| hydrate_runtime_session_summary(conn, cost_tracker, &row))
        .transpose()
}

fn hydrate_runtime_session_summary(
    conn: &Connection,
    cost_tracker: &CostTracker,
    row: &RuntimeSessionSummaryRow,
) -> Result<RuntimeSessionSummary, ApiError> {
    let metrics = compute_session_metrics(conn, cost_tracker, &row.session_id)?;
    Ok(RuntimeSessionSummary {
        session_id: row.session_id.clone(),
        agent_ids: normalize_agent_ids(row.agent_ids_csv.clone()),
        started_at: row.started_at.clone(),
        last_event_at: row.last_event_at.clone(),
        event_count: row.event_count,
        chain_valid: metrics.chain_valid,
        cumulative_cost: metrics.cumulative_cost,
        branched_from: row.branched_from.clone(),
    })
}

fn compute_session_metrics(
    conn: &Connection,
    cost_tracker: &CostTracker,
    session_id: &str,
) -> Result<SessionMetrics, ApiError> {
    let mut chain_valid = true;
    let mut previous_hash: Option<String> = None;
    let mut stmt = conn.prepare(
        "SELECT hex(event_hash) AS event_hash_hex, hex(previous_hash) AS previous_hash_hex
         FROM itp_events
         WHERE session_id = ?1
         ORDER BY sequence_number ASC",
    )?;
    let rows = stmt.query_map(params![session_id], |row| {
        Ok((
            row.get::<_, Option<String>>(0)?.unwrap_or_default(),
            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
        ))
    })?;
    for row in rows {
        let (event_hash, previous_event_hash) = row?;
        if let Some(expected) = &previous_hash {
            if expected != &previous_event_hash {
                chain_valid = false;
                break;
            }
        }
        previous_hash = Some(event_hash);
    }

    Ok(SessionMetrics {
        chain_valid,
        cumulative_cost: cost_tracker
            .get_session_total(parse_or_stable_uuid(session_id, "runtime-session")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compute_session_metrics_uses_tracked_total_for_legacy_session_ids() {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        conn.execute_batch(
            "CREATE TABLE itp_events (
                session_id TEXT NOT NULL,
                sequence_number INTEGER NOT NULL,
                event_hash BLOB NOT NULL,
                previous_hash BLOB NOT NULL
            );",
        )
        .expect("create itp_events");
        conn.execute(
            "INSERT INTO itp_events (session_id, sequence_number, event_hash, previous_hash)
             VALUES (?1, ?2, X'AA', X'')",
            params!["legacy-session", 1],
        )
        .expect("insert first event");
        conn.execute(
            "INSERT INTO itp_events (session_id, sequence_number, event_hash, previous_hash)
             VALUES (?1, ?2, X'BB', X'AA')",
            params!["legacy-session", 2],
        )
        .expect("insert second event");

        let tracker = CostTracker::new();
        tracker.record(
            parse_or_stable_uuid("agent-legacy", "runtime-agent"),
            parse_or_stable_uuid("legacy-session", "runtime-session"),
            1.75,
            false,
        );

        let metrics =
            compute_session_metrics(&conn, &tracker, "legacy-session").expect("session metrics");

        assert!(metrics.chain_valid);
        assert!((metrics.cumulative_cost - 1.75).abs() < f64::EPSILON);
    }
}
