//! Session API endpoints.
//!
//! Phase 2: Sessions are derived from itp_events grouped by session_id.
//! There is no dedicated sessions table — session_id is a column on
//! itp_events (created in v017_convergence_tables migration).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::auth::Claims;
use crate::api::error::{ApiError, ApiResult};
use crate::api::idempotency::execute_idempotent_json_mutation;
use crate::api::mutation::{
    error_response_with_idempotency, json_response_with_idempotency, write_mutation_audit_entry,
};
use crate::api::operation_context::OperationContext;
use crate::state::AppState;

const CREATE_BOOKMARK_ROUTE_TEMPLATE: &str = "/api/sessions/:id/bookmarks";
const DELETE_BOOKMARK_ROUTE_TEMPLATE: &str = "/api/sessions/:id/bookmarks/:bookmark_id";
const BRANCH_SESSION_ROUTE_TEMPLATE: &str = "/api/sessions/:id/branch";

fn session_actor(claims: Option<&Claims>) -> &str {
    claims
        .map(|claims| claims.sub.as_str())
        .unwrap_or("unknown")
}

/// Query parameters for session listing (F15 fix — was hardcoded LIMIT 100).
/// Phase 2 Task 3.7: Supports both page-based (legacy) and cursor-based pagination.
#[derive(Debug, Deserialize)]
pub struct SessionQueryParams {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
    /// Cursor-based pagination: last item's `last_event_at` from previous page.
    pub cursor: Option<String>,
    /// Items per page for cursor mode (default 50, max 200).
    pub limit: Option<u32>,
}

