//! Unified search endpoint (T-3.5.1).
//!
//! Global search now acts as an orchestrator over domain-owned search semantics
//! and emits addressable navigation metadata for every result.

use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use url::form_urlencoded::Serializer;

use crate::api::error::{ApiError, ApiResult};
use crate::api::memory::{execute_memory_search, MemorySearchParams};
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

#[derive(Debug, Clone, Serialize)]
pub struct SearchNavigation {
    pub href: String,
    pub route_kind: String,
    pub focus_id: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchMatchContext {
    pub matched_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub result_type: String,
    pub id: String,
    pub title: String,
    pub snippet: String,
    pub score: f64,
    pub navigation: SearchNavigation,
    pub match_context: SearchMatchContext,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchTypeCount {
    pub result_type: String,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchDomainWarning {
    pub result_type: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub results: Vec<SearchResult>,
    pub total: usize,
    pub returned_count: usize,
    pub totals_by_type: Vec<SearchTypeCount>,
    pub degraded: bool,
    pub warnings: Vec<SearchDomainWarning>,
}

#[derive(Debug)]
struct SearchBucket {
    result_type: &'static str,
    total_matches: usize,
    results: Vec<SearchResult>,
}

#[derive(Debug)]
struct SearchableAuditRow {
    id: String,
    event_type: String,
    details: String,
    agent_id: String,
    tool_name: Option<String>,
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

fn normalize_query(query: &str) -> String {
    query.trim().to_ascii_lowercase()
}

fn validate_types(types: &[&str]) -> Result<(), ApiError> {
    const VALID_TYPES: &[&str] = &["agents", "sessions", "memories", "proposals", "audit"];
    let invalid = types
        .iter()
        .copied()
        .filter(|value| !VALID_TYPES.contains(value))
        .collect::<Vec<_>>();
    if invalid.is_empty() {
        Ok(())
    } else {
        Err(ApiError::bad_request(format!(
            "Unknown search types: {}",
            invalid.join(", ")
        )))
    }
}

fn build_query_path(path: &str, params: &[(&str, Option<&str>)]) -> String {
    let mut serializer = Serializer::new(String::new());
    for (key, value) in params {
        if let Some(value) = value.filter(|value| !value.is_empty()) {
            serializer.append_pair(key, value);
        }
    }
    let query = serializer.finish();
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

fn navigation_for(result_type: &'static str, id: &str, query: &str) -> SearchNavigation {
    match result_type {
        "agent" => SearchNavigation {
            href: format!("/agents/{id}"),
            route_kind: "detail".into(),
            focus_id: None,
            query: Some(query.to_string()),
        },
        "session" => SearchNavigation {
            href: format!("/sessions/{id}"),
            route_kind: "detail".into(),
            focus_id: None,
            query: Some(query.to_string()),
        },
        "proposal" => SearchNavigation {
            href: format!("/goals/{id}"),
            route_kind: "detail".into(),
            focus_id: None,
            query: Some(query.to_string()),
        },
        "memory" => SearchNavigation {
            href: build_query_path("/memory", &[("q", Some(query)), ("focus", Some(id))]),
            route_kind: "collection".into(),
            focus_id: Some(id.to_string()),
            query: Some(query.to_string()),
        },
        "audit" => SearchNavigation {
            href: build_query_path("/security", &[("search", Some(query)), ("focus", Some(id))]),
            route_kind: "collection".into(),
            focus_id: Some(id.to_string()),
            query: Some(query.to_string()),
        },
        _ => SearchNavigation {
            href: build_query_path("/search", &[("q", Some(query))]),
            route_kind: "collection".into(),
            focus_id: None,
            query: Some(query.to_string()),
        },
    }
}

fn score_id_field(value: &str, query: &str) -> Option<f64> {
    let value = value.to_ascii_lowercase();
    if value == query {
        Some(1.0)
    } else if value.starts_with(query) {
        Some(0.94)
    } else if value.contains(query) {
        Some(0.82)
    } else {
        None
    }
}

fn score_text_field(value: &str, query: &str) -> Option<f64> {
    let value = value.to_ascii_lowercase();
    if value == query {
        Some(0.92)
    } else if value.starts_with(query) {
        Some(0.84)
    } else if value.contains(query) {
        Some(0.68)
    } else {
        None
    }
}

fn push_match(matches: &mut BTreeSet<String>, field: &str, matched: bool) {
    if matched {
        matches.insert(field.to_string());
    }
}

fn snippet_from_text(value: &str, query: &str, max_chars: usize) -> String {
    if value.is_empty() {
        return String::new();
    }

    let value_lower = value.to_ascii_lowercase();
    let Some(start) = value_lower.find(query) else {
        return if value.chars().count() > max_chars {
            format!("{}…", truncate_chars(value, max_chars))
        } else {
            value.to_string()
        };
    };

    let prefix_chars = value[..start].chars().count();
    let start_chars = prefix_chars.saturating_sub(max_chars / 3);
    let end_chars = (start_chars + max_chars).min(value.chars().count());
    let snippet = value
        .chars()
        .skip(start_chars)
        .take(end_chars - start_chars)
        .collect::<String>();
    let mut rendered = String::new();
    if start_chars > 0 {
        rendered.push('…');
    }
    rendered.push_str(&snippet);
    if end_chars < value.chars().count() {
        rendered.push('…');
    }
    rendered
}

fn normalize_memory_score(raw: f64) -> f64 {
    if !raw.is_finite() {
        return 0.0;
    }
    let raw = raw.max(0.0);
    raw / (1.0 + raw)
}

fn stringify_json(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
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

    let requested_types = params
        .types
        .split(',')
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    validate_types(&requested_types)?;

    let escaped = escape_like(q);
    let like_pattern = format!("%{escaped}%");
    let per_type_limit = (params.limit as usize / requested_types.len().max(1)).max(10);

    let mut buckets = Vec::new();
    let mut warnings = Vec::new();

    if requested_types.contains(&"memories") {
        match search_memories_domain(state.as_ref(), q, per_type_limit as u32).await {
            Ok(bucket) => buckets.push(bucket),
            Err(message) => warnings.push(SearchDomainWarning {
                result_type: "memory".into(),
                message,
            }),
        }
    }

    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("search", e))?;

    if requested_types.contains(&"agents") {
        match search_agents(&db, q, &like_pattern, per_type_limit) {
            Ok(bucket) => buckets.push(bucket),
            Err(message) => warnings.push(SearchDomainWarning {
                result_type: "agent".into(),
                message,
            }),
        }
    }
    if requested_types.contains(&"sessions") {
        match search_sessions(&db, q, &like_pattern, per_type_limit) {
            Ok(bucket) => buckets.push(bucket),
            Err(message) => warnings.push(SearchDomainWarning {
                result_type: "session".into(),
                message,
            }),
        }
    }
    if requested_types.contains(&"proposals") {
        match search_proposals(&db, q, &like_pattern, per_type_limit) {
            Ok(bucket) => buckets.push(bucket),
            Err(message) => warnings.push(SearchDomainWarning {
                result_type: "proposal".into(),
                message,
            }),
        }
    }
    if requested_types.contains(&"audit") {
        match search_audit(&db, q, &like_pattern, per_type_limit) {
            Ok(bucket) => buckets.push(bucket),
            Err(message) => warnings.push(SearchDomainWarning {
                result_type: "audit".into(),
                message,
            }),
        }
    }

    let total = buckets.iter().map(|bucket| bucket.total_matches).sum();
    let totals_by_type = buckets
        .iter()
        .map(|bucket| SearchTypeCount {
            result_type: bucket.result_type.to_string(),
            total: bucket.total_matches,
        })
        .collect::<Vec<_>>();

    let mut results = buckets
        .into_iter()
        .flat_map(|bucket| bucket.results.into_iter())
        .collect::<Vec<_>>();
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.result_type.cmp(&right.result_type))
            .then_with(|| left.title.cmp(&right.title))
    });
    results.truncate(params.limit as usize);

    let returned_count = results.len();
    Ok(Json(SearchResponse {
        query: q.to_string(),
        total,
        returned_count,
        totals_by_type,
        degraded: !warnings.is_empty(),
        warnings,
        results,
    }))
}

fn search_agents(
    db: &rusqlite::Connection,
    query: &str,
    pattern: &str,
    limit: usize,
) -> Result<SearchBucket, String> {
    let total_matches: usize = db
        .query_row(
            "SELECT COUNT(*) FROM agents
             WHERE name LIKE ?1 ESCAPE '\\' OR id LIKE ?1 ESCAPE '\\'",
            [pattern],
            |row| row.get(0),
        )
        .map_err(|error| format!("agent count failed: {error}"))?;

    let mut stmt = db
        .prepare(
            "SELECT id, name FROM agents
             WHERE name LIKE ?1 ESCAPE '\\' OR id LIKE ?1 ESCAPE '\\'
             ORDER BY name ASC
             LIMIT ?2",
        )
        .map_err(|error| format!("agent query prepare failed: {error}"))?;

    let normalized_query = normalize_query(query);
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| format!("agent query failed: {error}"))?;

