//! Migration v050: mutable convergence profile assignment table.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const PROFILE_ASSIGNMENT_COLUMNS: &[&str] = &[
    "agent_id",
    "profile_name",
    "created_at",
    "updated_at",
    "updated_by",
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if has_table(conn, "agent_profile_assignments")? {
        let columns = table_columns(conn, "agent_profile_assignments")?;
        let missing = PROFILE_ASSIGNMENT_COLUMNS
            .iter()
            .filter(|column| !columns.contains(**column))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(to_storage_err(format!(
                "unsupported legacy agent_profile_assignments shape: missing required columns [{}]",
                missing.join(", ")
            )));
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_profile_assignments (
            agent_id TEXT PRIMARY KEY,
            profile_name TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_by TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_agent_profile_assignments_profile
            ON agent_profile_assignments(profile_name);",
    )
    .map_err(|error| to_storage_err(format!("v050 create profile assignments: {error}")))?;

    conn.execute(
        "INSERT INTO agent_profile_assignments (agent_id, profile_name, created_at, updated_at, updated_by)
         SELECT cs.agent_id,
                cs.profile,
                COALESCE(cs.created_at, datetime('now')),
                COALESCE(cs.computed_at, datetime('now')),
                'migration:v050_profile_assignments'
         FROM convergence_scores AS cs
         WHERE cs.rowid IN (
                SELECT MAX(rowid)
                FROM convergence_scores
                WHERE profile IS NOT NULL AND trim(profile) <> '' AND profile <> 'standard'
                GROUP BY agent_id
         )
         ON CONFLICT(agent_id) DO NOTHING",
        [],
    )
    .map_err(|error| to_storage_err(format!("v050 backfill profile assignments: {error}")))?;

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