/// Query parameters for session events listing (T-2.1.1).
#[derive(Debug, Deserialize)]
pub struct SessionEventParams {
    pub offset: Option<u32>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuntimeSessionSummary {
    pub session_id: String,
    pub started_at: String,
    pub last_event_at: String,
    pub event_count: i64,
    pub agents: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuntimeSessionsPageResponse {
    pub sessions: Vec<RuntimeSessionSummary>,
    pub page: u32,
    pub page_size: u32,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct RuntimeSessionsCursorResponse {
    pub data: Vec<RuntimeSessionSummary>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: u64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(untagged)]
pub enum SessionListResponse {
    Page(RuntimeSessionsPageResponse),
    Cursor(RuntimeSessionsCursorResponse),
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
    pub offset: u32,
    pub limit: u32,
    pub chain_valid: bool,
    pub cumulative_cost: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionBookmark {
    pub id: String,
    pub event_index: u32,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SessionBookmarksResponse {
    pub bookmarks: Vec<SessionBookmark>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct CreateBookmarkResponse {
    pub id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DeleteBookmarkResponse {
    pub status: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct BranchSessionResponse {
    pub session_id: String,
    pub branched_from: String,
    pub events_copied: usize,
}

/// GET /api/sessions — list sessions derived from itp_events.
///
/// Groups itp_events by session_id, returning the first/last timestamp,
/// event count, and sender (agent) for each session.
///
/// Phase 2 Task 3.7: Supports cursor-based pagination (`?cursor=<last_event_at>&limit=50`)
/// alongside legacy page-based (`?page=1&page_size=50`).
pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SessionQueryParams>,
) -> impl IntoResponse {
    let db = match state.db.read() {
        Ok(db) => db,
        Err(e) => {
            tracing::error!(error = %e, "Failed to acquire DB read connection");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "database connection error"})),
            )
                .into_response();
        }
    };

    // Count total sessions.
    let total: u64 = match db.query_row(
        "SELECT COUNT(DISTINCT session_id) FROM itp_events",
        [],
        |row| row.get(0),
    ) {
        Ok(count) => count,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("count query failed: {e}")})),
            )
                .into_response();
        }
    };

    // Phase 2 Task 3.7: cursor-based pagination when `cursor` param is present.
    if params.cursor.is_some() || (params.page.is_none() && params.limit.is_some()) {
        let limit = params.limit.unwrap_or(50).min(200) as i64;

        let (query, query_params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) =
            if let Some(ref cursor) = params.cursor {
                (
                    "SELECT session_id, \
                            MIN(timestamp) as started_at, \
                            MAX(timestamp) as last_event_at, \
                            COUNT(*) as event_count, \
                            GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents \
                     FROM itp_events \
                     GROUP BY session_id \
                     HAVING last_event_at < ?1 \
                     ORDER BY last_event_at DESC \
                     LIMIT ?2"
                        .to_string(),
                    vec![
                        Box::new(cursor.clone()) as Box<dyn rusqlite::types::ToSql>,
                        Box::new(limit + 1),
                    ],
                )
            } else {
                (
                    "SELECT session_id, \
                            MIN(timestamp) as started_at, \
                            MAX(timestamp) as last_event_at, \
                            COUNT(*) as event_count, \
                            GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents \
                     FROM itp_events \
                     GROUP BY session_id \
                     ORDER BY last_event_at DESC \
                     LIMIT ?1"
                        .to_string(),
                    vec![Box::new(limit + 1) as Box<dyn rusqlite::types::ToSql>],
                )
            };

        let mut stmt = match db.prepare(&query) {
            Ok(stmt) => stmt,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
                )
                    .into_response();
            }
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            query_params.iter().map(|p| p.as_ref()).collect();

        let mut sessions = Vec::new();
        match stmt.query_map(param_refs.as_slice(), |row| {
            Ok(RuntimeSessionSummary {
                session_id: row.get::<_, String>(0)?,
                started_at: row.get::<_, String>(1)?,
                last_event_at: row.get::<_, String>(2)?,
                event_count: row.get::<_, i64>(3)?,
                agents: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            })
        }) {
            Ok(rows) => {
                for row in rows {
                    match row {
                        Ok(r) => sessions.push(r),
                        Err(e) => tracing::warn!(error = %e, "skipping malformed session row"),
                    }
                }
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query failed: {e}")})),
                )
                    .into_response();
            }
        }

        let has_more = sessions.len() > limit as usize;
        let data: Vec<RuntimeSessionSummary> = sessions.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            data.last().map(|session| session.last_event_at.clone())
        } else {
            None
        };

        return (
            StatusCode::OK,
            Json(SessionListResponse::Cursor(RuntimeSessionsCursorResponse {
                data,
                next_cursor,
                has_more,
                total_count: total,
            })),
        )
            .into_response();
    }

    // Legacy page-based pagination (backwards compatible).
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

    let mut stmt = match db.prepare(
        "SELECT session_id, \
                MIN(timestamp) as started_at, \
                MAX(timestamp) as last_event_at, \
                COUNT(*) as event_count, \
                GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) as agents \
         FROM itp_events \
         GROUP BY session_id \
         ORDER BY last_event_at DESC \
         LIMIT ?1 OFFSET ?2",
    ) {
        Ok(stmt) => stmt,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
            )
                .into_response();
        }
    };

    let mut sessions = Vec::new();
    match stmt.query_map(rusqlite::params![page_size, offset], |row| {
        Ok(RuntimeSessionSummary {
            session_id: row.get::<_, String>(0)?,
            started_at: row.get::<_, String>(1)?,
            last_event_at: row.get::<_, String>(2)?,
            event_count: row.get::<_, i64>(3)?,
            agents: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        })
    }) {
        Ok(rows) => {
            for row in rows {
                match row {
                    Ok(r) => sessions.push(r),
                    Err(e) => tracing::warn!(error = %e, "skipping malformed session row"),
                }
            }
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("query failed: {e}")})),
            )
                .into_response();
        }
    }

    (
        StatusCode::OK,
        Json(SessionListResponse::Page(RuntimeSessionsPageResponse {
            sessions,
            page,
            page_size,
            total,
        })),
    )
        .into_response()
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

/// Apply regex-based PII redaction to a string.
fn redact_pii(text: &str) -> String {
    let mut result = text.to_string();
    for (pattern, replacement) in PII_PATTERNS.iter() {
        result = pattern.replace_all(&result, *replacement).to_string();
    }
    result
}