    let mut results = Vec::new();
    for row in rows {
        let (id, name) = row.map_err(|error| format!("agent row failed: {error}"))?;
        let mut matched_fields = BTreeSet::new();
        let mut score: f64 = 0.0;
        if let Some(field_score) = score_id_field(&id, &normalized_query) {
            score = score.max(field_score);
            push_match(&mut matched_fields, "id", true);
        }
        if let Some(field_score) = score_text_field(&name, &normalized_query) {
            score = score.max(field_score);
            push_match(&mut matched_fields, "name", true);
        }

        results.push(SearchResult {
            result_type: "agent".into(),
            id: id.clone(),
            title: name,
            snippet: id.clone(),
            score,
            navigation: navigation_for("agent", &id, query),
            match_context: SearchMatchContext {
                matched_fields: matched_fields.into_iter().collect(),
            },
        });
    }

    Ok(SearchBucket {
        result_type: "agent",
        total_matches,
        results,
    })
}

fn search_sessions(
    db: &rusqlite::Connection,
    query: &str,
    pattern: &str,
    limit: usize,
) -> Result<SearchBucket, String> {
    let total_matches: usize = db
        .query_row(
            "SELECT COUNT(DISTINCT session_id) FROM itp_events
             WHERE session_id LIKE ?1 ESCAPE '\\'
                OR COALESCE(sender, '') LIKE ?1 ESCAPE '\\'
                OR COALESCE(attributes, '') LIKE ?1 ESCAPE '\\'",
            [pattern],
            |row| row.get(0),
        )
        .map_err(|error| format!("session count failed: {error}"))?;

    let mut stmt = db
        .prepare(
            "SELECT session_id,
                    GROUP_CONCAT(DISTINCT NULLIF(COALESCE(sender, ''), '')) AS agents,
                    MAX(CASE WHEN session_id LIKE ?1 ESCAPE '\\' THEN 1 ELSE 0 END) AS id_match,
                    MAX(CASE WHEN COALESCE(sender, '') LIKE ?1 ESCAPE '\\' THEN 1 ELSE 0 END) AS sender_match,
                    MAX(CASE WHEN COALESCE(attributes, '') LIKE ?1 ESCAPE '\\' THEN 1 ELSE 0 END) AS attributes_match,
                    MAX(timestamp) AS last_event_at
             FROM itp_events
             WHERE session_id LIKE ?1 ESCAPE '\\'
                OR COALESCE(sender, '') LIKE ?1 ESCAPE '\\'
                OR COALESCE(attributes, '') LIKE ?1 ESCAPE '\\'
             GROUP BY session_id
             ORDER BY last_event_at DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("session query prepare failed: {error}"))?;

    let normalized_query = normalize_query(query);
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                row.get::<_, i64>(2)? != 0,
                row.get::<_, i64>(3)? != 0,
                row.get::<_, i64>(4)? != 0,
            ))
        })
        .map_err(|error| format!("session query failed: {error}"))?;

    let mut results = Vec::new();
    for row in rows {
        let (session_id, agents_csv, id_match, sender_match, attributes_match) =
            row.map_err(|error| format!("session row failed: {error}"))?;
        let mut matched_fields = BTreeSet::new();
        let mut score: f64 = 0.0;

        if id_match {
            if let Some(field_score) = score_id_field(&session_id, &normalized_query) {
                score = score.max(field_score);
            }
            matched_fields.insert("session_id".into());
        }

        let agents = agents_csv
            .split(',')
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();

        if sender_match {
            for agent in &agents {
                if let Some(field_score) = score_text_field(agent, &normalized_query) {
                    score = score.max(field_score);
                }
            }
            matched_fields.insert("sender".into());
        }

        if attributes_match {
            score = score.max(0.7);
            matched_fields.insert("attributes".into());
        }

        let snippet = if attributes_match {
            "Matched session content".to_string()
        } else if sender_match && !agents.is_empty() {
            format!("Agents: {}", agents.join(", "))
        } else {
            format!("Session ID: {session_id}")
        };

        results.push(SearchResult {
            result_type: "session".into(),
            id: session_id.clone(),
            title: format!("Session {}", truncate_chars(&session_id, 12)),
            snippet,
            score,
            navigation: navigation_for("session", &session_id, query),
            match_context: SearchMatchContext {
                matched_fields: matched_fields.into_iter().collect(),
            },
        });
    }
    results.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);

    Ok(SearchBucket {
        result_type: "session",
        total_matches,
        results,
    })
}

