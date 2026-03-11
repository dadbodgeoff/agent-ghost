//! Memory API endpoints (Req 25 AC1-2).
//!
//! Phase 2: Wired to cortex-storage memory_snapshots table
//! (created in v016_convergence_safety migration).

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Extension;
use axum::Json;
use chrono::{DateTime, Utc};
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::{BaseMemory, Importance};
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

const WRITE_MEMORY_ROUTE_TEMPLATE: &str = "/api/memory";
const ARCHIVE_MEMORY_ROUTE_TEMPLATE: &str = "/api/memory/:id/archive";
const UNARCHIVE_MEMORY_ROUTE_TEMPLATE: &str = "/api/memory/:id/unarchive";
const LATEST_MEMORY_SNAPSHOTS_JOIN: &str = "
 JOIN (
     SELECT memory_id, MAX(id) AS max_id
     FROM memory_snapshots
     GROUP BY memory_id
 ) latest ON latest.max_id = ms.id";

fn memory_actor(claims: Option<&Claims>, fallback: Option<&str>) -> String {
    claims
        .and_then(|claims| {
            let subject = claims.sub.trim();
            if subject.is_empty() || subject == "anonymous" || subject == "unknown" {
                None
            } else {
                Some(claims.sub.clone())
            }
        })
        .or_else(|| fallback.map(ToOwned::to_owned))
        .unwrap_or_else(|| "anonymous".to_string())
}

fn normalize_memory_type_filter(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let canonical = match trimmed.to_ascii_lowercase().as_str() {
        "agentgoal" | "agent_goal" => "AgentGoal",
        "agentreflection" | "agent_reflection" | "reflection" => "AgentReflection",
        "convergenceevent" | "convergence_event" => "ConvergenceEvent",
        "boundaryviolation" | "boundary_violation" => "BoundaryViolation",
        "proposalrecord" | "proposal_record" => "ProposalRecord",
        "simulationresult" | "simulation_result" => "SimulationResult",
        "interventionplan" | "intervention_plan" => "InterventionPlan",
        "attachmentindicator" | "attachment_indicator" => "AttachmentIndicator",
        "patternrationale" | "pattern_rationale" => "PatternRationale",
        "constraintoverride" | "constraint_override" => "ConstraintOverride",
        "decisioncontext" | "decision_context" => "DecisionContext",
        "codesmell" | "code_smell" => "CodeSmell",
        "core" => "Core",
        "tribal" => "Tribal",
        "procedural" => "Procedural",
        "semantic" => "Semantic",
        "episodic" => "Episodic",
        "decision" => "Decision",
        "insight" => "Insight",
        "reference" => "Reference",
        "preference" => "Preference",
        "conversation" => "Conversation",
        "feedback" => "Feedback",
        "skill" => "Skill",
        "goal" => "Goal",
        "relationship" => "Relationship",
        "context" => "Context",
        "observation" => "Observation",
        "hypothesis" => "Hypothesis",
        "experiment" => "Experiment",
        "lesson" => "Lesson",
        _ => return Some(trimmed.to_string()),
    };

    Some(canonical.to_string())
}

fn normalize_importance_filter(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let canonical = match trimmed.to_ascii_lowercase().as_str() {
        "critical" => "Critical",
        "high" => "High",
        "medium" | "normal" => "Normal",
        "low" => "Low",
        "trivial" => "Trivial",
        _ => return Some(trimmed.to_string()),
    };

    Some(canonical.to_string())
}

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

/// Query parameters for the derived memory graph view.
#[derive(Debug, Deserialize)]
pub struct MemoryGraphParams {
    pub agent_id: Option<String>,
    pub limit: Option<u32>,
    /// Include archived memories in the graph (default: false).
    pub include_archived: Option<bool>,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, ToSchema)]
pub struct MemoryGraphNode {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub importance: f64,
    #[serde(rename = "decayFactor")]
    pub decay_factor: f64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, ToSchema)]
pub struct MemoryGraphEdge {
    pub source: String,
    pub target: String,
    pub relationship: String,
    pub strength: f64,
}