/// GET /api/sessions/:id/events — list events for a specific session (T-2.1.1).
///
/// Returns paginated events with hash chain verification, cumulative cost,
/// and basic PII redaction on the content fields.
pub async fn session_events(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
    Query(params): Query<SessionEventParams>,
) -> ApiResult<SessionEventsResponse> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(100).min(500);

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("session_events", e))?;

    // Total event count for this session.
    let total: u32 = db
        .query_row(
            "SELECT COUNT(*) FROM itp_events WHERE session_id = ?1",
            [&session_id],
            |row| row.get(0),
        )
        .map_err(|e| ApiError::db_error("session_events_count", e))?;

    if total == 0 {
        return Err(ApiError::not_found(format!(
            "Session {session_id} not found or has no events"
        )));
    }

    // Fetch events ordered by sequence_number.
    let mut stmt = db
        .prepare(
            "SELECT id, event_type, sender, timestamp, sequence_number, \
                    content_hash, content_length, privacy_level, \
                    latency_ms, token_count, \
                    hex(event_hash) as event_hash_hex, \
                    hex(previous_hash) as prev_hash_hex, \
                    attributes \
             FROM itp_events \
             WHERE session_id = ?1 \
             ORDER BY sequence_number ASC \
             LIMIT ?2 OFFSET ?3",
        )
        .map_err(|e| ApiError::db_error("session_events_prepare", e))?;

    let mut events = Vec::new();
    let mut cumulative_cost = 0.0f64;
    let mut prev_hash: Option<String> = None;
    let mut chain_valid = true;

    let rows = stmt
        .query_map(rusqlite::params![&session_id, limit, offset], |row| {
            let event_hash_hex: String = row.get::<_, Option<String>>(10)?.unwrap_or_default();
            let prev_hash_hex: String = row.get::<_, Option<String>>(11)?.unwrap_or_default();
            let token_count: Option<i64> = row.get(9)?;
            let attributes: Option<String> = row.get(12)?;

            Ok((
                SessionEvent {
                    id: row.get::<_, String>(0)?,
                    event_type: row.get::<_, String>(1)?,
                    sender: row.get::<_, Option<String>>(2)?,
                    timestamp: row.get::<_, String>(3)?,
                    sequence_number: row.get::<_, i64>(4)?,
                    content_hash: row.get::<_, Option<String>>(5)?,
                    content_length: row.get::<_, Option<i64>>(6)?,
                    privacy_level: row.get::<_, String>(7)?,
                    latency_ms: row.get::<_, Option<i64>>(8)?,
                    token_count,
                    event_hash: event_hash_hex.clone(),
                    previous_hash: prev_hash_hex.clone(),
                    attributes: attributes
                        .as_deref()
                        .and_then(|a| serde_json::from_str::<serde_json::Value>(a).ok())
                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
                },
                event_hash_hex,
                prev_hash_hex,
                token_count.unwrap_or(0) as f64,
            ))
        })
        .map_err(|e| ApiError::db_error("session_events_query", e))?;

    for row in rows {
        match row {
            Ok((mut event, hash, event_prev_hash, tokens)) => {
                // Hash chain verification: previous event's hash == this event's previous_hash.
                if let Some(ref expected) = prev_hash {
                    if *expected != event_prev_hash {
                        chain_valid = false;
                    }
                }
                prev_hash = Some(hash);

                // Approximate cost: $0.003 per 1K tokens (rough Claude estimate).
                cumulative_cost += tokens * 0.000003;

                // T-5.6.6: PII redaction on attributes — never return unredacted data.
                let raw = event.attributes.to_string();
                if !raw.is_empty() {
                    let redacted = redact_pii(&raw);
                    match serde_json::from_str::<serde_json::Value>(&redacted) {
                        Ok(parsed) => {
                            event.attributes = parsed;
                        }
                        Err(e) => {
                            // PII redaction produced invalid JSON — replace entirely
                            // to prevent leaking unredacted data.
                            tracing::error!(
                                error = %e,
                                "PII redaction produced invalid JSON — replacing with placeholder"
                            );
                            event.attributes = serde_json::json!({
                                "_redaction_error": "[PII_REDACTION_FAILED]"
                            });
                        }
                    }
                }

                events.push(event);
            }
            Err(e) => tracing::warn!(error = %e, "skipping malformed session event row"),
        }
    }

    Ok(Json(SessionEventsResponse {
        session_id,
        events,
        total,
        offset,
        limit,
        chain_valid,
        cumulative_cost,
    }))
}

// ── Session Bookmarks (Phase 3, Task 3.9) ──────────────────────────────

#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct CreateBookmarkRequest {
    pub id: Option<String>,
    #[serde(rename = "eventIndex")]
    pub event_index: u32,
    pub label: String,
}

/// GET /api/sessions/:id/bookmarks — list bookmarks for a session.
pub async fn list_bookmarks(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> ApiResult<SessionBookmarksResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_bookmarks", e))?;

    let bookmarks: Vec<SessionBookmark> = match db.prepare(
        "SELECT id, event_index, label, created_at FROM session_bookmarks \
         WHERE session_id = ?1 ORDER BY event_index ASC",
    ) {
        Ok(mut stmt) => stmt
            .query_map([&session_id], |row| {
                Ok(SessionBookmark {
                    id: row.get::<_, String>(0)?,
                    event_index: row.get::<_, u32>(1)?,
                    label: row.get::<_, String>(2)?,
                    created_at: row.get::<_, String>(3)?,
                })
            })
            .map_err(|e| ApiError::db_error("list_bookmarks_query", e))?
            .filter_map(|r| r.ok())
            .collect(),
        Err(_) => vec![],
    };

    Ok(Json(SessionBookmarksResponse { bookmarks }))
}