async fn search_memories_domain(
    state: &AppState,
    query: &str,
    limit: u32,
) -> Result<SearchBucket, String> {
    let executed = execute_memory_search(
        state,
        MemorySearchParams {
            q: Some(query.to_string()),
            agent_id: None,
            memory_type: None,
            importance: None,
            confidence_min: None,
            confidence_max: None,
            limit: Some(limit),
            include_archived: Some(false),
        },
    )
    .await
    .map_err(|error| format!("memory search failed: {error}"))?;
    let total_matches = executed.total_matches;

    let normalized_query = normalize_query(query);
    let results = executed
        .response
        .results
        .into_iter()
        .map(|memory| {
            let snapshot_text = stringify_json(&memory.snapshot);
            let summary = memory
                .snapshot
                .get("summary")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let title = if !summary.is_empty() {
                summary.clone()
            } else {
                memory
                    .snapshot
                    .get("memory_type")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("Memory")
                    .to_string()
            };
            let snippet = if !summary.is_empty() {
                summary
            } else {
                snippet_from_text(&snapshot_text, &normalized_query, 140)
            };

            let mut matched_fields = BTreeSet::new();
            let mut score = normalize_memory_score(memory.score);
            if let Some(field_score) = score_id_field(&memory.memory_id, &normalized_query) {
                score = score.max(field_score);
                matched_fields.insert("memory_id".into());
            }
            if score_text_field(&snapshot_text, &normalized_query).is_some() {
                matched_fields.insert("snapshot".into());
            }
            if memory
                .snapshot
                .get("summary")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|summary| score_text_field(summary, &normalized_query).is_some())
            {
                matched_fields.insert("summary".into());
            }

            SearchResult {
                result_type: "memory".into(),
                id: memory.memory_id.clone(),
                title,
                snippet,
                score,
                navigation: navigation_for("memory", &memory.memory_id, query),
                match_context: SearchMatchContext {
                    matched_fields: matched_fields.into_iter().collect(),
                },
            }
        })
        .collect::<Vec<_>>();

    Ok(SearchBucket {
        result_type: "memory",
        total_matches,
        results,
    })
}