#[derive(Debug, Clone, serde::Serialize, PartialEq, ToSchema)]
pub struct MemoryGraphResponse {
    pub nodes: Vec<MemoryGraphNode>,
    pub edges: Vec<MemoryGraphEdge>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemoryEntry {
    pub id: i64,
    pub memory_id: String,
    pub snapshot: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ListMemoriesResponse {
    pub memories: Vec<MemoryEntry>,
    pub page: u32,
    pub page_size: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemorySearchResultEntry {
    pub id: i64,
    pub memory_id: String,
    pub snapshot: serde_json::Value,
    pub created_at: String,
    pub score: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemorySearchFilters {
    pub agent_id: Option<String>,
    pub memory_type: Option<String>,
    pub importance: Option<String>,
    pub confidence_min: Option<f64>,
    pub confidence_max: Option<f64>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct SearchMemoriesResponse {
    pub results: Vec<MemorySearchResultEntry>,
    pub count: usize,
    pub query: Option<String>,
    pub search_mode: String,
    pub filters: MemorySearchFilters,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutedMemorySearch {
    pub response: SearchMemoriesResponse,
    pub total_matches: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArchivedMemoryEntry {
    pub memory_id: String,
    pub archived_at: String,
    pub reason: String,
    pub decayed_confidence: f64,
    pub original_confidence: f64,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ArchivedMemoryListResponse {
    pub archived: Vec<ArchivedMemoryEntry>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct MemoryArchiveStatusResponse {
    pub status: String,
    pub memory_id: String,
}

/// GET /api/memory — list memory snapshots with optional agent_id filter.
pub async fn list_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemoryQueryParams>,
) -> impl IntoResponse {
    let page = params.page.unwrap_or(1);
    let page_size = params.page_size.unwrap_or(50).min(200);
    let offset = (page.saturating_sub(1)) * page_size;

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
                "SELECT COUNT(*) FROM memory_snapshots ms \
                 {LATEST_MEMORY_SNAPSHOTS_JOIN} \
                 WHERE EXISTS (\
                     SELECT 1 FROM memory_events me \
                     WHERE me.memory_id = ms.memory_id AND me.actor_id = ?1\
                 ){archival_filter}"
            );
            match db.query_row(&sql, [agent_id], |row| row.get(0)) {
                Ok(count) => count,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("count query failed: {e}")})),
                    )
                        .into_response();
                }
            }
        }
        None => {
            let sql = format!(
                "SELECT COUNT(*) FROM memory_snapshots ms \
                 {LATEST_MEMORY_SNAPSHOTS_JOIN} \
                 WHERE 1 = 1{archival_filter}"
            );
            match db.query_row(&sql, [], |row| row.get(0)) {
                Ok(count) => count,
                Err(e) => {
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": format!("count query failed: {e}")})),
                    )
                        .into_response();
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
             {LATEST_MEMORY_SNAPSHOTS_JOIN} \
             WHERE EXISTS (\
                 SELECT 1 FROM memory_events me \
                 WHERE me.memory_id = ms.memory_id AND me.actor_id = ?1\
             ){archival_filter} \
             ORDER BY ms.id DESC LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = match db.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
                )
                    .into_response();
            }
        };
        let rows = stmt.query_map(rusqlite::params![agent_id, page_size, offset], |row| {
            Ok(MemoryEntry {
                id: row.get::<_, i64>(0)?,
                memory_id: row.get::<_, String>(1)?,
                snapshot: row.get::<_, String>(2)?,
                created_at: row.get::<_, String>(3)?,
            })
        });
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
                )
                    .into_response();
            }
        };
    } else {
        let sql = format!(
            "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at \
             FROM memory_snapshots ms \
             {LATEST_MEMORY_SNAPSHOTS_JOIN} \
             WHERE 1 = 1{archival_filter} \
             ORDER BY ms.id DESC LIMIT ?1 OFFSET ?2"
        );
        let mut stmt = match db.prepare(&sql) {
            Ok(stmt) => stmt,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": format!("query prepare failed: {e}")})),
                )
                    .into_response();
            }
        };
        let rows = stmt.query_map(rusqlite::params![page_size, offset], |row| {
            Ok(MemoryEntry {
                id: row.get::<_, i64>(0)?,
                memory_id: row.get::<_, String>(1)?,
                snapshot: row.get::<_, String>(2)?,
                created_at: row.get::<_, String>(3)?,
            })
        });
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
                )
                    .into_response();
            }
        };
    }

    (
        StatusCode::OK,
        Json(ListMemoriesResponse {
            memories,
            page,
            page_size,
            total,
        }),
    )
        .into_response()
}

/// GET /api/memory/:id — get a specific memory snapshot by ID.
pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
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

    // Try by memory_id first (TEXT), then by numeric id only if parseable (F1 fix).
    let row = db
        .query_row(
            "SELECT id, memory_id, snapshot, created_at FROM memory_snapshots \
             WHERE memory_id = ?1 ORDER BY id DESC LIMIT 1",
            [&id],
            |row| {
                Ok(MemoryEntry {
                    id: row.get::<_, i64>(0)?,
                    memory_id: row.get::<_, String>(1)?,
                    snapshot: row.get::<_, String>(2)?,
                    created_at: row.get::<_, String>(3)?,
                })
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
            let numeric_id: i64 = id
                .parse()
                .map_err(|_| rusqlite::Error::QueryReturnedNoRows)?;
            db.query_row(
                "SELECT id, memory_id, snapshot, created_at FROM memory_snapshots WHERE id = ?1",
                [numeric_id],
                |row| {
                    Ok(MemoryEntry {
                        id: row.get::<_, i64>(0)?,
                        memory_id: row.get::<_, String>(1)?,
                        snapshot: row.get::<_, String>(2)?,
                        created_at: row.get::<_, String>(3)?,
                    })
                },
            )
        });

    match row {
        Ok(memory) => (StatusCode::OK, Json(memory)).into_response(),
        Err(rusqlite::Error::QueryReturnedNoRows) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "memory not found", "id": id})),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("database error: {e}"), "id": id})),
        )
            .into_response(),
    }
}

/// GET /api/memory/graph — derive a graph view from the latest memory snapshots.
pub async fn get_memory_graph(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemoryGraphParams>,
) -> ApiResult<MemoryGraphResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_memory_graph", e))?;

    let limit = params.limit.unwrap_or(150).clamp(1, 300);
    let include_archived = params.include_archived.unwrap_or(false);
    let rows = query_graph_rows(&db, params.agent_id.as_deref(), include_archived, limit)?;

    Ok(Json(build_memory_graph(rows)))
}

/// GET /api/memory/search — search memories with filters (T-2.1.2).
///
/// Uses FTS5 full-text search when available (v031+), with RetrievalScorer
/// re-ranking for multi-factor relevance. Falls back to LIKE matching
/// on pre-v031 databases.
pub async fn search_memories(
    State(state): State<Arc<AppState>>,
    Query(params): Query<MemorySearchParams>,
) -> ApiResult<SearchMemoriesResponse> {
    let executed = execute_memory_search(state.as_ref(), params).await?;
    Ok(Json(executed.response))
}

