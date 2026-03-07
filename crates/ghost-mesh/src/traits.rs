//! ClawMesh trait definitions.
//!
//! These traits define the interface boundary for the mesh payment system.
//! No implementations are provided — Phase 9 deferred.

use crate::types::{MeshEscrow, MeshInvoice, MeshReceipt, MeshTransaction, MeshWallet};
use uuid::Uuid;

/// Errors that can occur during mesh operations.
#[derive(Debug, thiserror::Error)]
pub enum MeshError {
    /// The requested operation is not yet implemented (Phase 9 deferred).
    #[error("mesh operation not implemented: {0}")]
    NotImplemented(String),

    /// Insufficient funds for the requested operation.
    #[error("insufficient funds: required {required}, available {available}")]
    InsufficientFunds { required: u64, available: u64 },

    /// The referenced entity was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The operation was rejected due to a policy violation.
    #[error("policy violation: {0}")]
    PolicyViolation(String),

    /// The escrow has expired.
    #[error("escrow expired: {0}")]
    EscrowExpired(Uuid),

    /// A dispute is already in progress for this transaction.
    #[error("dispute already active for transaction {0}")]
    DisputeAlreadyActive(Uuid),
}

/// Provider for mesh payment operations between agents.
///
/// Implementations handle invoice creation, payment execution,
/// balance queries, and escrow management.
pub trait IMeshProvider: Send + Sync {
    /// Create an invoice from one agent to another.
    fn create_invoice(
        &self,
        issuer_agent_id: &str,
        payer_agent_id: &str,
        amount: u64,
        currency: &str,
        description: &str,
    ) -> Result<MeshInvoice, MeshError>;

    /// Pay an outstanding invoice, returning a receipt.
    fn pay_invoice(&self, invoice_id: Uuid) -> Result<MeshReceipt, MeshError>;

    /// Check the balance of an agent's wallet.
    fn check_balance(&self, agent_id: &str, currency: &str) -> Result<MeshWallet, MeshError>;

    /// Place funds in escrow for a transaction.
    fn escrow(
        &self,
        transaction_id: Uuid,
        depositor_agent_id: &str,
        beneficiary_agent_id: &str,
        amount: u64,
        currency: &str,
    ) -> Result<MeshEscrow, MeshError>;

    /// Release escrowed funds to the beneficiary.
    fn release_escrow(&self, escrow_id: Uuid) -> Result<MeshReceipt, MeshError>;

    /// Initiate a dispute on a transaction.
    fn dispute(&self, transaction_id: Uuid, reason: &str) -> Result<(), MeshError>;
}

/// Ledger for recording and querying mesh transactions.
///
/// Provides an append-only transaction log with verification capabilities.
pub trait IMeshLedger: Send + Sync {
    /// Append a transaction to the ledger.
    fn append_transaction(&self, transaction: &MeshTransaction) -> Result<(), MeshError>;

    /// Query transactions for a given agent (as sender or receiver).
    fn query_transactions(&self, agent_id: &str) -> Result<Vec<MeshTransaction>, MeshError>;

    /// Verify a receipt against the ledger.
    fn verify_receipt(&self, receipt: &MeshReceipt) -> Result<bool, MeshError>;
}