fn search_proposals(
    db: &rusqlite::Connection,
    query: &str,
    pattern: &str,
    limit: usize,
) -> Result<SearchBucket, String> {
    let total_matches: usize = db
        .query_row(
            "SELECT COUNT(*) FROM goal_proposals
             WHERE id LIKE ?1 ESCAPE '\\'
                OR operation LIKE ?1 ESCAPE '\\'
                OR agent_id LIKE ?1 ESCAPE '\\'
                OR target_type LIKE ?1 ESCAPE '\\'
                OR COALESCE(content, '') LIKE ?1 ESCAPE '\\'",
            [pattern],
            |row| row.get(0),
        )
        .map_err(|error| format!("proposal count failed: {error}"))?;

    let mut stmt = db
        .prepare(
            "SELECT id, operation, decision, agent_id, target_type, COALESCE(content, '')
             FROM goal_proposals
             WHERE id LIKE ?1 ESCAPE '\\'
                OR operation LIKE ?1 ESCAPE '\\'
                OR agent_id LIKE ?1 ESCAPE '\\'
                OR target_type LIKE ?1 ESCAPE '\\'
                OR COALESCE(content, '') LIKE ?1 ESCAPE '\\'
             ORDER BY created_at DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("proposal query prepare failed: {error}"))?;

    let normalized_query = normalize_query(query);
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| format!("proposal query failed: {error}"))?;

    let mut results = Vec::new();
    for row in rows {
        let (id, operation, decision, agent_id, target_type, content) =
            row.map_err(|error| format!("proposal row failed: {error}"))?;
        let mut matched_fields = BTreeSet::new();
        let mut score: f64 = 0.0;
        for (field, value, scorer) in [
            (
                "id",
                id.as_str(),
                score_id_field as fn(&str, &str) -> Option<f64>,
            ),
            ("operation", operation.as_str(), score_text_field),
            ("agent_id", agent_id.as_str(), score_text_field),
            ("target_type", target_type.as_str(), score_text_field),
            ("content", content.as_str(), score_text_field),
        ] {
            if let Some(field_score) = scorer(value, &normalized_query) {
                score = score.max(field_score);
                matched_fields.insert(field.to_string());
            }
        }

        let snippet = if score_text_field(&content, &normalized_query).is_some() {
            snippet_from_text(&content, &normalized_query, 140)
        } else {
            format!("Status: {decision} | Agent: {agent_id} | Target: {target_type}")
        };

        results.push(SearchResult {
            result_type: "proposal".into(),
            id: id.clone(),
            title: operation,
            snippet,
            score,
            navigation: navigation_for("proposal", &id, query),
            match_context: SearchMatchContext {
                matched_fields: matched_fields.into_iter().collect(),
            },
        });
    }

    Ok(SearchBucket {
        result_type: "proposal",
        total_matches,
        results,
    })
}

