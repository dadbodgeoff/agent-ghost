//! Migration v035: Convergence links table for parent-child agent delegation (Phase 10).
//!
//! Tracks convergence inheritance relationships between delegating (parent)
//! and delegated (child) agents. Used for convergence propagation:
//! - Child inherits parent's convergence score (can only increase)
//! - Parent adjusted when child score rises: max(parent, child * 0.5)
//! - Boundary violation in child → quarantine + parent penalty

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::Connection;

pub fn migrate(conn: &Connection) -> CortexResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS convergence_links (
            id              TEXT PRIMARY KEY,
            parent_agent_id TEXT NOT NULL,
            child_agent_id  TEXT NOT NULL,
            delegation_id   TEXT NOT NULL,
            inherited_score REAL NOT NULL,
            inherited_level INTEGER NOT NULL,
            status          TEXT NOT NULL DEFAULT 'active',
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            updated_at      TEXT NOT NULL DEFAULT (datetime('now')),
            UNIQUE(parent_agent_id, child_agent_id, delegation_id)
        );
        CREATE INDEX IF NOT EXISTS idx_convergence_links_parent
            ON convergence_links(parent_agent_id);
        CREATE INDEX IF NOT EXISTS idx_convergence_links_child
            ON convergence_links(child_agent_id);
        CREATE INDEX IF NOT EXISTS idx_convergence_links_delegation
            ON convergence_links(delegation_id);
        CREATE INDEX IF NOT EXISTS idx_convergence_links_active
            ON convergence_links(status) WHERE status = 'active';",
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}
