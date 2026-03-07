//! Cryptographic hash chain for tamper-evident event logs.
//!
//! Each event's hash = blake3(event_type || "|" || delta_json || "|" || actor_id
//!                            || "|" || recorded_at || "|" || previous_hash)
//!
//! Uses blake3 (workspace standard) — NOT sha2.

use thiserror::Error;

/// The zero hash — genesis of every chain.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

#[derive(Debug, Clone, Error, serde::Serialize, serde::Deserialize)]
pub enum ChainError {
    #[error("chain broken at index {index}: expected previous_hash mismatch")]
    BrokenLink { index: usize },
    #[error("chain broken at index {index}: computed hash mismatch")]
    HashMismatch { index: usize },
    #[error("duplicate event_hash at indices {first} and {second}")]
    DuplicateHash { first: usize, second: usize },
}

/// A single event in a hash chain.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainEvent {
    pub event_type: String,
    pub delta_json: String,
    pub actor_id: String,
    pub recorded_at: String,
    pub event_hash: [u8; 32],
    pub previous_hash: [u8; 32],
}

/// Hash a single event, chaining to the previous hash.
pub fn compute_event_hash(
    event_type: &str,
    delta_json: &str,
    actor_id: &str,
    recorded_at: &str,
    previous_hash: &[u8; 32],
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

/// Result of chain verification.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChainVerification {
    pub total_events: usize,
    pub verified_events: usize,
    pub is_valid: bool,
    pub error: Option<ChainError>,
}

/// Verify a sequence of chain events.
pub fn verify_chain(events: &[ChainEvent]) -> ChainVerification {
    if events.is_empty() {
        return ChainVerification {
            total_events: 0,
            verified_events: 0,
            is_valid: true,
            error: None,
        };
    }

    // Check for duplicate hashes
    let mut seen = std::collections::HashSet::new();
    for (i, event) in events.iter().enumerate() {
        if !seen.insert(event.event_hash) {
            // Find the first occurrence
            let first = events
                .iter()
                .position(|e| e.event_hash == event.event_hash)
                .unwrap();
            return ChainVerification {
                total_events: events.len(),
                verified_events: i,
                is_valid: false,
                error: Some(ChainError::DuplicateHash { first, second: i }),
            };
        }
    }

    let mut expected_previous = GENESIS_HASH;

    for (i, event) in events.iter().enumerate() {
        // Verify previous_hash links correctly
        if event.previous_hash != expected_previous {
            return ChainVerification {
                total_events: events.len(),
                verified_events: i,
                is_valid: false,
                error: Some(ChainError::BrokenLink { index: i }),
            };
        }

        // Verify computed hash matches stored hash
        let computed = compute_event_hash(
            &event.event_type,
            &event.delta_json,
            &event.actor_id,
            &event.recorded_at,
            &event.previous_hash,
        );

        if computed != event.event_hash {
            return ChainVerification {
                total_events: events.len(),
                verified_events: i,
                is_valid: false,
                error: Some(ChainError::HashMismatch { index: i }),
            };
        }

        expected_previous = event.event_hash;
    }

    ChainVerification {
        total_events: events.len(),
        verified_events: events.len(),
        is_valid: true,
        error: None,
    }
}

/// Verify all hash chains in the database.
///
/// Queries all distinct memory_ids from `memory_events` that have non-empty
/// event_hash values, then verifies each chain independently.
/// Returns only broken chains (empty vec = all chains valid).
///
/// Requires the `sqlite` feature.
#[cfg(feature = "sqlite")]
pub fn verify_all_chains(
    conn: &rusqlite::Connection,
) -> Result<Vec<ChainVerification>, cortex_core::models::error::CortexError> {
    use cortex_core::models::error::CortexError;

    let mut stmt = conn
        .prepare("SELECT DISTINCT memory_id FROM memory_events WHERE event_hash != x''")
        .map_err(|e| CortexError::Storage(e.to_string()))?;

    let memory_ids: Vec<String> = stmt
        .query_map([], |row| row.get(0))
        .map_err(|e| CortexError::Storage(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CortexError::Storage(e.to_string()))?;

    let mut broken = Vec::new();

    for memory_id in &memory_ids {
        let events = load_chain_events(conn, memory_id)?;
        let result = verify_chain(&events);
        if !result.is_valid {
            broken.push(result);
        }
    }

    Ok(broken)
}

/// Load chain events for a specific memory_id from the database.
#[cfg(feature = "sqlite")]
fn load_chain_events(
    conn: &rusqlite::Connection,
    memory_id: &str,
) -> Result<Vec<ChainEvent>, cortex_core::models::error::CortexError> {
    use cortex_core::models::error::CortexError;

    let mut stmt = conn
        .prepare(
            "SELECT event_type, delta, actor_id, recorded_at, event_hash, previous_hash
             FROM memory_events
             WHERE memory_id = ?1 AND event_hash != x''
             ORDER BY event_id ASC",
        )
        .map_err(|e| CortexError::Storage(e.to_string()))?;

    let events = stmt
        .query_map(rusqlite::params![memory_id], |row| {
            let event_hash_vec: Vec<u8> = row.get(4)?;
            let previous_hash_vec: Vec<u8> = row.get(5)?;

            let mut event_hash = [0u8; 32];
            let mut previous_hash = [0u8; 32];
            if event_hash_vec.len() == 32 {
                event_hash.copy_from_slice(&event_hash_vec);
            }
            if previous_hash_vec.len() == 32 {
                previous_hash.copy_from_slice(&previous_hash_vec);
            }

            Ok(ChainEvent {
                event_type: row.get(0)?,
                delta_json: row.get(1)?,
                actor_id: row.get(2)?,
                recorded_at: row.get(3)?,
                event_hash,
                previous_hash,
            })
        })
        .map_err(|e| CortexError::Storage(e.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| CortexError::Storage(e.to_string()))?;

    Ok(events)
}
