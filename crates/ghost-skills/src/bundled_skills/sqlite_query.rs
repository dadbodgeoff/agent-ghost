//! `sqlite_query` — read-only SQLite queries against user databases.
//!
//! Executes SELECT queries against a user-specified SQLite database file.
//! Only read-only operations are permitted — the connection is opened
//! with `SQLITE_OPEN_READ_ONLY` to enforce this at the SQLite level.
//!
//! ## Input
//!
//! | Field       | Type   | Required | Default | Description                     |
//! |-------------|--------|----------|---------|---------------------------------|
//! | `db_path`   | string | yes      | —       | Absolute path to SQLite database |
//! | `query`     | string | yes      | —       | SQL SELECT query                |
//! | `params`    | array  | no       | `[]`    | Positional bind parameters      |
//! | `limit`     | int    | no       | 100     | Max rows to return              |
//!
//! ## Safety
//!
//! - Database is opened `SQLITE_OPEN_READ_ONLY` — no writes possible
//! - Queries are validated to start with SELECT (case-insensitive)
//! - Dangerous SQL keywords are rejected (DROP, DELETE, INSERT, UPDATE, etc.)
//! - Row limit is enforced to prevent memory exhaustion

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

pub struct SqliteQuerySkill;

const DEFAULT_ROW_LIMIT: u64 = 100;
const MAX_ROW_LIMIT: u64 = 10_000;

impl Skill for SqliteQuerySkill {
    fn name(&self) -> &str {
        "sqlite_query"
    }

    fn description(&self) -> &str {
        "Execute read-only SQL queries against a SQLite database"
    }

    fn removable(&self) -> bool {
        true
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let db_path = input
            .get("db_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'db_path'".into())
            })?;

        let query = input
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'query'".into())
            })?;

        let limit = input
            .get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(DEFAULT_ROW_LIMIT)
            .min(MAX_ROW_LIMIT);

        let params = input
            .get("params")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        // Validate query is read-only.
        validate_read_only(query)?;

        // Verify database file exists.
        if !std::path::Path::new(db_path).exists() {
            return Err(SkillError::InvalidInput(format!(
                "database file not found: '{db_path}'"
            )));
        }

        // Open database in read-only mode.
        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| {
            SkillError::Internal(format!("cannot open database '{db_path}': {e}"))
        })?;

        // Apply the row limit via LIMIT clause if not already present.
        let effective_query = if query.to_uppercase().contains("LIMIT") {
            query.to_string()
        } else {
            format!("{query} LIMIT {limit}")
        };

        let mut stmt = conn.prepare(&effective_query).map_err(|e| {
            SkillError::InvalidInput(format!("invalid SQL: {e}"))
        })?;

        // Bind parameters.
        let param_refs: Vec<Box<dyn rusqlite::types::ToSql>> = params
            .iter()
            .map(|v| -> Box<dyn rusqlite::types::ToSql> {
                match v {
                    serde_json::Value::Null => Box::new(rusqlite::types::Null),
                    serde_json::Value::Bool(b) => Box::new(*b),
                    serde_json::Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i)
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f)
                        } else {
                            Box::new(n.to_string())
                        }
                    }
                    serde_json::Value::String(s) => Box::new(s.clone()),
                    _ => Box::new(v.to_string()),
                }
            })
            .collect();

        let param_slice: Vec<&dyn rusqlite::types::ToSql> =
            param_refs.iter().map(|p| p.as_ref()).collect();

        // Get column names.
        let column_names: Vec<String> = stmt
            .column_names()
            .iter()
            .map(|&s| s.to_string())
            .collect();

        // Execute and collect rows.
        let rows = stmt
            .query_map(param_slice.as_slice(), |row| {
                let mut obj = serde_json::Map::new();
                for (i, col_name) in column_names.iter().enumerate() {
                    let value = match row.get_ref(i) {
                        Ok(rusqlite::types::ValueRef::Null) => serde_json::Value::Null,
                        Ok(rusqlite::types::ValueRef::Integer(n)) => serde_json::json!(n),
                        Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                        Ok(rusqlite::types::ValueRef::Text(s)) => {
                            serde_json::json!(String::from_utf8_lossy(s))
                        }
                        Ok(rusqlite::types::ValueRef::Blob(b)) => {
                            serde_json::json!(format!("<blob {} bytes>", b.len()))
                        }
                        Err(_) => serde_json::Value::Null,
                    };
                    obj.insert(col_name.clone(), value);
                }
                Ok(serde_json::Value::Object(obj))
            })
            .map_err(|e| SkillError::Internal(format!("query execution failed: {e}")))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| SkillError::Internal(format!("row iteration failed: {e}")))?;

        Ok(serde_json::json!({
            "db_path": db_path,
            "query": query,
            "columns": column_names,
            "rows": rows,
            "row_count": rows.len(),
        }))
    }
}