pub(crate) async fn execute_memory_search(
    state: &AppState,
    params: MemorySearchParams,
) -> Result<ExecutedMemorySearch, ApiError> {
    let limit = params.limit.unwrap_or(50).min(200);
    let include_archived = params.include_archived.unwrap_or(false);

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("search_memories", e))?;

    // T-5.6.5: Validate confidence bounds.
    if let (Some(cmin), Some(cmax)) = (params.confidence_min, params.confidence_max) {
        if cmin > cmax {
            return Err(ApiError::bad_request(format!(
                "confidence_min ({cmin}) must be <= confidence_max ({cmax})"
            )));
        }
    }

    // Try FTS5 path first (v031+), fall back to LIKE.
    let use_fts = params.q.as_ref().is_some_and(|q| !q.trim().is_empty())
        && cortex_storage::queries::fts_queries::fts_available(&db);

    let raw_results = if use_fts {
        // FTS5 candidate retrieval with BM25 ranking.
        let q = params.q.as_deref().unwrap_or("");
        let fts_results = cortex_storage::queries::fts_queries::fts_search(
            &db,
            q,
            limit * 3, // Over-fetch for re-ranking
            include_archived,
        )
        .map_err(|e| {
            tracing::error!(error = %e, query = %q, "memory fts search failed");
            ApiError::internal(e.to_string())
        })?;

        fts_results
            .into_iter()
            .map(|r| SearchCandidate {
                id: r.id,
                memory_id: r.memory_id,
                snapshot: r.snapshot,
                created_at: r.created_at,
            })
            .collect::<Vec<_>>()
    } else {
        // Fallback: LIKE-based search (pre-v031 or no query text).
        search_like_fallback(&db, &params, include_archived, limit).map_err(|error| {
            tracing::error!(error = %error, query = ?params.q, "memory like search failed");
            error
        })?
    };

    // Apply post-retrieval filters (memory_type, importance, confidence range).
    let filtered: Vec<_> = raw_results
        .into_iter()
        .filter(|c| apply_snapshot_filters(c, &params))
        .collect();

    // Re-rank with RetrievalScorer (all 11 factors).
    let scorer = cortex_retrieval::RetrievalScorer::default();
    let convergence_score = get_convergence_score(&db);

    // Embed the query for vector similarity scoring.
    let query_embedding = if let Some(ref q) = params.q {
        if !q.trim().is_empty() {
            Some(state.embedding_engine.lock().await.embed_query(q))
        } else {
            None
        }
    } else {
        None
    };

    // Batch-fetch memory embeddings for all candidates.
    let memory_ids: Vec<&str> = filtered.iter().map(|c| c.memory_id.as_str()).collect();
    let embedding_map: std::collections::HashMap<String, Vec<f32>> =
        if cortex_storage::queries::embedding_queries::embeddings_available(&db) {
            cortex_storage::queries::embedding_queries::get_embeddings_batch(&db, &memory_ids)
                .unwrap_or_default()
                .into_iter()
                .collect()
        } else {
            std::collections::HashMap::new()
        };

    let mut scored: Vec<(SearchCandidate, f64)> = filtered
        .into_iter()
        .map(|c| {
            let query_ctx = cortex_retrieval::QueryContext {
                query_text: params.q.clone(),
                query_embedding: query_embedding.clone(),
                memory_embedding: embedding_map.get(&c.memory_id).cloned(),
                ..Default::default()
            };
            let score = parse_and_score(&c.snapshot, &scorer, convergence_score, &query_ctx);
            (c, score)
        })
        .collect();

    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(limit as usize);

    let total_matches = scored.len();
    let results: Vec<MemorySearchResultEntry> = scored
        .iter()
        .map(|(c, score)| {
            let snapshot_parsed = serde_json::from_str::<serde_json::Value>(&c.snapshot)
                .unwrap_or(serde_json::Value::String(c.snapshot.clone()));
            MemorySearchResultEntry {
                id: c.id,
                memory_id: c.memory_id.clone(),
                snapshot: snapshot_parsed,
                created_at: c.created_at.clone(),
                score: *score,
            }
        })
        .collect();

    Ok(ExecutedMemorySearch {
        total_matches,
        response: SearchMemoriesResponse {
            count: results.len(),
            results,
            query: params.q,
            search_mode: if use_fts { "fts5" } else { "like" }.to_string(),
            filters: MemorySearchFilters {
                agent_id: params.agent_id,
                memory_type: params.memory_type,
                importance: params.importance,
                confidence_min: params.confidence_min,
                confidence_max: params.confidence_max,
            },
        },
    })
}