/// POST /api/sessions/:id/bookmarks — create a bookmark.
pub async fn create_bookmark(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(session_id): Path<String>,
    Json(body): Json<CreateBookmarkRequest>,
) -> Response {
    let actor = session_actor(claims.as_ref().map(|claims| &claims.0));
    let bookmark_id = body
        .id
        .clone()
        .or_else(|| operation_context.operation_id.clone())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    let request_body = serde_json::json!({
        "session_id": session_id,
        "bookmark": body,
    });

    let db = state.db.write().await;
    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        CREATE_BOOKMARK_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            conn.execute(
                "INSERT INTO session_bookmarks (id, session_id, event_index, label) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![bookmark_id, session_id, body.event_index, body.label],
            ).map_err(|e| ApiError::db_error("create_bookmark", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::to_value(CreateBookmarkResponse {
                    id: bookmark_id.clone(),
                    status: "created".to_string(),
                })
                .unwrap_or_else(|_| serde_json::json!({ "id": bookmark_id, "status": "created" })),
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
                    "event_index": body.event_index,
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
    Path((_session_id, bookmark_id)): Path<(String, String)>,
) -> Response {
    let actor = session_actor(claims.as_ref().map(|claims| &claims.0));
    let request_body = serde_json::json!({ "bookmark_id": bookmark_id.clone() });
    let db = state.db.write().await;

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "DELETE",
        DELETE_BOOKMARK_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let deleted = conn
                .execute(
                    "DELETE FROM session_bookmarks WHERE id = ?1",
                    [&bookmark_id],
                )
                .map_err(|e| ApiError::db_error("delete_bookmark", e))?;

            if deleted == 0 {
                return Err(ApiError::not_found(format!(
                    "session bookmark {bookmark_id} not found"
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
                "session",
                "delete_session_bookmark",
                "info",
                actor,
                "deleted",
                serde_json::json!({ "bookmark_id": bookmark_id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/sessions/:id/branch — branch a new session from a checkpoint.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct BranchRequest {
    pub from_event_index: u32,
}

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
        "from_event_index": body.from_event_index,
    });

    let db = state.db.write().await;
    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        actor,
        "POST",
        BRANCH_SESSION_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let copied: usize = conn.execute(
                "INSERT INTO itp_events (id, session_id, event_type, sender, timestamp, sequence_number, \
                 content_hash, content_length, privacy_level, latency_ms, token_count, event_hash, previous_hash, attributes) \
                 SELECT hex(randomblob(16)), ?1, event_type, sender, timestamp, sequence_number, \
                 content_hash, content_length, privacy_level, latency_ms, token_count, event_hash, previous_hash, attributes \
                 FROM itp_events WHERE session_id = ?2 AND sequence_number <= ?3 \
                 ORDER BY sequence_number ASC",
                rusqlite::params![new_session_id, session_id, body.from_event_index],
            ).map_err(|e| ApiError::db_error("branch_session", e))?;

            Ok((
                StatusCode::CREATED,
                serde_json::to_value(BranchSessionResponse {
                    session_id: new_session_id.clone(),
                    branched_from: session_id.clone(),
                    events_copied: copied,
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "session_id": new_session_id,
                        "branched_from": session_id,
                        "events_copied": copied,
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
                    "from_event_index": body.from_event_index,
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
/// Frontend POSTs every 30s to indicate it's still consuming the SSE stream.
/// Backend tracks the timestamp; the SSE producer pauses when stale >90s.
/// Only existing Studio sessions or runtime sessions are allowed to refresh
/// liveness state.
pub async fn session_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(session_id): Path<String>,
) -> impl IntoResponse {
    match session_exists_for_heartbeat(&state, &session_id).await {
        Ok(true) => {}
        Ok(false) => {
            return ApiError::not_found(format!("session {session_id} not found")).into_response();
        }
        Err(error) => return error.into_response(),
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

    let runtime_session_exists: i64 = db
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM itp_events WHERE session_id = ?1 LIMIT 1)",
            rusqlite::params![session_id],
            |row| row.get(0),
        )
        .map_err(|error| ApiError::db_error("heartbeat_runtime_session_lookup", error))?;

    Ok(runtime_session_exists != 0)
}
