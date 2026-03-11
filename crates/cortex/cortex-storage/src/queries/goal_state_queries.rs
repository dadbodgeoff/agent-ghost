//! Active durable goal-state queries.

use crate::to_storage_err;
use cortex_core::models::error::CortexResult;
use rusqlite::{params, Connection};

#[derive(Debug, Clone)]
pub struct ActiveGoalRow {
    pub goal_id: String,
    pub source_proposal_id: String,
    pub agent_id: String,
    pub session_id: String,
    pub lineage_id: String,
    pub subject_type: String,
    pub subject_key: String,
    pub reviewed_revision: String,
    pub state: String,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
}

pub fn count_active_goals(conn: &Connection, agent_id: Option<&str>) -> CortexResult<u32> {
    let sql = if agent_id.is_some() {
        "SELECT COUNT(*)
         FROM goal_lineage_heads glh
         JOIN goal_proposals_v2 gpv2 ON gpv2.id = glh.head_proposal_id
         JOIN goal_proposals gp ON gp.id = glh.head_proposal_id
         WHERE gp.agent_id = ?1
           AND glh.head_state IN ('approved', 'auto_applied')"
    } else {
        "SELECT COUNT(*)
         FROM goal_lineage_heads glh
         WHERE glh.head_state IN ('approved', 'auto_applied')"
    };

    let count = if let Some(agent_id) = agent_id {
        conn.query_row(sql, params![agent_id], |row| row.get(0))
    } else {
        conn.query_row(sql, [], |row| row.get(0))
    }
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(count)
}

pub fn list_active_goals(
    conn: &Connection,
    agent_id: Option<&str>,
    limit: u32,
    offset: u32,
) -> CortexResult<Vec<ActiveGoalRow>> {
    let sql = if agent_id.is_some() {
        "SELECT
             glh.lineage_id,
             glh.head_proposal_id,
             gp.agent_id,
             gp.session_id,
             gpv2.subject_type,
             gpv2.subject_key,
             gpv2.reviewed_revision,
             glh.head_state,
             gpv2.content,
             gp.created_at,
             glh.updated_at
         FROM goal_lineage_heads glh
         JOIN goal_proposals_v2 gpv2 ON gpv2.id = glh.head_proposal_id
         JOIN goal_proposals gp ON gp.id = glh.head_proposal_id
         WHERE gp.agent_id = ?1
           AND glh.head_state IN ('approved', 'auto_applied')
         ORDER BY glh.updated_at DESC
         LIMIT ?2 OFFSET ?3"
    } else {
        "SELECT
             glh.lineage_id,
             glh.head_proposal_id,
             gp.agent_id,
             gp.session_id,
             gpv2.subject_type,
             gpv2.subject_key,
             gpv2.reviewed_revision,
             glh.head_state,
             gpv2.content,
             gp.created_at,
             glh.updated_at
         FROM goal_lineage_heads glh
         JOIN goal_proposals_v2 gpv2 ON gpv2.id = glh.head_proposal_id
         JOIN goal_proposals gp ON gp.id = glh.head_proposal_id
         WHERE glh.head_state IN ('approved', 'auto_applied')
         ORDER BY glh.updated_at DESC
         LIMIT ?1 OFFSET ?2"
    };

    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = if let Some(agent_id) = agent_id {
        stmt.query_map(params![agent_id, limit, offset], map_active_goal_row)
    } else {
        stmt.query_map(params![limit, offset], map_active_goal_row)
    }
    .map_err(|e| to_storage_err(e.to_string()))?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

fn map_active_goal_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ActiveGoalRow> {
    let lineage_id: String = row.get(0)?;
    Ok(ActiveGoalRow {
        goal_id: lineage_id.clone(),
        source_proposal_id: row.get(1)?,
        agent_id: row.get(2)?,
        session_id: row.get(3)?,
        lineage_id,
        subject_type: row.get(4)?,
        subject_key: row.get(5)?,
        reviewed_revision: row.get(6)?,
        state: row.get(7)?,
        content: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}
