//! Hiring contract FSM.
//!
//! State machine: Proposed → Accepted → InProgress → Completed
//!                  |    |       |           |
//!                  |    → Rejected → Canceled → Disputed → Resolved
//!                  → Canceled (hirer withdraws)

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

use crate::error::{MarketplaceError, MarketplaceResult};

/// Valid contract states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractState {
    Proposed,
    Accepted,
    Rejected,
    InProgress,
    Completed,
    Disputed,
    Canceled,
    Resolved,
}

impl ContractState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Proposed => "proposed",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::InProgress => "in_progress",
            Self::Completed => "completed",
            Self::Disputed => "disputed",
            Self::Canceled => "canceled",
            Self::Resolved => "resolved",
        }
    }

    /// Check if a transition from self -> target is valid.
    pub fn can_transition_to(&self, target: Self) -> bool {
        matches!(
            (self, target),
            (Self::Proposed, Self::Accepted)
                | (Self::Proposed, Self::Rejected)
                | (Self::Proposed, Self::Canceled)
                | (Self::Accepted, Self::InProgress)
                | (Self::Accepted, Self::Canceled)
                | (Self::InProgress, Self::Completed)
                | (Self::InProgress, Self::Disputed)
                | (Self::Disputed, Self::Resolved)
        )
    }
}

impl FromStr for ContractState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "proposed" => Ok(Self::Proposed),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            "in_progress" => Ok(Self::InProgress),
            "completed" => Ok(Self::Completed),
            "disputed" => Ok(Self::Disputed),
            "canceled" => Ok(Self::Canceled),
            "resolved" => Ok(Self::Resolved),
            _ => Err(()),
        }
    }
}

/// Propose a new contract: create contract + escrow.
pub fn propose_contract(
    conn: &Connection,
    hirer_agent_id: &str,
    worker_agent_id: &str,
    task_description: &str,
    agreed_price: i64,
    max_duration_secs: Option<i64>,
) -> MarketplaceResult<String> {
    if agreed_price <= 0 {
        return Err(MarketplaceError::Validation(
            "agreed_price must be positive".into(),
        ));
    }
    if hirer_agent_id == worker_agent_id {
        return Err(MarketplaceError::Validation(
            "hirer and worker cannot be the same agent".into(),
        ));
    }
    if task_description.trim().is_empty() {
        return Err(MarketplaceError::Validation(
            "task_description must not be empty".into(),
        ));
    }

    let contract_id = uuid::Uuid::new_v4().to_string();
    let escrow_id = uuid::Uuid::new_v4().to_string();

    // Compute hash chain
    let event_hash = blake3::hash(
        format!("propose:{contract_id}:{hirer_agent_id}:{worker_agent_id}:{agreed_price}")
            .as_bytes(),
    )
    .to_hex()
    .to_string();

    // Create contract first, then escrow. If contract insert fails, no funds are locked.
    cortex_storage::queries::marketplace_queries::insert_contract(
        conn,
        &contract_id,
        hirer_agent_id,
        worker_agent_id,
        task_description,
        agreed_price,
        max_duration_secs,
        Some(&escrow_id),
        Some(&event_hash),
        None,
    )?;

    // Create escrow (will fail if insufficient balance — contract row will remain
    // in 'proposed' state but can be cleaned up or rejected)
    if let Err(_e) = cortex_storage::queries::marketplace_queries::create_escrow(
        conn,
        &escrow_id,
        &contract_id,
        hirer_agent_id,
        worker_agent_id,
        agreed_price,
    ) {
        // Roll back the contract since escrow failed
        let _ = cortex_storage::queries::marketplace_queries::transition_contract(
            conn,
            &contract_id,
            "rejected",
            None,
            None,
            Some("escrow creation failed"),
            None,
            None,
        );
        return Err(MarketplaceError::InsufficientBalance {
            required: agreed_price,
            available: 0,
        });
    }

    tracing::info!(
        contract_id = %contract_id,
        hirer = %hirer_agent_id,
        worker = %worker_agent_id,
        price = agreed_price,
        "contract proposed with escrow"
    );

    Ok(contract_id)
}

/// Transition a contract to a new state with validation.
pub fn transition_contract(
    conn: &Connection,
    contract_id: &str,
    target_state: ContractState,
    result: Option<&str>,
) -> MarketplaceResult<()> {
    let contract = cortex_storage::queries::marketplace_queries::get_contract(conn, contract_id)?
        .ok_or_else(|| MarketplaceError::NotFound(format!("contract {contract_id}")))?;

    let current = contract
        .state
        .parse::<ContractState>()
        .map_err(|()| MarketplaceError::Validation(format!("unknown state: {}", contract.state)))?;

    if !current.can_transition_to(target_state) {
        return Err(MarketplaceError::InvalidTransition {
            from: contract.state,
            to: target_state.as_str().to_string(),
        });
    }

    // Compute hash chain (includes result in hash for audit integrity)
    let result_hash_part = result.unwrap_or("");
    let event_hash = blake3::hash(
        format!(
            "transition:{contract_id}:{}:{}:{}",
            current.as_str(),
            target_state.as_str(),
            result_hash_part,
        )
        .as_bytes(),
    )
    .to_hex()
    .to_string();

    cortex_storage::queries::marketplace_queries::transition_contract(
        conn,
        contract_id,
        target_state.as_str(),
        None,
        None,
        result,
        Some(&event_hash),
        contract.event_hash.as_deref(),
    )?;

    // Handle side effects
    match target_state {
        ContractState::Completed => {
            // Release escrow to worker
            if let Some(escrow_id) = &contract.escrow_id {
                cortex_storage::queries::marketplace_queries::release_escrow(conn, escrow_id)?;
            }
            // Update listing stats
            let _ = cortex_storage::queries::marketplace_queries::increment_completed(
                conn,
                &contract.worker_agent_id,
            );
        }
        ContractState::Rejected | ContractState::Canceled => {
            // Refund escrow to hirer
            if let Some(escrow_id) = &contract.escrow_id {
                cortex_storage::queries::marketplace_queries::refund_escrow(conn, escrow_id)?;
            }
        }
        ContractState::Disputed => {
            // Mark worker as having a failure (dispute initiated)
            let _ = cortex_storage::queries::marketplace_queries::increment_failed(
                conn,
                &contract.worker_agent_id,
            );
            // Escrow stays held until resolution
        }
        ContractState::Resolved => {
            // For alpha: disputed contracts refund on resolution.
            // Future: result field could indicate "release" vs "refund" for arbitration.
            if let Some(escrow_id) = &contract.escrow_id {
                if result == Some("release_to_worker") {
                    cortex_storage::queries::marketplace_queries::release_escrow(conn, escrow_id)?;
                } else {
                    // Default: refund to hirer
                    cortex_storage::queries::marketplace_queries::refund_escrow(conn, escrow_id)?;
                }
            }
        }
        _ => {}
    }

    tracing::info!(
        contract_id = %contract_id,
        from = %current.as_str(),
        to = %target_state.as_str(),
        "contract state transitioned"
    );

    Ok(())
}
