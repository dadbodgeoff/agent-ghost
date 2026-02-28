//! ClawMesh type definitions.
//!
//! All types are stubs for trait boundary definition.
//! No runtime implementation — Phase 9 deferred.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Status of a settlement between agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettlementStatus {
    /// Settlement is pending processing.
    Pending,
    /// Settlement has been completed successfully.
    Completed,
    /// Settlement failed.
    Failed,
    /// Settlement is under dispute.
    Disputed,
    /// Settlement was cancelled before completion.
    Cancelled,
}

/// A transaction in the mesh payment network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshTransaction {
    /// Unique transaction identifier (UUIDv7, time-ordered).
    pub id: Uuid,
    /// Agent ID of the sender.
    pub from_agent_id: String,
    /// Agent ID of the receiver.
    pub to_agent_id: String,
    /// Amount in the smallest unit of the payment currency.
    pub amount: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// Human-readable description of the transaction purpose.
    pub description: String,
    /// When the transaction was created.
    pub created_at: DateTime<Utc>,
    /// Current settlement status.
    pub status: SettlementStatus,
}

/// An invoice issued by one agent to another for services rendered.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshInvoice {
    /// Unique invoice identifier (UUIDv7, time-ordered).
    pub id: Uuid,
    /// Agent ID of the invoicing party.
    pub issuer_agent_id: String,
    /// Agent ID of the party being invoiced.
    pub payer_agent_id: String,
    /// Requested amount.
    pub amount: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// Line-item description of services.
    pub description: String,
    /// When the invoice was issued.
    pub issued_at: DateTime<Utc>,
    /// When the invoice expires (payment deadline).
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the invoice has been paid.
    pub paid: bool,
}

/// A receipt proving a completed payment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshReceipt {
    /// Unique receipt identifier (UUIDv7, time-ordered).
    pub id: Uuid,
    /// The transaction this receipt covers.
    pub transaction_id: Uuid,
    /// The invoice this receipt settles (if any).
    pub invoice_id: Option<Uuid>,
    /// Amount paid.
    pub amount: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// When the receipt was issued.
    pub issued_at: DateTime<Utc>,
}

/// An agent's wallet in the mesh network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshWallet {
    /// Unique wallet identifier.
    pub id: Uuid,
    /// Agent ID that owns this wallet.
    pub agent_id: String,
    /// Current available balance.
    pub balance: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// When the wallet was created.
    pub created_at: DateTime<Utc>,
}

/// An escrow holding funds during a multi-step transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshEscrow {
    /// Unique escrow identifier.
    pub id: Uuid,
    /// The transaction this escrow secures.
    pub transaction_id: Uuid,
    /// Amount held in escrow.
    pub amount: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// Agent ID of the depositor.
    pub depositor_agent_id: String,
    /// Agent ID of the beneficiary.
    pub beneficiary_agent_id: String,
    /// When the escrow was created.
    pub created_at: DateTime<Utc>,
    /// When the escrow expires (auto-refund deadline).
    pub expires_at: Option<DateTime<Utc>>,
    /// Current settlement status of the escrow.
    pub status: SettlementStatus,
}

/// A settlement record for a completed transaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeshSettlement {
    /// Unique settlement identifier.
    pub id: Uuid,
    /// The transaction being settled.
    pub transaction_id: Uuid,
    /// Final settled amount.
    pub amount: u64,
    /// ISO 4217 currency code or token identifier.
    pub currency: String,
    /// When the settlement was finalized.
    pub settled_at: DateTime<Utc>,
    /// Final status.
    pub status: SettlementStatus,
}
