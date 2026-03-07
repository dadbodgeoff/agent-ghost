//! Review system — feeds into LocalTrustStore → EigenTrust.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::{MarketplaceError, MarketplaceResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    pub id: i64,
    pub contract_id: String,
    pub reviewer_agent_id: String,
    pub reviewee_agent_id: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: String,
}

/// Submit a review for a completed contract.
pub fn submit_review(
    conn: &Connection,
    contract_id: &str,
    reviewer_agent_id: &str,
    reviewee_agent_id: &str,
    rating: i32,
    comment: &str,
) -> MarketplaceResult<()> {
    if !(1..=5).contains(&rating) {
        return Err(MarketplaceError::Validation(
            "rating must be between 1 and 5".into(),
        ));
    }

    // Verify contract is completed
    let contract =
        cortex_storage::queries::marketplace_queries::get_contract(conn, contract_id)?
            .ok_or_else(|| MarketplaceError::NotFound(format!("contract {contract_id}")))?;

    if contract.state != "completed" && contract.state != "resolved" {
        return Err(MarketplaceError::Validation(
            "can only review completed or resolved contracts".into(),
        ));
    }

    // Verify reviewer is party to the contract
    if contract.hirer_agent_id != reviewer_agent_id
        && contract.worker_agent_id != reviewer_agent_id
    {
        return Err(MarketplaceError::Validation(
            "reviewer must be a party to the contract".into(),
        ));
    }

    // Verify reviewee is the OTHER party (prevent self-review and arbitrary targets)
    if reviewer_agent_id == reviewee_agent_id {
        return Err(MarketplaceError::Validation(
            "cannot review yourself".into(),
        ));
    }
    if contract.hirer_agent_id != reviewee_agent_id
        && contract.worker_agent_id != reviewee_agent_id
    {
        return Err(MarketplaceError::Validation(
            "reviewee must be the other party in the contract".into(),
        ));
    }

    cortex_storage::queries::marketplace_queries::insert_review(
        conn,
        contract_id,
        reviewer_agent_id,
        reviewee_agent_id,
        rating,
        comment,
    )?;

    tracing::info!(
        contract_id = %contract_id,
        reviewer = %reviewer_agent_id,
        reviewee = %reviewee_agent_id,
        rating = rating,
        "review submitted"
    );

    Ok(())
}

/// List reviews for an agent.
pub fn list_reviews(
    conn: &Connection,
    agent_id: &str,
    limit: u32,
    offset: u32,
) -> MarketplaceResult<Vec<Review>> {
    let rows = cortex_storage::queries::marketplace_queries::list_reviews_for_agent(
        conn, agent_id, limit, offset,
    )?;
    Ok(rows
        .into_iter()
        .map(|r| Review {
            id: r.id,
            contract_id: r.contract_id,
            reviewer_agent_id: r.reviewer_agent_id,
            reviewee_agent_id: r.reviewee_agent_id,
            rating: r.rating,
            comment: r.comment,
            created_at: r.created_at,
        })
        .collect())
}
