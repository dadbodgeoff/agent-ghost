//! Migration v047: adopt canonical monitor threshold tables.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const CONFIG_COLUMNS: &[&str] = &[
    "config_key",
    "critical_override_threshold",
    "updated_at",
    "updated_by",
    "confirmed_by",
];
const HISTORY_COLUMNS: &[&str] = &[
    "id",
    "config_key",
    "previous_value",
    "new_value",
    "initiated_by",
    "confirmed_by",
    "change_mode",
    "changed_at",
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    verify_supported_shape(conn, "monitor_threshold_config", CONFIG_COLUMNS)?;
    verify_supported_shape(conn, "monitor_threshold_history", HISTORY_COLUMNS)?;

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS monitor_threshold_config (
            config_key TEXT PRIMARY KEY,
            critical_override_threshold REAL NOT NULL,
            updated_at TEXT NOT NULL,
            updated_by TEXT NOT NULL,
            confirmed_by TEXT
        );

        CREATE TABLE IF NOT EXISTS monitor_threshold_history (
            id TEXT PRIMARY KEY,
            config_key TEXT NOT NULL,
            previous_value REAL NOT NULL,
            new_value REAL NOT NULL,
            initiated_by TEXT NOT NULL,
            confirmed_by TEXT,
            change_mode TEXT NOT NULL,
            changed_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_monitor_threshold_history_key_changed_at
            ON monitor_threshold_history(config_key, changed_at DESC);",
    )
    .map_err(|error| to_storage_err(format!("v047 monitor threshold tables: {error}")))?;

    Ok(())
}

fn verify_supported_shape(
    conn: &Connection,
    table: &str,
    required_columns: &[&str],
) -> CortexResult<()> {
    if !has_table(conn, table)? {
        return Ok(());
    }

    let columns = table_columns(conn, table)?;
    let missing = required_columns
        .iter()
        .filter(|column| !columns.contains(**column))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }

    Err(to_storage_err(format!(
        "unsupported legacy {table} shape: missing required columns [{}]",
        missing.join(", ")
    )))
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
