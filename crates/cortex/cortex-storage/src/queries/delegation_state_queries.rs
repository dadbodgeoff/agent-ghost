//! Delegation state queries (v018 delegation_state table).

use rusqlite::{params, Connection};
use cortex_core::models::error::CortexResult;
use crate::to_storage_err;

pub fn insert_delegation(
    conn: &Connection,
    id: &str,
    delegation_id: &str,
    sender_id: &str,
    recipient_id: &str,
    task: &str,
    offer_message_id: &str,
    event_hash: &[u8],
    previous_hash: &[u8],
) -> CortexResult<()> {
    conn.execute(
        "INSERT INTO delegation_state (id, delegation_id, sender_id, recipient_id,
         task, state, offer_message_id, event_hash, previous_hash)
         VALUES (?1, ?2, ?3, ?4, ?5, 'Offered', ?6, ?7, ?8)",
        params![
            id, delegation_id, sender_id, recipient_id, task,
            offer_message_id, event_hash, previous_hash,
        ],
    )
    .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(())
}

pub fn transition(
    conn: &Connection,
    id: &str,
    new_state: &str,
    accept_message_id: Option<&str>,
    complete_message_id: Option<&str>,
    result: Option<&str>,
    dispute_reason: Option<&str>,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE delegation_state SET state = ?2, accept_message_id = COALESCE(?3, accept_message_id),
             complete_message_id = COALESCE(?4, complete_message_id),
             result = COALESCE(?5, result), dispute_reason = COALESCE(?6, dispute_reason),
             updated_at = datetime('now')
             WHERE id = ?1",
            params![id, new_state, accept_message_id, complete_message_id, result, dispute_reason],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

/// Transition by delegation_id (used when the caller only has the external task ID).
pub fn transition_by_delegation_id(
    conn: &Connection,
    delegation_id: &str,
    new_state: &str,
    accept_message_id: Option<&str>,
    complete_message_id: Option<&str>,
    result: Option<&str>,
    dispute_reason: Option<&str>,
) -> CortexResult<bool> {
    let updated = conn
        .execute(
            "UPDATE delegation_state SET state = ?2, accept_message_id = COALESCE(?3, accept_message_id),
             complete_message_id = COALESCE(?4, complete_message_id),
             result = COALESCE(?5, result), dispute_reason = COALESCE(?6, dispute_reason),
             updated_at = datetime('now')
             WHERE delegation_id = ?1",
            params![delegation_id, new_state, accept_message_id, complete_message_id, result, dispute_reason],
        )
        .map_err(|e| to_storage_err(e.to_string()))?;
    Ok(updated > 0)
}

pub fn query_pending(conn: &Connection) -> CortexResult<Vec<DelegationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, delegation_id, sender_id, recipient_id, task, state, created_at
             FROM delegation_state WHERE state IN ('Offered', 'Accepted')
             ORDER BY created_at DESC",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let rows = stmt
        .query_map([], |row| {
            Ok(DelegationRow {
                id: row.get(0)?,
                delegation_id: row.get(1)?,
                sender_id: row.get(2)?,
                recipient_id: row.get(3)?,
                task: row.get(4)?,
                state: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows)
}

/// Query a single delegation by its delegation_id.
pub fn query_by_delegation_id(
    conn: &Connection,
    delegation_id: &str,
) -> CortexResult<Option<DelegationRow>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, delegation_id, sender_id, recipient_id, task, state, created_at
             FROM delegation_state WHERE delegation_id = ?1
             LIMIT 1",
        )
        .map_err(|e| to_storage_err(e.to_string()))?;

    let mut rows = stmt
        .query_map(params![delegation_id], |row| {
            Ok(DelegationRow {
                id: row.get(0)?,
                delegation_id: row.get(1)?,
                sender_id: row.get(2)?,
                recipient_id: row.get(3)?,
                task: row.get(4)?,
                state: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|e| to_storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| to_storage_err(e.to_string()))?;

    Ok(rows.pop())
}

/// Get the most recent event_hash for a sender (for hash chain continuity).
pub fn query_last_hash(conn: &Connection, sender_id: &str) -> CortexResult<Option<Vec<u8>>> {
    let result: Result<Vec<u8>, _> = conn.query_row(
        "SELECT event_hash FROM delegation_state
         WHERE sender_id = ?1
         ORDER BY created_at DESC LIMIT 1",
        params![sender_id],
        |row| row.get(0),
    );
    match result {
        Ok(hash) => Ok(Some(hash)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(to_storage_err(e.to_string())),
    }
}

#[derive(Debug, Clone)]
pub struct DelegationRow {
    pub id: String,
    pub delegation_id: String,
    pub sender_id: String,
    pub recipient_id: String,
    pub task: String,
    pub state: String,
    pub created_at: String,
}
