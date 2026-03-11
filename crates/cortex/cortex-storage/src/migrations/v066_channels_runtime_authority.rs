//! v066: channels runtime authority columns and routing-key uniqueness.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const REQUIRED_COLUMNS: &[(&str, &str)] = &[
    ("routing_key", "TEXT NOT NULL DEFAULT ''"),
    ("source", "TEXT NOT NULL DEFAULT 'operator_created'"),
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    let columns = table_columns(conn, "channels")?;
    for (column, column_type) in REQUIRED_COLUMNS {
        if columns.contains(*column) {
            continue;
        }
        let sql = format!("ALTER TABLE channels ADD COLUMN {column} {column_type};");
        conn.execute_batch(&sql)
            .map_err(|error| to_storage_err(format!("v066 add channels.{column}: {error}")))?;
    }

    conn.execute_batch(
        "UPDATE channels
         SET routing_key = CASE
                 WHEN trim(COALESCE(routing_key, '')) <> '' THEN routing_key
                 ELSE channel_type || ':' || agent_id
             END,
             source = CASE
                 WHEN trim(COALESCE(source, '')) <> '' THEN source
                 ELSE 'operator_created'
             END;

         CREATE UNIQUE INDEX IF NOT EXISTS idx_channels_routing_key
             ON channels(routing_key);",
    )
    .map_err(|error| to_storage_err(format!("v066 backfill channels authority: {error}")))?;

    Ok(())
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
