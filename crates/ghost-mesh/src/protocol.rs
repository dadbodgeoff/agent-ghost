//! ClawMesh protocol message definitions.
//!
//! Defines the message format for agent-to-agent payment negotiation.
//! All implementations return `MeshError::NotImplemented` — Phase 9 deferred.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::traits::MeshError;

/// A protocol message exchanged between agents during payment negotiation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MeshMessage {
    /// Request payment from another agent.
    Request {
        /// Unique message identifier.
        id: Uuid,
        /// Agent requesting payment.
        from_agent_id: String,
        /// Agent being asked to pay.
        to_agent_id: String,
        /// Requested amount.
        amount: u64,
        /// Currency code.
        currency: String,
        /// Description of the service or goods.
        description: String,
        /// When the request was created.
        created_at: DateTime<Utc>,
    },
    /// Accept a payment request.
    Accept {
        /// Unique message identifier.
        id: Uuid,
        /// The request being accepted.
        request_id: Uuid,
        /// Agent accepting the request.
        from_agent_id: String,
        /// When the acceptance was issued.
        created_at: DateTime<Utc>,
    },
    /// Reject a payment request.
    Reject {
        /// Unique message identifier.
        id: Uuid,
        /// The request being rejected.
        request_id: Uuid,
        /// Agent rejecting the request.
        from_agent_id: String,
        /// Reason for rejection.
        reason: String,
        /// When the rejection was issued.
        created_at: DateTime<Utc>,
    },
    /// Signal that a payment has been completed.
    Complete {
        /// Unique message identifier.
        id: Uuid,
        /// The request that was fulfilled.
        request_id: Uuid,
        /// The transaction that settled the payment.
        transaction_id: Uuid,
        /// When the completion was recorded.
        created_at: DateTime<Utc>,
    },
    /// Initiate a dispute on a completed payment.
    Dispute {
        /// Unique message identifier.
        id: Uuid,
        /// The transaction being disputed.
        transaction_id: Uuid,
        /// Agent initiating the dispute.
        from_agent_id: String,
        /// Reason for the dispute.
        reason: String,
        /// When the dispute was filed.
        created_at: DateTime<Utc>,
    },
}

/// Stub protocol handler for mesh message processing.
///
/// All methods return `MeshError::NotImplemented`.
pub struct MeshProtocol;

impl MeshProtocol {
    /// Process an incoming mesh message.
    ///
    /// # Errors
    /// Always returns `MeshError::NotImplemented` — Phase 9 deferred.
    pub fn process_message(&self, _message: &MeshMessage) -> Result<(), MeshError> {
        Err(MeshError::NotImplemented(
            "MeshProtocol::process_message".into(),
        ))
    }

    /// Send a mesh message to another agent.
    ///
    /// # Errors
    /// Always returns `MeshError::NotImplemented` — Phase 9 deferred.
    pub fn send_message(&self, _message: &MeshMessage) -> Result<(), MeshError> {
        Err(MeshError::NotImplemented(
            "MeshProtocol::send_message".into(),
        ))
    }
}
