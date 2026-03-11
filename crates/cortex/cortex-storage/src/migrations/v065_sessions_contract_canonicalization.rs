//! Migration v065: canonicalize runtime session bookmarks and branch lineage.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const SESSION_BOOKMARK_COLUMNS: &[&str] =
    &["id", "session_id", "sequence_number", "label", "created_at"];
const LEGACY_SESSION_BOOKMARK_COLUMNS: &[&str] =
    &["id", "session_id", "event_index", "label", "created_at"];
const SESSION_BRANCH_COLUMNS: &[&str] = &[
    "session_id",
    "source_session_id",
    "source_sequence_number",
    "created_at",
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    canonicalize_session_bookmarks(conn)?;
    canonicalize_session_branches(conn)?;
    Ok(())
}

fn canonicalize_session_bookmarks(conn: &Connection) -> CortexResult<()> {
    if has_table(conn, "session_bookmarks")? {
        let columns = table_columns(conn, "session_bookmarks")?;
        let is_canonical = SESSION_BOOKMARK_COLUMNS
            .iter()
            .all(|column| columns.contains(*column));
        let is_legacy = LEGACY_SESSION_BOOKMARK_COLUMNS
            .iter()
            .all(|column| columns.contains(*column));

        if !is_canonical && !is_legacy {
            return Err(to_storage_err(format!(
                "unsupported legacy session_bookmarks shape: found columns [{}]",
                columns.into_iter().collect::<Vec<_>>().join(", ")
            )));
        }

        if !is_canonical {
            conn.execute_batch(
                "ALTER TABLE session_bookmarks RENAME TO session_bookmarks_legacy;

                 CREATE TABLE session_bookmarks (
                     id TEXT PRIMARY KEY,
                     session_id TEXT NOT NULL,
                     sequence_number INTEGER NOT NULL,
                     label TEXT NOT NULL,
                     created_at TEXT NOT NULL DEFAULT (datetime('now'))
                 );

                 INSERT INTO session_bookmarks (id, session_id, sequence_number, label, created_at)
                 SELECT id, session_id, event_index, label, COALESCE(created_at, datetime('now'))
                 FROM session_bookmarks_legacy;

                 DROP TABLE session_bookmarks_legacy;",
            )
            .map_err(|error| {
                to_storage_err(format!("v065 canonicalize session_bookmarks: {error}"))
            })?;
        }
    } else {
        conn.execute_batch(
            "CREATE TABLE session_bookmarks (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 sequence_number INTEGER NOT NULL,
                 label TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT (datetime('now'))
             );",
        )
        .map_err(|error| to_storage_err(format!("v065 create session_bookmarks: {error}")))?;
    }

    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_session_bookmarks_session ON session_bookmarks(session_id);
         CREATE INDEX IF NOT EXISTS idx_session_bookmarks_session_sequence
             ON session_bookmarks(session_id, sequence_number);",
    )
    .map_err(|error| to_storage_err(format!("v065 index session_bookmarks: {error}")))?;

    Ok(())
}

fn canonicalize_session_branches(conn: &Connection) -> CortexResult<()> {
    if has_table(conn, "session_branches")? {
        let columns = table_columns(conn, "session_branches")?;
        let missing = SESSION_BRANCH_COLUMNS
            .iter()
            .filter(|column| !columns.contains(**column))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(to_storage_err(format!(
                "unsupported legacy session_branches shape: missing required columns [{}]",
                missing.join(", ")
            )));
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_branches (
             session_id TEXT PRIMARY KEY,
             source_session_id TEXT NOT NULL,
             source_sequence_number INTEGER NOT NULL,
             created_at TEXT NOT NULL DEFAULT (datetime('now'))
         );

         CREATE INDEX IF NOT EXISTS idx_session_branches_source
             ON session_branches(source_session_id);",
    )
    .map_err(|error| to_storage_err(format!("v065 canonicalize session_branches: {error}")))?;

    Ok(())
}

fn has_table(conn: &Connection, table: &str) -> CortexResult<bool> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
        [table],
        |_row| Ok(()),
    )
    .map(|_| true)
    .or_else(|error| match error {
        rusqlite::Error::QueryReturnedNoRows => Ok(false),
        other => Err(other),
    })
    .map_err(|error| to_storage_err(error.to_string()))
}

fn table_columns(conn: &Connection, table: &str) -> CortexResult<BTreeSet<String>> {
    let sql = format!("PRAGMA table_info('{table}')");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|error| to_storage_err(error.to_string()))?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| to_storage_err(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| to_storage_err(error.to_string()))?;
    Ok(rows.into_iter().collect())
}
