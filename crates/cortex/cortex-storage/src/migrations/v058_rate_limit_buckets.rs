//! Migration v058: shared rate-limit buckets for database-scoped enforcement.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS rate_limit_buckets (
            scope          TEXT NOT NULL,
            subject_key    TEXT NOT NULL,
            bucket_start   INTEGER NOT NULL,
            window_seconds INTEGER NOT NULL CHECK(window_seconds > 0),
            request_count  INTEGER NOT NULL DEFAULT 0 CHECK(request_count >= 0),
            updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
            PRIMARY KEY (scope, subject_key, bucket_start)
        );

        CREATE INDEX IF NOT EXISTS idx_rate_limit_buckets_updated_at
            ON rate_limit_buckets(updated_at);",
    )
    .map_err(|error| to_storage_err(format!("v058 rate_limit_buckets: {error}")))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_rate_limit_bucket_table() {
        let conn = Connection::open_in_memory().unwrap();
        migrate(&conn).unwrap();

        let columns = conn
            .prepare("SELECT name FROM pragma_table_info('rate_limit_buckets')")
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();

        for column in [
            "scope",
            "subject_key",
            "bucket_start",
            "window_seconds",
            "request_count",
            "updated_at",
        ] {
            assert!(columns.contains(&column.to_string()), "missing {column}");
        }
    }
}
