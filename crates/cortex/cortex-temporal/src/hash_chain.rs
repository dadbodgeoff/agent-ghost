//! Cryptographic hash chain for tamper-evident event logs.
//!
//! Each event's hash = blake3(event_type || "|" || delta_json || "|" || actor_id
//!                            || "|" || recorded_at || "|" || previous_hash)
//!
//! Uses blake3 (workspace standard) — NOT sha2.

use thiserror::Error;

/// The zero hash — genesis of every chain.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

#[derive(Debug, Clone, Error)]
pub enum ChainError {
    #[error("chain broken at index {index}: expected previous_hash mismatch")]
    BrokenLink { index: usize },
    #[error("chain broken at index {index}: computed hash mismatch")]
    HashMismatch { index: usize },
    #[error("duplicate event_hash at indices {first} and {second}")]
    DuplicateHash { first: usize, second: usize },
}

/// A single event in a hash chain.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
            let first = events.iter().position(|e| e.event_hash == event.event_hash).unwrap();
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
