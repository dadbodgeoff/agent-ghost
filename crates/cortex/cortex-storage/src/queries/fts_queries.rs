//! FTS5 full-text search queries (v031 memory_fts table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

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
        "AND ms.memory_id NOT IN (SELECT memory_id FROM memory_archival_log)"
    };

    let sql = format!(
        "SELECT ms.id, ms.memory_id, ms.snapshot, ms.created_at,
                bm25(memory_fts) AS fts_rank
         FROM memory_fts f
         JOIN memory_snapshots ms ON f.memory_id = ms.memory_id
         WHERE memory_fts MATCH ?1
         {archived_filter}
         GROUP BY ms.memory_id
         ORDER BY fts_rank
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

    Ok(rows)
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
        .split_whitespace()
        .filter(|w| !w.is_empty())
        .map(|w| {
            // Remove FTS5 operators and special chars
            let clean: String = w
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"")
            }
        })
        .filter(|s| !s.is_empty())
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
}
