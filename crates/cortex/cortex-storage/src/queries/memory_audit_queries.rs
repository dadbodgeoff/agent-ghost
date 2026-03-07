//! Memory audit log queries (v016 memory_audit_log table).

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

pub fn insert_audit(
    conn: &Connection,
    memory_id: &str,
    operation: &str,
    details: Option<&str>,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO memory_audit_log (memory_id, operation, details)
         VALUES (?1, ?2, ?3)",
        params![memory_id, operation, details],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn query_by_memory(conn: &Connection, memory_id: &str) -> CortexResult<Vec<AuditRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, memory_id, operation, timestamp, details
             FROM memory_audit_log WHERE memory_id = ?1
             ORDER BY timestamp DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![memory_id], |row| {
            Ok(AuditRow {
                id: row.get(0)?,
                memory_id: row.get(1)?,
                operation: row.get(2)?,
                timestamp: row.get(3)?,
                details: row.get(4)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct AuditRow {
    pub id: i64,
    pub memory_id: String,
    pub operation: String,
    pub timestamp: String,
    pub details: Option<String>,
}