fn search_audit(
    db: &rusqlite::Connection,
    query: &str,
    pattern: &str,
    limit: usize,
) -> Result<SearchBucket, String> {
    let total_matches: usize = db
        .query_row(
            "SELECT COUNT(*) FROM audit_log
             WHERE event_type LIKE ?1 ESCAPE '\\'
                OR details LIKE ?1 ESCAPE '\\'
                OR agent_id LIKE ?1 ESCAPE '\\'
                OR COALESCE(tool_name, '') LIKE ?1 ESCAPE '\\'",
            [pattern],
            |row| row.get(0),
        )
        .map_err(|error| format!("audit count failed: {error}"))?;

    let mut stmt = db
        .prepare(
            "SELECT id, event_type, details, agent_id, tool_name
             FROM audit_log
             WHERE event_type LIKE ?1 ESCAPE '\\'
                OR details LIKE ?1 ESCAPE '\\'
                OR agent_id LIKE ?1 ESCAPE '\\'
                OR COALESCE(tool_name, '') LIKE ?1 ESCAPE '\\'
             ORDER BY timestamp DESC
             LIMIT ?2",
        )
        .map_err(|error| format!("audit query prepare failed: {error}"))?;

    let normalized_query = normalize_query(query);
    let rows = stmt
        .query_map(rusqlite::params![pattern, limit], |row| {
            Ok(SearchableAuditRow {
                id: row.get(0)?,
                event_type: row.get(1)?,
                details: row.get(2)?,
                agent_id: row.get(3)?,
                tool_name: row.get(4)?,
            })
        })
        .map_err(|error| format!("audit query failed: {error}"))?;

    let mut results = Vec::new();
    for row in rows {
        let row = row.map_err(|error| format!("audit row failed: {error}"))?;
        let mut matched_fields = BTreeSet::new();
        let mut score: f64 = 0.0;
        for (field, value) in [
            ("event_type", row.event_type.as_str()),
            ("details", row.details.as_str()),
            ("agent_id", row.agent_id.as_str()),
            ("tool_name", row.tool_name.as_deref().unwrap_or("")),
        ] {
            if let Some(field_score) = score_text_field(value, &normalized_query) {
                score = score.max(field_score);
                matched_fields.insert(field.to_string());
            }
        }
        let snippet = if score_text_field(&row.details, &normalized_query).is_some() {
            snippet_from_text(&row.details, &normalized_query, 140)
        } else {
            row.details.clone()
        };

        results.push(SearchResult {
            result_type: "audit".into(),
            id: row.id.clone(),
            title: row.event_type,
            snippet,
            score,
            navigation: navigation_for("audit", &row.id, query),
            match_context: SearchMatchContext {
                matched_fields: matched_fields.into_iter().collect(),
            },
        });
    }

    Ok(SearchBucket {
        result_type: "audit",
        total_matches,
        results,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ghost_audit::{AuditEntry, AuditQueryEngine};
    use rusqlite::params;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::run_all_migrations(&conn).unwrap();
        conn
    }

    #[test]
    fn search_sessions_matches_sender_and_attributes() {
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
                r#"{"content":"marker from attributes"}"#,
            ],
        )
        .unwrap();

        let sender_bucket = search_sessions(&conn, "agent-search", "%agent-search%", 10).unwrap();
        assert_eq!(sender_bucket.results.len(), 1);
        assert_eq!(sender_bucket.results[0].id, "session-knowledge");
        assert!(sender_bucket.results[0]
            .match_context
            .matched_fields
            .contains(&"sender".to_string()));

        let attr_bucket = search_sessions(
            &conn,
            "marker from attributes",
            "%marker from attributes%",
            10,
        )
        .unwrap();
        assert_eq!(attr_bucket.results.len(), 1);
        assert!(attr_bucket.results[0]
            .match_context
            .matched_fields
            .contains(&"attributes".to_string()));
    }

    #[test]
    fn search_proposals_matches_content_and_target_type() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO goal_proposals (
                id, agent_id, session_id, proposer_type, operation, target_type, content,
                cited_memory_ids, decision, event_hash, previous_hash, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                "proposal-1",
                "agent-alpha",
                "session-1",
                "agent",
                "update_goal",
                "workspace",
                r#"{"goal_text":"ship the search remediation milestone"}"#,
                "[]",
                "pending_review",
                vec![2u8; 32],
                vec![1u8; 32],
                "2026-03-09T00:00:00Z",
            ],
        )
        .unwrap();

        let bucket = search_proposals(
            &conn,
            "search remediation milestone",
            "%search remediation milestone%",
            10,
        )
        .unwrap();
        assert_eq!(bucket.results.len(), 1);
        assert!(bucket.results[0]
            .match_context
            .matched_fields
            .contains(&"content".to_string()));
    }

    #[test]
    fn search_audit_builds_focus_navigation() {
        let conn = setup_db();
        let audit = AuditQueryEngine::new(&conn);
        audit
            .insert(&AuditEntry {
                id: "audit-1".into(),
                timestamp: "2026-03-09T00:00:00Z".into(),
                agent_id: "agent-1".into(),
                event_type: "memory_write".into(),
                severity: "info".into(),
                tool_name: Some("memory".into()),
                details: "marker audit entry".into(),
                session_id: Some("session-1".into()),
                actor_id: None,
                operation_id: None,
                request_id: None,
                idempotency_key: None,
                idempotency_status: None,
            })
            .unwrap();

        let bucket = search_audit(&conn, "marker audit entry", "%marker audit entry%", 10).unwrap();
        assert_eq!(bucket.results.len(), 1);
        assert_eq!(
            bucket.results[0].navigation.href,
            "/security?search=marker+audit+entry&focus=audit-1"
        );
    }

    #[test]
    fn build_query_path_omits_empty_values() {
        assert_eq!(
            build_query_path("/memory", &[("q", Some("needle")), ("focus", None)]),
            "/memory?q=needle"
        );
    }
}
