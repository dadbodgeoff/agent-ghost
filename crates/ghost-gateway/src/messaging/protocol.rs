//! Agent message protocol (Req 19 AC1-AC3).

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Agent message struct (AC1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    pub id: Uuid,
    pub sender: Uuid,
    pub recipient: Uuid,
    pub payload: MessagePayload,
    pub context: BTreeMap<String, serde_json::Value>,
    pub nonce: Uuid,
    pub timestamp: DateTime<Utc>,
    pub content_hash: [u8; 32],
    pub signature: Vec<u8>,
    pub encrypted: bool,
}

/// Message payload variants (AC2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessagePayload {
    TaskRequest { task: String, parameters: serde_json::Value },
    TaskResponse { task_id: Uuid, result: serde_json::Value },
    Notification { message: String },
    DelegationOffer { task: String, requirements: serde_json::Value },
    DelegationAccept { offer_id: Uuid },
    DelegationReject { offer_id: Uuid, reason: String },
    DelegationComplete { delegation_id: Uuid, result: serde_json::Value },
    DelegationDispute { delegation_id: Uuid, reason: String },
}

/// Delegation state machine (AC14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationState {
    Offered,
    Accepted,
    Rejected,
    Completed,
    Disputed,
}

impl DelegationState {
    pub fn can_transition_to(self, to: Self) -> bool {
        matches!(
            (self, to),
            (Self::Offered, Self::Accepted)
                | (Self::Offered, Self::Rejected)
                | (Self::Accepted, Self::Completed)
                | (Self::Accepted, Self::Disputed)
        )
    }
}

impl AgentMessage {
    /// Compute canonical bytes for signing (AC3).
    /// Deterministic concatenation in exact field order. BTreeMap for maps.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        buf.extend_from_slice(self.id.as_bytes());
        buf.extend_from_slice(self.sender.as_bytes());
        buf.extend_from_slice(self.recipient.as_bytes());
        buf.extend_from_slice(&serde_json::to_vec(&self.payload).unwrap_or_default());
        // BTreeMap is already sorted by key
        buf.extend_from_slice(&serde_json::to_vec(&self.context).unwrap_or_default());
        buf.extend_from_slice(self.nonce.as_bytes());
        buf.extend_from_slice(self.timestamp.to_rfc3339().as_bytes());
        buf
    }

    /// Compute content hash (blake3) for cheap gate before Ed25519 verify.
    pub fn compute_content_hash(&self) -> [u8; 32] {
        let canonical = self.canonical_bytes();
        *blake3::hash(&canonical).as_bytes()
    }
}
