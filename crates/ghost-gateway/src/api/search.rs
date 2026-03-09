//! Unified search endpoint (T-3.5.1).
//!
//! Parallel LIKE queries across entity tables: agents, sessions, memories,
//! proposals (goals), and audit logs.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_types")]
    pub types: String,
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_types() -> String {
    "agents,sessions,memories,proposals,audit".into()
}

fn default_limit() -> u32 {
    50
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub result_type: String,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
}

/// Escape LIKE metacharacters in user input.
fn escape_like(input: &str) -> String {
    input
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// UTF-8-safe string truncation. Returns up to `max_chars` characters.
fn truncate_chars(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        Some((byte_idx, _)) => &s[..byte_idx],
        None => s,
    }
}

/// GET /api/search — unified search across entity types.
pub async fn search(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> ApiResult<SearchResponse> {
    let q = params.q.trim();
    if q.is_empty() {
        return Err(ApiError::bad_request("Search query cannot be empty"));
    }

    let types: Vec<&str> = params.types.split(',').map(|s| s.trim()).collect();
    let escaped = escape_like(q);
    let like_pattern = format!("%{escaped}%");
    let per_type_limit = (params.limit as usize / types.len().max(1)).max(5);

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("search", e))?;
    let mut results = Vec::new();

    if types.contains(&"agents") {
        search_agents(&db, &like_pattern, per_type_limit, &mut results);
    }
    if types.contains(&"sessions") {
        search_sessions(&db, &like_pattern, per_type_limit, &mut results);
    }
    if types.contains(&"memories") {
        search_memories(&db, &like_pattern, per_type_limit, &mut results);
    }
    if types.contains(&"proposals") {
        search_proposals(&db, &like_pattern, per_type_limit, &mut results);
    }
    if types.contains(&"audit") {
        search_audit(&db, &like_pattern, per_type_limit, &mut results);
    }

    // Sort by score descending.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(params.limit as usize);

    let total = results.len();
    Ok(Json(SearchResponse {
        query: q.to_string(),
        results,
        total,
    }))
}

fn search_agents(
    db: &rusqlite::Connection,
    pattern: &str,
    limit: usize,
    results: &mut Vec<SearchResult>,
) {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, name FROM agents \
         WHERE name LIKE ?1 ESCAPE '\\' OR id LIKE ?1 ESCAPE '\\' \
         LIMIT ?2",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        Ok(SearchResult {
            result_type: "agent".into(),
            id: row.get(0)?,
            title: row.get(1)?,
            snippet: String::new(),
            score: 1.0,
        })
    }) else {
        return;
    };
    results.extend(rows.filter_map(|r| r.ok()));
}

fn search_sessions(
    db: &rusqlite::Connection,
    pattern: &str,
    limit: usize,
    results: &mut Vec<SearchResult>,
) {
    let Ok(mut stmt) = db.prepare(
        "SELECT session_id, GROUP_CONCAT(DISTINCT COALESCE(sender, 'unknown')) AS agents \
         FROM itp_events \
         WHERE session_id LIKE ?1 ESCAPE '\\' OR COALESCE(sender, '') LIKE ?1 ESCAPE '\\' \
         GROUP BY session_id \
         LIMIT ?2",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let sid: String = row.get(0)?;
        let aid: String = row.get::<_, String>(1).unwrap_or_default();
        Ok(SearchResult {
            result_type: "session".into(),
            id: sid.clone(),
            title: format!("Session {}", truncate_chars(&sid, 8)),
            snippet: format!("Agent: {aid}"),
            score: 0.8,
        })
    }) else {
        return;
    };
    results.extend(rows.filter_map(|r| r.ok()));
}