struct SearchCandidate {
    id: i64,
    memory_id: String,
    snapshot: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct MemoryGraphRow {
    memory_id: String,
    snapshot: String,
    created_at: String,
}

#[derive(Debug, Clone)]
struct GraphNodeSource {
    response: MemoryGraphNode,
    tags: Vec<String>,
    memory_type: Option<MemoryType>,
    created_at: Option<DateTime<Utc>>,
}

fn query_graph_rows(
    db: &rusqlite::Connection,
    agent_id: Option<&str>,
    include_archived: bool,
    limit: u32,
) -> Result<Vec<MemoryGraphRow>, ApiError> {
    let archival_filter = if include_archived {
        ""
    } else {
        " AND ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)"
    };

    let rows = if let Some(agent_id) = agent_id {
        let sql = format!(
            "SELECT ms.memory_id, ms.snapshot, ms.created_at \
             FROM memory_snapshots ms \
             {LATEST_MEMORY_SNAPSHOTS_JOIN} \
             WHERE EXISTS (\
                 SELECT 1 FROM memory_events me \
                 WHERE me.memory_id = ms.memory_id AND me.actor_id = ?1\
             ){archival_filter} \
             ORDER BY ms.id DESC \
             LIMIT ?2"
        );
        let mut stmt = db
            .prepare(&sql)
            .map_err(|e| ApiError::db_error("memory_graph_prepare", e))?;
        let rows = stmt.query_map(rusqlite::params![agent_id, limit], |row| {
            Ok(MemoryGraphRow {
                memory_id: row.get(0)?,
                snapshot: row.get(1)?,
                created_at: row.get(2)?,
            })
        });
        let collected = rows
            .map_err(|e| ApiError::db_error("memory_graph_query", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ApiError::db_error("memory_graph_collect", e))?;
        drop(stmt);
        collected
    } else {
        let sql = format!(
            "SELECT ms.memory_id, ms.snapshot, ms.created_at \
             FROM memory_snapshots ms \
             {LATEST_MEMORY_SNAPSHOTS_JOIN} \
             WHERE 1 = 1{archival_filter} \
             ORDER BY ms.id DESC \
             LIMIT ?1"
        );
        let mut stmt = db
            .prepare(&sql)
            .map_err(|e| ApiError::db_error("memory_graph_prepare", e))?;
        let rows = stmt.query_map(rusqlite::params![limit], |row| {
            Ok(MemoryGraphRow {
                memory_id: row.get(0)?,
                snapshot: row.get(1)?,
                created_at: row.get(2)?,
            })
        });
        let collected = rows
            .map_err(|e| ApiError::db_error("memory_graph_query", e))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ApiError::db_error("memory_graph_collect", e))?;
        drop(stmt);
        collected
    };

    Ok(rows)
}

fn build_memory_graph(rows: Vec<MemoryGraphRow>) -> MemoryGraphResponse {
    let sources: Vec<GraphNodeSource> = rows.into_iter().map(graph_node_from_row).collect();
    let nodes = sources
        .iter()
        .map(|source| source.response.clone())
        .collect();
    let edges = build_graph_edges(&sources);

    MemoryGraphResponse { nodes, edges }
}

fn graph_node_from_row(row: MemoryGraphRow) -> GraphNodeSource {
    let parsed_memory = serde_json::from_str::<BaseMemory>(&row.snapshot).ok();
    let snapshot_value = serde_json::from_str::<serde_json::Value>(&row.snapshot)
        .unwrap_or_else(|_| serde_json::Value::String(row.snapshot.clone()));

    let label = label_for_memory(&row.memory_id, parsed_memory.as_ref(), &snapshot_value);
    let memory_type = parsed_memory.as_ref().map(|memory| memory.memory_type);
    let node_type = classify_node_type(memory_type);
    let importance = parsed_memory
        .as_ref()
        .map(|memory| importance_score(memory.importance))
        .unwrap_or_else(|| importance_from_value(&snapshot_value));
    let decay_factor = parsed_memory
        .as_ref()
        .map(memory_decay_factor)
        .unwrap_or_else(|| decay_from_created_at(&row.created_at, None));
    let tags = collect_graph_tags(parsed_memory.as_ref(), &snapshot_value);
    let created_at = parsed_memory
        .as_ref()
        .map(|memory| memory.created_at)
        .or_else(|| parse_rfc3339_utc(&row.created_at));

    GraphNodeSource {
        response: MemoryGraphNode {
            id: row.memory_id,
            label,
            node_type,
            importance,
            decay_factor,
        },
        tags,
        memory_type,
        created_at,
    }
}

fn build_graph_edges(sources: &[GraphNodeSource]) -> Vec<MemoryGraphEdge> {
    let mut edges: HashMap<(String, String), MemoryGraphEdge> = HashMap::new();
    let mut tag_groups: HashMap<String, Vec<&GraphNodeSource>> = HashMap::new();

    for source in sources {
        for tag in &source.tags {
            tag_groups.entry(tag.clone()).or_default().push(source);
        }
    }

    for (tag, mut group) in tag_groups {
        if group.len() < 2 {
            continue;
        }
        group.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        for pair in group.windows(2) {
            insert_graph_edge(&mut edges, pair[0], pair[1], tag.clone());
        }
    }

    if edges.is_empty() {
        let mut type_groups: HashMap<String, Vec<&GraphNodeSource>> = HashMap::new();
        for source in sources {
            let key = source
                .memory_type
                .map(|memory_type| format!("{memory_type:?}"))
                .unwrap_or_else(|| source.response.node_type.clone());
            type_groups.entry(key).or_default().push(source);
        }

        for (relationship, mut group) in type_groups {
            if group.len() < 2 {
                continue;
            }
            group.sort_by(|a, b| b.created_at.cmp(&a.created_at));
            for pair in group.windows(2) {
                insert_graph_edge(&mut edges, pair[0], pair[1], relationship.clone());
            }
        }
    }

    let mut edge_list: Vec<_> = edges.into_values().collect();
    edge_list.sort_by(|a, b| {
        b.strength
            .partial_cmp(&a.strength)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if edge_list.len() > 250 {
        edge_list.truncate(250);
    }
    edge_list
}

fn insert_graph_edge(
    edges: &mut HashMap<(String, String), MemoryGraphEdge>,
    left: &GraphNodeSource,
    right: &GraphNodeSource,
    relationship: String,
) {
    if left.response.id == right.response.id {
        return;
    }

    let (source, target) = if left.response.id <= right.response.id {
        (left.response.id.clone(), right.response.id.clone())
    } else {
        (right.response.id.clone(), left.response.id.clone())
    };

    let strength = ((left.response.importance + right.response.importance) / 2.0).clamp(0.0, 1.0);
    let key = (source.clone(), target.clone());

    match edges.get_mut(&key) {
        Some(existing) if strength > existing.strength => {
            existing.relationship = relationship;
            existing.strength = strength;
        }
        Some(_) => {}
        None => {
            edges.insert(
                key,
                MemoryGraphEdge {
                    source,
                    target,
                    relationship,
                    strength,
                },
            );
        }
    }
}

fn label_for_memory(
    memory_id: &str,
    parsed_memory: Option<&BaseMemory>,
    snapshot_value: &serde_json::Value,
) -> String {
    if let Some(memory) = parsed_memory {
        let summary = memory.summary.trim();
        if !summary.is_empty() {
            return truncate_label(summary);
        }

        if let Some(text) = first_string_from_value(
            &memory.content,
            &[
                "title",
                "name",
                "subject",
                "entity",
                "label",
                "message",
                "description",
                "text",
            ],
        ) {
            return truncate_label(text);
        }
    }

    if let Some(text) = first_string_from_value(
        snapshot_value,
        &[
            "summary",
            "title",
            "name",
            "subject",
            "entity",
            "label",
            "message",
            "description",
            "text",
        ],
    ) {
        return truncate_label(text);
    }

    truncate_label(memory_id)
}

fn collect_graph_tags(
    parsed_memory: Option<&BaseMemory>,
    snapshot_value: &serde_json::Value,
) -> Vec<String> {
    let mut tags = Vec::new();

    if let Some(memory) = parsed_memory {
        tags.extend(memory.tags.iter().cloned());

        if let Some(strings) =
            string_array_from_value(&memory.content, &["entities", "concepts", "topics"])
        {
            tags.extend(strings);
        }
        if let Some(value) =
            first_string_from_value(&memory.content, &["subject", "entity", "topic"])
        {
            tags.push(value.to_string());
        }
    }

    if let Some(strings) =
        string_array_from_value(snapshot_value, &["tags", "entities", "concepts", "topics"])
    {
        tags.extend(strings);
    }
    if let Some(value) = first_string_from_value(snapshot_value, &["subject", "entity", "topic"]) {
        tags.push(value.to_string());
    }

    let mut seen = HashSet::new();
    tags.into_iter()
        .map(|tag| tag.trim().to_lowercase())
        .filter(|tag| !tag.is_empty() && tag.len() <= 64)
        .filter(|tag| seen.insert(tag.clone()))
        .collect()
}

fn string_array_from_value(value: &serde_json::Value, keys: &[&str]) -> Option<Vec<String>> {
    let object = value.as_object()?;
    let mut values = Vec::new();

    for key in keys {
        if let Some(items) = object.get(*key).and_then(|entry| entry.as_array()) {
            for item in items {
                if let Some(text) = item.as_str() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        values.push(trimmed.to_string());
                    }
                }
            }
        }
    }

    if values.is_empty() {
        None
    } else {
        Some(values)
    }
}

fn first_string_from_value<'a>(value: &'a serde_json::Value, keys: &[&str]) -> Option<&'a str> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(candidate) = object.get(*key).and_then(|entry| entry.as_str()) {
            let trimmed = candidate.trim();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn classify_node_type(memory_type: Option<MemoryType>) -> String {
    match memory_type {
        Some(
            MemoryType::Goal
            | MemoryType::Relationship
            | MemoryType::Preference
            | MemoryType::AgentGoal,
        ) => "entity".to_string(),
        Some(
            MemoryType::Conversation
            | MemoryType::Episodic
            | MemoryType::Observation
            | MemoryType::Experiment
            | MemoryType::ConvergenceEvent
            | MemoryType::BoundaryViolation
            | MemoryType::SimulationResult,
        ) => "event".to_string(),
        _ => "concept".to_string(),
    }
}

