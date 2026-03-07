//! Credit ledger implementing IMeshProvider + IMeshLedger traits.
//!
//! Alpha: SQLite-backed internal credits. No real money.

use std::sync::{Arc, Mutex};

use ghost_mesh::traits::{IMeshLedger, IMeshProvider, MeshError};
use ghost_mesh::types::{MeshEscrow, MeshInvoice, MeshReceipt, MeshTransaction, MeshWallet};
use rusqlite::Connection;
use uuid::Uuid;

/// Credit system backed by SQLite marketplace tables.
pub struct CreditLedger {
    db: Arc<Mutex<Connection>>,
}

impl CreditLedger {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Seed an agent's wallet with credits (alpha only).
    pub fn seed(&self, agent_id: &str, amount: i64) -> Result<(), MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        cortex_storage::queries::marketplace_queries::seed_wallet(&conn, agent_id, amount)
            .map_err(|e| MeshError::NotImplemented(e.to_string()))
    }

    /// Get wallet balance.
    pub fn balance(&self, agent_id: &str) -> Result<(i64, i64), MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        let wallet = cortex_storage::queries::marketplace_queries::get_wallet(&conn, agent_id)
            .map_err(|e| MeshError::NotImplemented(e.to_string()))?
            .ok_or_else(|| MeshError::NotFound(format!("wallet for {agent_id}")))?;
        Ok((wallet.balance, wallet.escrowed))
    }
}

impl IMeshProvider for CreditLedger {
    fn create_invoice(
        &self,
        _issuer_agent_id: &str,
        _payer_agent_id: &str,
        _amount: u64,
        _currency: &str,
        _description: &str,
    ) -> Result<MeshInvoice, MeshError> {
        Err(MeshError::NotImplemented(
            "invoices not needed for alpha credit system".into(),
        ))
    }

    fn pay_invoice(&self, _invoice_id: Uuid) -> Result<MeshReceipt, MeshError> {
        Err(MeshError::NotImplemented(
            "invoices not needed for alpha credit system".into(),
        ))
    }

    fn check_balance(&self, agent_id: &str, _currency: &str) -> Result<MeshWallet, MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        let wallet = cortex_storage::queries::marketplace_queries::get_wallet(&conn, agent_id)
            .map_err(|e| MeshError::NotImplemented(e.to_string()))?
            .ok_or_else(|| MeshError::NotFound(format!("wallet for {agent_id}")))?;

        Ok(MeshWallet {
            agent_id: agent_id.to_string(),
            currency: "credits".to_string(),
            balance: wallet.balance as u64,
        })
    }

    fn escrow(
        &self,
        _transaction_id: Uuid,
        depositor_agent_id: &str,
        beneficiary_agent_id: &str,
        amount: u64,
        _currency: &str,
    ) -> Result<MeshEscrow, MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        let escrow_id = Uuid::new_v4();

        cortex_storage::queries::marketplace_queries::create_escrow(
            &conn,
            &escrow_id.to_string(),
            &escrow_id.to_string(), // self-referencing for standalone escrow
            depositor_agent_id,
            beneficiary_agent_id,
            amount as i64,
        )
        .map_err(|e| {
            if e.to_string().contains("insufficient") {
                MeshError::InsufficientFunds {
                    required: amount,
                    available: 0,
                }
            } else {
                MeshError::NotImplemented(e.to_string())
            }
        })?;

        Ok(MeshEscrow {
            id: escrow_id,
            transaction_id: _transaction_id,
            depositor: depositor_agent_id.to_string(),
            beneficiary: beneficiary_agent_id.to_string(),
            amount,
            currency: "credits".to_string(),
            created_at: chrono::Utc::now(),
        })
    }

    fn release_escrow(&self, escrow_id: Uuid) -> Result<MeshReceipt, MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        cortex_storage::queries::marketplace_queries::release_escrow(
            &conn,
            &escrow_id.to_string(),
        )
        .map_err(|e| MeshError::NotImplemented(e.to_string()))?;

        Ok(MeshReceipt {
            id: Uuid::new_v4(),
            transaction_id: escrow_id,
            amount: 0,
            currency: "credits".to_string(),
            issued_at: chrono::Utc::now(),
        })
    }

    fn dispute(&self, _transaction_id: Uuid, _reason: &str) -> Result<(), MeshError> {
        // Alpha: disputes auto-refund via contract FSM
        Ok(())
    }
}

impl IMeshLedger for CreditLedger {
    fn append_transaction(&self, _transaction: &MeshTransaction) -> Result<(), MeshError> {
        // Transactions are appended automatically by escrow/release/seed operations
        Ok(())
    }

    fn query_transactions(&self, agent_id: &str) -> Result<Vec<MeshTransaction>, MeshError> {
        let conn = self
            .db
            .lock()
            .map_err(|_| MeshError::NotImplemented("db lock poisoned".into()))?;
        let rows =
            cortex_storage::queries::marketplace_queries::list_transactions(&conn, agent_id, 100, 0)
                .map_err(|e| MeshError::NotImplemented(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| MeshTransaction {
                id: Uuid::new_v4(),
                from_agent: r.from_agent.unwrap_or_default(),
                to_agent: r.to_agent.unwrap_or_default(),
                amount: r.amount as u64,
                currency: "credits".to_string(),
                created_at: chrono::Utc::now(),
            })
            .collect())
    }

    fn verify_receipt(&self, _receipt: &MeshReceipt) -> Result<bool, MeshError> {
        // Alpha: trust all receipts from local DB
        Ok(true)
    }
}
