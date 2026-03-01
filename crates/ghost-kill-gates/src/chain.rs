//! Gate event hash chain — blake3-based tamper-evident log.
//!
//! Each gate event is chained: hash = blake3(event_type || node_id ||
//! timestamp || payload_json || previous_hash).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Genesis hash — zero bytes, same convention as cortex-temporal.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

/// A single gate event in the hash chain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateChainEvent {
    pub event_type: GateEventType,
    pub node_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub payload_json: String,
    pub event_hash: [u8; 32],
    pub previous_hash: [u8; 32],
}

/// Types of gate events recorded in the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GateEventType {
    /// Gate closed (kill activated).
    Close,
    /// Propagation sent to peer.
    Propagate,
    /// Ack received from peer.
    Ack,
    /// Resume vote cast.
    ResumeVote,
    /// Quorum reached, gate reopened.
    ResumeConfirmed,
    /// Node detected as partitioned.
    PartitionDetected,
    /// Node rejoined after partition.
    Rejoin,
}

/// Compute the hash for a gate event.
pub fn compute_gate_event_hash(
    event_type: GateEventType,
    node_id: &Uuid,
    timestamp: &DateTime<Utc>,
    payload_json: &str,
    previous_hash: &[u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(format!("{:?}", event_type).as_bytes());
    hasher.update(b"|");
    hasher.update(node_id.as_bytes());
    hasher.update(b"|");
    hasher.update(timestamp.to_rfc3339().as_bytes());
    hasher.update(b"|");
    hasher.update(payload_json.as_bytes());
    hasher.update(b"|");
    hasher.update(previous_hash);
    *hasher.finalize().as_bytes()
}
