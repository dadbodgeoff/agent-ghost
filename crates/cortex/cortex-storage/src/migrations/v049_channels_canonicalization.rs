//! Migration v049: canonicalize channels table ownership under migrations.

use std::collections::BTreeSet;

use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

use crate::to_storage_err;

const CHANNEL_COLUMNS: &[&str] = &[
    "id",
    "channel_type",
    "status",
    "status_message",
    "agent_id",
    "config",
    "last_message_at",
    "message_count",
    "created_at",
    "updated_at",
];

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    if has_table(conn, "channels")? {
        let columns = table_columns(conn, "channels")?;
        let missing = CHANNEL_COLUMNS
            .iter()
            .filter(|column| !columns.contains(**column))
            .copied()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            return Err(to_storage_err(format!(
                "unsupported legacy channels shape: missing required columns [{}]",
                missing.join(", ")
            )));
        }
    }

    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS channels (
            id TEXT PRIMARY KEY,
            channel_type TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'configuring',
            status_message TEXT,
            agent_id TEXT NOT NULL,
            config TEXT NOT NULL DEFAULT '{}',
            last_message_at TEXT,
            message_count INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE INDEX IF NOT EXISTS idx_channels_agent ON channels(agent_id);
        CREATE INDEX IF NOT EXISTS idx_channels_type ON channels(channel_type);",
    )
    .map_err(|error| to_storage_err(format!("v049 canonicalize channels: {error}")))?;

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