/// Validate that a query is read-only.
///
/// Rejects any query that doesn't start with SELECT or contains
/// dangerous SQL keywords that could modify data or schema.
fn validate_read_only(query: &str) -> Result<(), SkillError> {
    let normalized = query.trim().to_uppercase();

    // Must start with SELECT, WITH, EXPLAIN, or PRAGMA.
    let allowed_starts = ["SELECT", "WITH", "EXPLAIN", "PRAGMA"];
    if !allowed_starts.iter().any(|s| normalized.starts_with(s)) {
        return Err(SkillError::AuthorizationDenied(format!(
            "only SELECT queries are allowed (got: {})",
            &normalized[..normalized.len().min(20)]
        )));
    }

    // Reject dangerous keywords anywhere in the query.
    let forbidden = [
        "INSERT", "UPDATE", "DELETE", "DROP", "ALTER", "CREATE",
        "ATTACH", "DETACH", "VACUUM", "REINDEX", "REPLACE",
    ];
    // Split on whitespace and check each token to avoid false positives
    // in string literals or column names.
    let tokens: Vec<&str> = normalized.split_whitespace().collect();
    for &keyword in &forbidden {
        if tokens.contains(&keyword) {
            return Err(SkillError::AuthorizationDenied(format!(
                "query contains forbidden keyword: {keyword}"
            )));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    fn create_test_sqlite_db() -> (std::path::PathBuf, rusqlite::Connection) {
        let path = std::env::temp_dir().join(format!("ghost-sqlite-test-{}.db", Uuid::now_v7()));
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER);
             INSERT INTO users VALUES (1, 'Alice', 30);
             INSERT INTO users VALUES (2, 'Bob', 25);
             INSERT INTO users VALUES (3, 'Charlie', 35);",
        )
        .unwrap();
        (path, conn)
    }

    #[test]
    fn query_simple_select() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let (sqlite_path, _conn) = create_test_sqlite_db();

        let result = SqliteQuerySkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "db_path": sqlite_path.to_str().unwrap(),
                    "query": "SELECT * FROM users ORDER BY id",
                }),
            )
            .unwrap();

        assert_eq!(result["row_count"], 3);
        assert_eq!(result["rows"][0]["name"], "Alice");
        assert_eq!(result["rows"][1]["age"], 25);

        let _ = std::fs::remove_file(&sqlite_path);
    }

    #[test]
    fn query_with_params() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let (sqlite_path, _conn) = create_test_sqlite_db();

        let result = SqliteQuerySkill
            .execute(
                &ctx,
                &serde_json::json!({
                    "db_path": sqlite_path.to_str().unwrap(),
                    "query": "SELECT * FROM users WHERE age > ?1",
                    "params": [28],
                }),
            )
            .unwrap();

        assert_eq!(result["row_count"], 2);

        let _ = std::fs::remove_file(&sqlite_path);
    }

    #[test]
    fn rejects_write_query() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let (sqlite_path, _conn) = create_test_sqlite_db();

        let result = SqliteQuerySkill.execute(
            &ctx,
            &serde_json::json!({
                "db_path": sqlite_path.to_str().unwrap(),
                "query": "INSERT INTO users VALUES (4, 'Dave', 40)",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::AuthorizationDenied(msg) => {
                assert!(msg.contains("SELECT"));
            }
            other => panic!("Expected AuthorizationDenied, got: {other:?}"),
        }

        let _ = std::fs::remove_file(&sqlite_path);
    }

    #[test]
    fn rejects_drop_in_select() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = validate_read_only("SELECT * FROM users; DROP TABLE users");
        assert!(result.is_err());
    }

    #[test]
    fn allows_with_cte() {
        let result = validate_read_only(
            "WITH cte AS (SELECT 1 AS x) SELECT * FROM cte",
        );
        assert!(result.is_ok());
    }

    #[test]
    fn rejects_nonexistent_db() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SqliteQuerySkill.execute(
            &ctx,
            &serde_json::json!({
                "db_path": "/nonexistent/db.sqlite",
                "query": "SELECT 1",
            }),
        );
        assert!(result.is_err());
    }

    #[test]
    fn skill_metadata() {
        assert_eq!(SqliteQuerySkill.name(), "sqlite_query");
        assert!(SqliteQuerySkill.removable());
        assert_eq!(SqliteQuerySkill.source(), SkillSource::Bundled);
    }
}
