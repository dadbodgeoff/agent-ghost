//! Goal proposal queries.

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn insert_proposal(
    conn: &Connection,
    id: &str,
    agent_id: &str,
    session_id: &str,
    proposer_type: &str,
    operation: &str,
    target_type: &str,
    content: &str,
    cited_memory_ids: &str,
    decision: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO goal_proposals (id, agent_id, session_id, proposer_type,
         operation, target_type, content, cited_memory_ids, decision,
         event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            id, agent_id, session_id, proposer_type, operation, target_type,
            content, cited_memory_ids, decision, event_hash, previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

/// Resolve an unresolved proposal. Only succeeds if resolved_at IS NULL (AC10).
pub fn resolve_proposal(
    conn: &Connection,
    id: &str,
    decision: &str,
    resolver: &str,
    resolved_at: &str,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE goal_proposals SET decision = ?2, resolver = ?3, resolved_at = ?4
             WHERE id = ?1 AND resolved_at IS NULL",
            params![id, decision, resolver, resolved_at],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn query_pending(conn: &Connection) -> CortexResult<Vec<ProposalRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type,
                    decision, resolved_at, created_at
             FROM goal_proposals WHERE resolved_at IS NULL
             ORDER BY created_at ASC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(ProposalRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                proposer_type: row.get(3)?,
                operation: row.get(4)?,
                target_type: row.get(5)?,
                decision: row.get(6)?,
                resolved_at: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

pub fn query_by_agent(conn: &Connection, agent_id: &str) -> CortexResult<Vec<ProposalRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, agent_id, session_id, proposer_type, operation, target_type,
                    decision, resolved_at, created_at
             FROM goal_proposals WHERE agent_id = ?1
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map(params![agent_id], |row| {
            Ok(ProposalRow {
                id: row.get(0)?,
                agent_id: row.get(1)?,
                session_id: row.get(2)?,
                proposer_type: row.get(3)?,
                operation: row.get(4)?,
                target_type: row.get(5)?,
                decision: row.get(6)?,
                resolved_at: row.get(7)?,
                created_at: row.get(8)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

#[derive(Debug, Clone)]
pub struct ProposalRow {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub proposer_type: String,
    pub operation: String,
    pub target_type: String,
    pub decision: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}
