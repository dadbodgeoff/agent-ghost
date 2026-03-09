//! FTS5 full-text search queries (v031 memory_fts table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};
use std::collections::HashSet;

/// Search memories using FTS5 with BM25 ranking.
///
/// Returns results ordered by relevance (best match first).
/// The `query` parameter uses FTS5 match syntax.
pub fn fts_search(
    conn: &Connection,
    query: &str,
    limit: u32,
    include_archived: bool,
) -> CortexResult<Vec<FtsResult>> {
    // Sanitize query for FTS5: escape double quotes, strip bare special chars.
    let safe_query = sanitize_fts_query(query);
    if safe_query.is_empty() {
        return Ok(vec![]);
    }

    let archived_filter = if include_archived {
        ""
    } else {
        "AND latest.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)"
    };

    let sql = format!(
        "SELECT latest.id, latest.memory_id, latest.snapshot, latest.created_at,
                bm25(memory_fts) AS fts_rank
         FROM memory_fts
         JOIN (
             SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at
             FROM memory_snapshots ms
             JOIN (
                 SELECT memory_id, MAX(id) AS max_id
                 FROM memory_snapshots
                 GROUP BY memory_id
             ) newest ON newest.max_id = ms.id
         ) latest ON latest.memory_id = memory_fts.memory_id
         WHERE memory_fts MATCH ?1
         {archived_filter}
         ORDER BY fts_rank, latest.id DESC
         LIMIT ?2"
    );

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![safe_query, limit], |row| {
            Ok(FtsResult {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                snapshot: row.get(2)?,
                created_at: row.get(3)?,
                fts_rank: row.get(4)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut seen_memory_ids = HashSet::new();
    let deduped = rows
        .into_iter()
        .filter(|row| seen_memory_ids.insert(row.memory_id.clone()))
        .collect();

    Ok(deduped)
}

/// Check if the FTS5 table exists (for graceful fallback).
pub fn fts_available(conn: &Connection) -> bool {
    conn.prepare("SELECT * FROM memory_fts LIMIT 0").is_ok()
}

/// Sanitize a query string for FTS5 MATCH syntax.
///
/// Wraps each word in double quotes to prevent FTS5 syntax errors
/// from user input containing special characters.
fn sanitize_fts_query(query: &str) -> String {
    query
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\""))
        .collect::<Vec<_>>()
        .join(" ")
}

#[derive(Debug, Clone)]
pub struct FtsResult {
    pub id: i64,
    pub memory_id: String,
    pub snapshot: String,
    pub created_at: String,
    pub fts_rank: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_simple_query() {
        assert_eq!(sanitize_fts_query("hello world"), "\"hello\" \"world\"");
    }

    #[test]
    fn sanitize_strips_operators() {
        assert_eq!(sanitize_fts_query("NOT OR AND"), "\"NOT\" \"OR\" \"AND\"");
    }

    #[test]
    fn sanitize_empty() {
        assert_eq!(sanitize_fts_query(""), "");
    }

    #[test]
    fn sanitize_special_chars() {
        assert_eq!(sanitize_fts_query("hello* (world)"), "\"hello\" \"world\"");
    }

    #[test]
    fn sanitize_hyphenated_query() {
        assert_eq!(
            sanitize_fts_query("knowledge-live-20260309-131129"),
            "\"knowledge\" \"live\" \"20260309\" \"131129\""
        );
    }

    #[test]
    fn fts_search_returns_latest_snapshot_once_per_memory() {
        let conn = Connection::open_in_memory().unwrap();
        crate::run_all_migrations(&conn).unwrap();

        crate::queries::memory_snapshot_queries::insert_snapshot(
            &conn,
            "memory-alpha",
            r#"{"summary":"older alpha summary","content":"knowledge alpha","tags":["knowledge"]}"#,
            None,
        )
        .unwrap();
        crate::queries::memory_snapshot_queries::insert_snapshot(
            &conn,
            "memory-alpha",
            r#"{"summary":"latest alpha summary","content":"knowledge alpha latest","tags":["knowledge"]}"#,
            None,
        )
        .unwrap();
        crate::queries::memory_snapshot_queries::insert_snapshot(
            &conn,
            "memory-beta",
            r#"{"summary":"beta summary","content":"knowledge beta","tags":["knowledge"]}"#,
            None,
        )
        .unwrap();

        let results = fts_search(&conn, "knowledge", 10, true).unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().any(|row| {
            row.memory_id == "memory-alpha" && row.snapshot.contains("latest alpha summary")
        }));
        assert!(results.iter().any(|row| row.memory_id == "memory-beta"));
    }
}