fn importance_score(importance: Importance) -> f64 {
    match importance {
        Importance::Trivial => 0.15,
        Importance::Low => 0.3,
        Importance::Normal => 0.5,
        Importance::High => 0.8,
        Importance::Critical => 1.0,
    }
}

fn importance_from_value(snapshot_value: &serde_json::Value) -> f64 {
    match first_string_from_value(snapshot_value, &["importance"]) {
        Some("Trivial") => 0.15,
        Some("Low") => 0.3,
        Some("High") => 0.8,
        Some("Critical") => 1.0,
        _ => 0.5,
    }
}

fn memory_decay_factor(memory: &BaseMemory) -> f64 {
    decay_from_datetime(memory.created_at, memory.memory_type.half_life_days())
}

fn decay_from_created_at(created_at: &str, half_life_days: Option<u32>) -> f64 {
    parse_rfc3339_utc(created_at)
        .map(|timestamp| decay_from_datetime(timestamp, half_life_days))
        .unwrap_or(0.0)
}

fn decay_from_datetime(created_at: DateTime<Utc>, half_life_days: Option<u32>) -> f64 {
    let Some(half_life_days) = half_life_days else {
        return 0.0;
    };
    let age_days = (Utc::now() - created_at).num_seconds().max(0) as f64 / 86_400.0;
    (age_days / f64::from(half_life_days)).clamp(0.0, 1.0)
}

fn parse_rfc3339_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn truncate_label(value: &str) -> String {
    const MAX_LABEL_CHARS: usize = 48;
    let label: String = value.chars().take(MAX_LABEL_CHARS).collect();
    if value.chars().count() > MAX_LABEL_CHARS {
        format!("{label}...")
    } else {
        label
    }
}

