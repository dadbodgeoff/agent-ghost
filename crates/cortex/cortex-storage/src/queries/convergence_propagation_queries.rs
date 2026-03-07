//! Convergence propagation queries (Phase 10).
//!
//! Manages parent-child convergence links for delegation.
//! Used by delegation skills to track inheritance relationships
//! and by the convergence watcher for upward score propagation.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

/// Insert a parent-child convergence link for a delegation.
pub fn link_parent_child(
    conn: &Connection,
    id: &str,
    parent_agent_id: &str,
    child_agent_id: &str,
    delegation_id: &str,
    inherited_score: f64,
    inherited_level: i32,
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO convergence_links (id, parent_agent_id, child_agent_id,
         delegation_id, inherited_score, inherited_level)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            id,
            parent_agent_id,
            child_agent_id,
            delegation_id,
            inherited_score,
            inherited_level
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Get all active children of a parent agent.
pub fn get_children(
    conn: &Connection,
    parent_agent_id: &str,
) -> CortexResult<Vec<ConvergenceLinkRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_agent_id, child_agent_id, delegation_id,
                    inherited_score, inherited_level, status, created_at
             FROM convergence_links
             WHERE parent_agent_id = ?1 AND status = 'active'
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![parent_agent_id], |row| {
            Ok(ConvergenceLinkRow {
                id: row.get(0)?,
                parent_agent_id: row.get(1)?,
                child_agent_id: row.get(2)?,
                delegation_id: row.get(3)?,
                inherited_score: row.get(4)?,
                inherited_level: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Get the parent link for a child agent (if any).
pub fn get_parent(
    conn: &Connection,
    child_agent_id: &str,
) -> CortexResult<Option<ConvergenceLinkRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, parent_agent_id, child_agent_id, delegation_id,
                    inherited_score, inherited_level, status, created_at
             FROM convergence_links
             WHERE child_agent_id = ?1 AND status = 'active'
             ORDER BY created_at DESC LIMIT 1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![child_agent_id], |row| {
            Ok(ConvergenceLinkRow {
                id: row.get(0)?,
                parent_agent_id: row.get(1)?,
                child_agent_id: row.get(2)?,
                delegation_id: row.get(3)?,
                inherited_score: row.get(4)?,
                inherited_level: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows.pop())
}

/// Mark a child as quarantined (e.g., after a boundary violation).
pub fn quarantine_child(
    conn: &Connection,
    child_agent_id: &str,
    reason: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE convergence_links SET status = 'quarantined', updated_at = datetime('now')
             WHERE child_agent_id = ?1 AND status = 'active'",
            params![child_agent_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    if updated > 0 {
        tracing::warn!(
            child_agent_id,
            reason,
            "Child agent quarantined in convergence link"
        );
    }
    Ok(updated > 0)
}

/// Mark a convergence link as completed (delegation finished normally).
pub fn complete_link(conn: &Connection, delegation_id: &str) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE convergence_links SET status = 'completed', updated_at = datetime('now')
             WHERE delegation_id = ?1 AND status = 'active'",
            params![delegation_id],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

#[derive(Debug, Clone)]
pub struct ConvergenceLinkRow {
    pub id: String,
    pub parent_agent_id: String,
    pub child_agent_id: String,
    pub delegation_id: String,
    pub inherited_score: f64,
    pub inherited_level: i32,
    pub status: String,
    pub created_at: String,
}