fn search_memories(
    db: &rusqlite::Connection,
    pattern: &str,
    limit: usize,
    results: &mut Vec<SearchResult>,
) {
    let Ok(mut stmt) = db.prepare(
        "SELECT ms.memory_id, \
                COALESCE(json_extract(ms.snapshot, '$.memory_type'), 'memory'), \
                COALESCE(json_extract(ms.snapshot, '$.summary'), '') \
         FROM memory_snapshots ms \
         JOIN (
             SELECT memory_id, MAX(id) AS max_id
             FROM memory_snapshots
             GROUP BY memory_id
         ) latest ON latest.max_id = ms.id \
         WHERE COALESCE(json_extract(ms.snapshot, '$.summary'), '') LIKE ?1 ESCAPE '\\' \
            OR ms.snapshot LIKE ?1 ESCAPE '\\' \
            OR ms.memory_id LIKE ?1 ESCAPE '\\' \
         LIMIT ?2",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let id: String = row.get(0)?;
        let mtype: String = row.get::<_, String>(1).unwrap_or_default();
        let summary: String = row.get::<_, String>(2).unwrap_or_default();
        Ok(SearchResult {
            result_type: "memory".into(),
            id,
            title: mtype,
            snippet: if summary.chars().count() > 100 {
                format!("{}…", truncate_chars(&summary, 100))
            } else {
                summary
            },
            score: 0.9,
        })
    }) else {
        return;
    };
    results.extend(rows.filter_map(|r| r.ok()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn search_sessions_matches_sender_column() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO itp_events (
                id, session_id, event_type, sender, timestamp, sequence_number,
                content_hash, content_length, privacy_level, latency_ms, token_count,
                event_hash, previous_hash, attributes
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                "evt-1",
                "session-knowledge",
                "InteractionMessage",
                "agent-search",
                "2026-03-09T00:00:00Z",
                1i64,
                "hash-1",
                8i64,
                "internal",
                1i64,
                2i64,
                vec![1u8; 32],
                vec![0u8; 32],
                "{}",
            ],
        )
        .unwrap();

        let mut results = Vec::new();
        search_sessions(&conn, "%agent-search%", 10, &mut results);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "session-knowledge");
        assert!(results[0].snippet.contains("agent-search"));
    }

    #[test]
    fn search_memories_uses_latest_snapshot_fields() {
        let conn = setup_db();
        cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &conn,
            "memory-alpha",
            r#"{"memory_type":"Semantic","summary":"stale summary","content":"old"}"#,
            None,
        )
        .unwrap();
        cortex_storage::queries::memory_snapshot_queries::insert_snapshot(
            &conn,
            "memory-alpha",
            r#"{"memory_type":"Semantic","summary":"fresh summary marker","content":"new"}"#,
            None,
        )
        .unwrap();

        let mut results = Vec::new();
        search_memories(&conn, "%fresh summary marker%", 10, &mut results);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "memory-alpha");
        assert_eq!(results[0].title, "Semantic");
        assert_eq!(results[0].snippet, "fresh summary marker");
    }
}

fn search_proposals(
    db: &rusqlite::Connection,
    pattern: &str,
    limit: usize,
    results: &mut Vec<SearchResult>,
) {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, operation, decision FROM goal_proposals \
         WHERE operation LIKE ?1 ESCAPE '\\' OR id LIKE ?1 ESCAPE '\\' \
         LIMIT ?2",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let id: String = row.get(0)?;
        let op: String = row.get::<_, String>(1).unwrap_or_default();
        let status: String = row.get::<_, String>(2).unwrap_or_default();
        Ok(SearchResult {
            result_type: "proposal".into(),
            id,
            title: op,
            snippet: format!("Status: {status}"),
            score: 0.7,
        })
    }) else {
        return;
    };
    results.extend(rows.filter_map(|r| r.ok()));
}

fn search_audit(
    db: &rusqlite::Connection,
    pattern: &str,
    limit: usize,
    results: &mut Vec<SearchResult>,
) {
    let Ok(mut stmt) = db.prepare(
        "SELECT id, event_type, details FROM audit_log \
         WHERE event_type LIKE ?1 ESCAPE '\\' OR details LIKE ?1 ESCAPE '\\' \
         LIMIT ?2",
    ) else {
        return;
    };
    let Ok(rows) = stmt.query_map(rusqlite::params![pattern, limit], |row| {
        let id: String = row.get(0)?;
        let etype: String = row.get::<_, String>(1).unwrap_or_default();
        let details: String = row.get::<_, String>(2).unwrap_or_default();
        Ok(SearchResult {
            result_type: "audit".into(),
            id,
            title: etype,
            snippet: if details.chars().count() > 100 {
                format!("{}…", truncate_chars(&details, 100))
            } else {
                details
            },
            score: 0.6,
        })
    }) else {
        return;
    };
    results.extend(rows.filter_map(|r| r.ok()));
}