/// LIKE-based fallback for pre-FTS5 databases.
fn search_like_fallback(
    db: &rusqlite::Connection,
    params: &MemorySearchParams,
    include_archived: bool,
    limit: u32,
) -> Result<Vec<SearchCandidate>, ApiError> {
    let mut conditions = Vec::new();
    let mut bind_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    let mut idx = 1u32;

    if !include_archived {
        conditions
            .push("ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)".to_string());
    }

    if let Some(ref q) = params.q {
        if !q.trim().is_empty() {
            conditions.push(format!("ms.snapshot LIKE ?{idx} ESCAPE '\\'"));
            let escaped = q
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            bind_params.push(Box::new(format!("%{escaped}%")));
            idx += 1;
        }
    }

    if let Some(ref agent_id) = params.agent_id {
        conditions.push(format!(
            "EXISTS (SELECT 1 FROM memory_events me WHERE me.memory_id = ms.memory_id AND me.actor_id = ?{idx})"
        ));
        bind_params.push(Box::new(agent_id.clone()));
        idx += 1;
    }

    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    let query = format!(
        "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at \
         FROM memory_snapshots ms \
         {LATEST_MEMORY_SNAPSHOTS_JOIN} \
         {where_clause} \
         ORDER BY ms.id DESC \
         LIMIT ?{idx}"
    );
    bind_params.push(Box::new(limit));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> =
        bind_params.iter().map(|p| p.as_ref()).collect();

    let mut stmt = db
        .prepare(&query)
        .map_err(|e| ApiError::db_error("memory_search_prepare", e))?;

    let results = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok(SearchCandidate {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                snapshot: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|e| ApiError::db_error("memory_search_query", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(results)
}

/// Apply memory_type, importance, and confidence filters on snapshot JSON.
fn apply_snapshot_filters(candidate: &SearchCandidate, params: &MemorySearchParams) -> bool {
    let snapshot: serde_json::Value = match serde_json::from_str(&candidate.snapshot) {
        Ok(v) => v,
        Err(_) => return true, // Keep unparseable snapshots
    };

    if let Some(ref mt) = params.memory_type {
        let expected = normalize_memory_type_filter(mt).unwrap_or_else(|| mt.clone());
        if snapshot.get("memory_type").and_then(|v| v.as_str()) != Some(expected.as_str()) {
            return false;
        }
    }

    if let Some(ref imp) = params.importance {
        let expected = normalize_importance_filter(imp).unwrap_or_else(|| imp.clone());
        if snapshot.get("importance").and_then(|v| v.as_str()) != Some(expected.as_str()) {
            return false;
        }
    }

    if let Some(cmin) = params.confidence_min {
        if let Some(conf) = snapshot.get("confidence").and_then(|v| v.as_f64()) {
            if conf < cmin {
                return false;
            }
        }
    }

    if let Some(cmax) = params.confidence_max {
        if let Some(conf) = snapshot.get("confidence").and_then(|v| v.as_f64()) {
            if conf > cmax {
                return false;
            }
        }
    }

    true
}

/// Parse a snapshot into BaseMemory and score with RetrievalScorer.
fn parse_and_score(
    snapshot_str: &str,
    scorer: &cortex_retrieval::RetrievalScorer,
    convergence_score: f64,
    ctx: &cortex_retrieval::QueryContext,
) -> f64 {
    // Try to parse as BaseMemory; if it fails, return a neutral score.
    let parsed: Result<BaseMemory, _> = serde_json::from_str(snapshot_str);
    match parsed {
        Ok(memory) => scorer.score_with_context(&memory, convergence_score, ctx),
        Err(_) => {
            // Fallback: parse what we can for a minimal score.
            let v: serde_json::Value =
                serde_json::from_str(snapshot_str).unwrap_or(serde_json::json!({}));
            let importance = match v.get("importance").and_then(|i| i.as_str()) {
                Some("Critical") => Importance::Critical,
                Some("High") => Importance::High,
                Some("Low") => Importance::Low,
                Some("Trivial") => Importance::Trivial,
                _ => Importance::Normal,
            };
            let confidence = v.get("confidence").and_then(|c| c.as_f64()).unwrap_or(0.5);
            let memory = BaseMemory {
                id: uuid::Uuid::nil(),
                memory_type: MemoryType::Semantic,
                content: v.get("content").cloned().unwrap_or(serde_json::json!({})),
                summary: v
                    .get("summary")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                importance,
                confidence,
                created_at: chrono::Utc::now(),
                last_accessed: None,
                access_count: 0,
                tags: vec![],
                archived: false,
            };
            scorer.score_with_context(&memory, convergence_score, ctx)
        }
    }
}

/// Fetch the latest convergence score from the database.
fn get_convergence_score(db: &rusqlite::Connection) -> f64 {
    db.query_row(
        "SELECT COALESCE(composite_score, 0.0) FROM convergence_scores \
         ORDER BY recorded_at DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .unwrap_or(0.0)
}

/// Request body for creating/updating a memory.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
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
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Json(body): Json<WriteMemoryRequest>,
) -> Response {
    let db = state.db.write().await;
    let actor = memory_actor(
        claims.as_ref().map(|claims| &claims.0),
        Some(body.actor_id.as_str()),
    );
    let request_body = serde_json::to_value(&body).unwrap_or(serde_json::Value::Null);

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor,
        "POST",
        WRITE_MEMORY_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let previous_hash = cortex_storage::queries::memory_event_queries::latest_event_hash(
                conn,
                &body.memory_id,
            )
            .map_err(|e| ApiError::internal(format!("memory hash lookup failed: {e}")))?;
            let event_hash = blake3::hash(
                format!(
                    "{}:{}:{}:{}",
                    body.memory_id, body.event_type, body.actor_id, operation_context.request_id
                )
                .as_bytes(),
            );
            let previous_hash = previous_hash.unwrap_or_else(|| vec![0u8; 32]);

            cortex_storage::queries::memory_event_queries::insert_event(
                conn,
                &body.memory_id,
                &body.event_type,
                &body.delta,
                &body.actor_id,
                event_hash.as_bytes(),
                previous_hash.as_slice(),
            )
            .map_err(|e| ApiError::internal(format!("memory event insert failed: {e}")))?;

            if let Some(ref snapshot) = body.snapshot {
                let state_hash = blake3::hash(snapshot.as_bytes());
                cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
                    conn,
                    &body.memory_id,
                    snapshot,
                    Some(state_hash.as_bytes()),
                )
                .map_err(|e| ApiError::internal(format!("snapshot insert failed: {e}")))?;
            }

            let details = format!("event_type={}, actor={}", body.event_type, body.actor_id);
            cortex_storage::queries::memory_audit_queries::insert_audit(
                conn,
                &body.memory_id,
                &body.event_type,
                Some(&details),
            )
            .map_err(|e| ApiError::internal(format!("memory audit insert failed: {e}")))?;

            Ok((
                StatusCode::CREATED,
                serde_json::json!({
                    "status": "ok",
                    "memory_id": body.memory_id,
                    "event_type": body.event_type,
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "memory",
                "memory_write",
                "info",
                &actor,
                "ok",
                serde_json::json!({
                    "memory_id": body.memory_id,
                    "event_type": body.event_type,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );

            if outcome.idempotency_status
                == crate::api::operation_context::IdempotencyStatus::Executed
            {
                if let Some(ref snapshot) = body.snapshot {
                    if let Ok(memory) =
                        serde_json::from_str::<cortex_core::memory::BaseMemory>(snapshot)
                    {
                        let mut engine = state.embedding_engine.lock().await;
                        let embedding = engine.embed_memory(&memory);
                        if cortex_storage::queries::embedding_queries::embeddings_available(&db) {
                            if let Err(error) =
                                cortex_storage::queries::embedding_queries::upsert_embedding(
                                    &db,
                                    &body.memory_id,
                                    &embedding,
                                    "tfidf",
                                )
                            {
                                tracing::warn!(
                                    error = %error,
                                    memory_id = %body.memory_id,
                                    "embedding storage failed"
                                );
                            }
                        }
                    }
                }
            }

            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

// ─── Archival endpoints ──────────────────────────────────────────────────

/// Request body for archiving a memory.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
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
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(memory_id): Path<String>,
    Json(body): Json<ArchiveMemoryRequest>,
) -> Response {
    let db = state.db.write().await;
    let actor = memory_actor(claims.as_ref().map(|claims| &claims.0), None);
    let request_body = serde_json::json!({
        "memory_id": memory_id,
        "reason": body.reason.clone(),
        "decayed_confidence": body.decayed_confidence,
        "original_confidence": body.original_confidence,
    });

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor,
        "POST",
        ARCHIVE_MEMORY_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            let exists: bool = conn
                .query_row(
                    "SELECT COUNT(*) > 0 FROM memory_snapshots WHERE memory_id = ?1",
                    [&memory_id],
                    |row| row.get(0),
                )
                .map_err(|e| ApiError::db_error("archive_check", e))?;

            if !exists {
                return Err(ApiError::not_found(format!("memory {memory_id} not found")));
            }

            if cortex_storage::queries::archival_queries::is_archived(conn, &memory_id)
                .map_err(|e| ApiError::internal(e.to_string()))?
            {
                return Err(ApiError::bad_request(format!(
                    "memory {memory_id} is already archived"
                )));
            }

            cortex_storage::queries::archival_queries::insert_archival_record(
                conn,
                &memory_id,
                &body.reason,
                body.decayed_confidence.unwrap_or(0.0),
                body.original_confidence.unwrap_or(0.0),
            )
            .map_err(|e| ApiError::internal(e.to_string()))?;

            if let Ok(Some(latest)) =
                cortex_storage::queries::memory_snapshot_queries::latest_by_memory(conn, &memory_id)
            {
                if let Ok(mut snapshot) =
                    serde_json::from_str::<serde_json::Value>(&latest.snapshot)
                {
                    snapshot["archived"] = serde_json::json!(true);
                    let updated = serde_json::to_string(&snapshot).unwrap_or_default();
                    let state_hash = blake3::hash(updated.as_bytes());
                    cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
                        conn,
                        &memory_id,
                        &updated,
                        Some(state_hash.as_bytes()),
                    )
                    .map_err(|e| {
                        ApiError::internal(format!("archive snapshot insert failed: {e}"))
                    })?;
                }
            }

            Ok((
                StatusCode::OK,
                serde_json::to_value(MemoryArchiveStatusResponse {
                    status: "archived".to_string(),
                    memory_id: memory_id.clone(),
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "status": "archived",
                        "memory_id": memory_id,
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "memory",
                "memory_archive",
                "info",
                &actor,
                "archived",
                serde_json::json!({
                    "memory_id": memory_id,
                    "reason": body.reason,
                }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// POST /api/memory/:id/unarchive — restore an archived memory.
pub async fn unarchive_memory(
    State(state): State<Arc<AppState>>,
    claims: Option<Extension<Claims>>,
    Extension(operation_context): Extension<OperationContext>,
    Path(memory_id): Path<String>,
) -> Response {
    let db = state.db.write().await;
    let actor = memory_actor(claims.as_ref().map(|claims| &claims.0), None);
    let request_body = serde_json::json!({ "memory_id": memory_id.clone() });

    match execute_idempotent_json_mutation(
        &db,
        &operation_context,
        &actor,
        "POST",
        UNARCHIVE_MEMORY_ROUTE_TEMPLATE,
        &request_body,
        |conn| {
            if !cortex_storage::queries::archival_queries::is_archived(conn, &memory_id)
                .map_err(|e| ApiError::internal(e.to_string()))?
            {
                return Err(ApiError::bad_request(format!(
                    "memory {memory_id} is not archived"
                )));
            }

            cortex_storage::queries::archival_queries::remove_archival_record(conn, &memory_id)
                .map_err(|e| ApiError::internal(e.to_string()))?;

            if let Ok(Some(latest)) =
                cortex_storage::queries::memory_snapshot_queries::latest_by_memory(conn, &memory_id)
            {
                if let Ok(mut snapshot) =
                    serde_json::from_str::<serde_json::Value>(&latest.snapshot)
                {
                    snapshot["archived"] = serde_json::json!(false);
                    let updated = serde_json::to_string(&snapshot).unwrap_or_default();
                    let state_hash = blake3::hash(updated.as_bytes());
                    cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
                        conn,
                        &memory_id,
                        &updated,
                        Some(state_hash.as_bytes()),
                    )
                    .map_err(|e| {
                        ApiError::internal(format!("unarchive snapshot insert failed: {e}"))
                    })?;
                }
            }

            Ok((
                StatusCode::OK,
                serde_json::to_value(MemoryArchiveStatusResponse {
                    status: "unarchived".to_string(),
                    memory_id: memory_id.clone(),
                })
                .unwrap_or_else(|_| {
                    serde_json::json!({
                        "status": "unarchived",
                        "memory_id": memory_id,
                    })
                }),
            ))
        },
    ) {
        Ok(outcome) => {
            write_mutation_audit_entry(
                &db,
                "memory",
                "memory_unarchive",
                "info",
                &actor,
                "unarchived",
                serde_json::json!({ "memory_id": memory_id }),
                &operation_context,
                &outcome.idempotency_status,
            );
            json_response_with_idempotency(outcome.status, outcome.body, outcome.idempotency_status)
        }
        Err(error) => error_response_with_idempotency(error),
    }
}

/// GET /api/memory/archived — list archived memories.
pub async fn list_archived(
    State(state): State<Arc<AppState>>,
) -> ApiResult<ArchivedMemoryListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_archived", e))?;

    let rows = cortex_storage::queries::archival_queries::query_archived(&db, 200)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let results: Vec<ArchivedMemoryEntry> = rows
        .iter()
        .map(|r| ArchivedMemoryEntry {
            memory_id: r.memory_id.clone(),
            archived_at: r.archived_at.clone(),
            reason: r.reason.clone(),
            decayed_confidence: r.decayed_confidence.unwrap_or(0.0),
            original_confidence: r.original_confidence.unwrap_or(0.0),
        })
        .collect();

    Ok(Json(ArchivedMemoryListResponse {
        count: results.len(),
        archived: results,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn sample_memory(id: &str, memory_type: MemoryType, summary: &str, tags: &[&str]) -> String {
        serde_json::to_string(&BaseMemory {
            id: Uuid::parse_str(id).unwrap(),
            memory_type,
            content: json!({
                "subject": summary,
                "entities": tags,
            }),
            summary: summary.to_string(),
            importance: Importance::High,
            confidence: 0.9,
            created_at: Utc::now(),
            last_accessed: None,
            access_count: 0,
            tags: tags.iter().map(|tag| tag.to_string()).collect(),
            archived: false,
        })
        .unwrap()
    }

    #[test]
    fn graph_builds_edges_from_shared_tags() {
        let rows = vec![
            MemoryGraphRow {
                memory_id: "memory-alpha".to_string(),
                snapshot: sample_memory(
                    "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
                    MemoryType::Semantic,
                    "Shared architecture decision",
                    &["rust", "gateway"],
                ),
                created_at: Utc::now().to_rfc3339(),
            },
            MemoryGraphRow {
                memory_id: "memory-beta".to_string(),
                snapshot: sample_memory(
                    "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                    MemoryType::Skill,
                    "Gateway implementation note",
                    &["gateway"],
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        ];

        let graph = build_memory_graph(rows);

        assert_eq!(graph.nodes.len(), 2);
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].relationship, "gateway");
        assert_eq!(graph.edges[0].source, "memory-alpha");
        assert_eq!(graph.edges[0].target, "memory-beta");
    }

    #[test]
    fn graph_falls_back_to_type_edges_without_shared_tags() {
        let rows = vec![
            MemoryGraphRow {
                memory_id: "memory-one".to_string(),
                snapshot: sample_memory(
                    "11111111-1111-4111-8111-111111111111",
                    MemoryType::Semantic,
                    "First concept",
                    &["alpha"],
                ),
                created_at: Utc::now().to_rfc3339(),
            },
            MemoryGraphRow {
                memory_id: "memory-two".to_string(),
                snapshot: sample_memory(
                    "22222222-2222-4222-8222-222222222222",
                    MemoryType::Semantic,
                    "Second concept",
                    &["beta"],
                ),
                created_at: Utc::now().to_rfc3339(),
            },
        ];

        let graph = build_memory_graph(rows);

        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.edges[0].relationship, "Semantic");
    }

    #[test]
    fn filter_normalization_maps_dashboard_aliases_to_canonical_values() {
        assert_eq!(
            normalize_memory_type_filter("reflection").as_deref(),
            Some("AgentReflection")
        );
        assert_eq!(
            normalize_importance_filter("medium").as_deref(),
            Some("Normal")
        );
    }

    #[test]
    fn snapshot_filters_accept_lowercase_alias_filters_against_canonical_snapshot() {
        let candidate = SearchCandidate {
            id: 1,
            memory_id: "memory-1".to_string(),
            snapshot: serde_json::json!({
                "memory_type": "Semantic",
                "importance": "Normal",
                "confidence": 0.8
            })
            .to_string(),
            created_at: Utc::now().to_rfc3339(),
        };

        assert!(apply_snapshot_filters(
            &candidate,
            &MemorySearchParams {
                q: None,
                agent_id: None,
                memory_type: Some("semantic".to_string()),
                importance: Some("medium".to_string()),
                confidence_min: None,
                confidence_max: None,
                limit: None,
                include_archived: None,
            }
        ));
    }
}
