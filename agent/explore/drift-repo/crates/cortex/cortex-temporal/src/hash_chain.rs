//! Cryptographic hash chain for tamper-evident event logs.
//!
//! Each event's hash = blake3(event_type || "|" || delta || "|" || actor_id || "|" || recorded_at || "|" || previous_hash)
//! Chains are per-memory_id (not global) to avoid serialization bottleneck.
//!
//! Uses blake3 (workspace standard) — NOT sha2.

use rusqlite::Connection;

use cortex_core::errors::{CortexResult, StorageError};
use cortex_core::CortexError;

/// Hash a single event, chaining to the previous hash.
pub fn compute_event_hash(
    event_type: &str,
    delta_json: &str,
    actor_id: &str,
    recorded_at: &str,
    previous_hash: &[u8],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(event_type.as_bytes());
    hasher.update(b"|");
    hasher.update(delta_json.as_bytes());
    hasher.update(b"|");
    hasher.update(actor_id.as_bytes());
    hasher.update(b"|");
    hasher.update(recorded_at.as_bytes());
    hasher.update(b"|");
    hasher.update(previous_hash);
    *hasher.finalize().as_bytes()
}

/// The zero hash — genesis of every per-memory chain.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

/// Result of chain verification.
#[derive(Debug)]
pub struct ChainVerification {
    pub memory_id: String,
    pub total_events: usize,
    pub verified_events: usize,
    pub is_valid: bool,
    pub broken_at_index: Option<usize>,
    pub broken_at_event_id: Option<i64>,
}

/// Verify the hash chain for a single memory_id.
pub fn verify_chain(conn: &Connection, memory_id: &str) -> CortexResult<ChainVerification> {
    let mut stmt = conn
        .prepare(
            "SELECT event_id, event_type, delta, actor_id, recorded_at, event_hash, previous_hash
             FROM memory_events
             WHERE memory_id = ?1
             ORDER BY recorded_at ASC, event_id ASC",
        )
        .map_err(|e| storage_err(e.to_string()))?;

    let events: Vec<EventRow> = stmt
        .query_map([memory_id], |row| {
            Ok(EventRow {
                event_id: row.get(0)?,
                event_type: row.get(1)?,
                delta: row.get(2)?,
                actor_id: row.get(3)?,
                recorded_at: row.get(4)?,
                event_hash: row.get(5)?,
                previous_hash: row.get(6)?,
            })
        })
        .map_err(|e| storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| storage_err(e.to_string()))?;

    let total = events.len();
    let mut expected_previous = GENESIS_HASH.to_vec();

    for (i, event) in events.iter().enumerate() {
        if event.previous_hash != expected_previous {
            return Ok(ChainVerification {
                memory_id: memory_id.to_string(),
                total_events: total,
                verified_events: i,
                is_valid: false,
                broken_at_index: Some(i),
                broken_at_event_id: Some(event.event_id),
            });
        }

        let computed = compute_event_hash(
            &event.event_type,
            &event.delta,
            &event.actor_id,
            &event.recorded_at,
            &event.previous_hash,
        );

        if computed.as_slice() != event.event_hash.as_slice() {
            return Ok(ChainVerification {
                memory_id: memory_id.to_string(),
                total_events: total,
                verified_events: i,
                is_valid: false,
                broken_at_index: Some(i),
                broken_at_event_id: Some(event.event_id),
            });
        }

        expected_previous = event.event_hash.clone();
    }

    Ok(ChainVerification {
        memory_id: memory_id.to_string(),
        total_events: total,
        verified_events: total,
        is_valid: true,
        broken_at_index: None,
        broken_at_event_id: None,
    })
}

/// Verify all chains in the database. Returns broken chains only.
pub fn verify_all_chains(conn: &Connection) -> CortexResult<Vec<ChainVerification>> {
    let memory_ids: Vec<String> = conn
        .prepare("SELECT DISTINCT memory_id FROM memory_events WHERE event_hash != x''")
        .map_err(|e| storage_err(e.to_string()))?
        .query_map([], |row| row.get(0))
        .map_err(|e| storage_err(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| storage_err(e.to_string()))?;

    let mut broken = Vec::new();
    for memory_id in &memory_ids {
        let result = verify_chain(conn, memory_id)?;
        if !result.is_valid {
            broken.push(result);
        }
    }
    Ok(broken)
}

#[derive(Debug)]
struct EventRow {
    event_id: i64,
    event_type: String,
    delta: String,
    actor_id: String,
    recorded_at: String,
    event_hash: Vec<u8>,
    previous_hash: Vec<u8>,
}

fn storage_err(msg: String) -> CortexError {
    CortexError::StorageError(StorageError::SqliteError { message: msg })
}
