//! Marketplace discovery — capability matching + trust-weighted ranking.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::MarketplaceResult;
use crate::listings::AgentListing;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverRequest {
    pub capabilities: Vec<String>,
    pub min_trust: Option<f64>,
    pub min_rating: Option<f64>,
    pub max_price: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoverResult {
    pub agents: Vec<AgentListing>,
    pub total_matches: usize,
}

/// Discover agents matching requested capabilities.
///
/// Each requested capability must appear in the agent's JSON capabilities array.
/// Ranking: `(trust * 0.4) + (rating/5 * 0.3) + (completion_rate * 0.2) + (price_fit * 0.1)`
pub fn discover_agents(
    conn: &Connection,
    request: &DiscoverRequest,
) -> MarketplaceResult<DiscoverResult> {
    let min_trust = request.min_trust.unwrap_or(0.0);
    let min_rating = request.min_rating.unwrap_or(0.0);
    let limit = request.limit.unwrap_or(20);

    let rows = cortex_storage::queries::marketplace_queries::discover_agents(
        conn,
        &request.capabilities,
        min_trust,
        min_rating,
        request.max_price,
        limit,
    )?;

    let agents: Vec<AgentListing> = rows
        .into_iter()
        .map(|r| {
            let capabilities: Vec<String> =
                serde_json::from_str(&r.capabilities).unwrap_or_default();
            AgentListing {
                agent_id: r.agent_id,
                description: r.description,
                capabilities,
                pricing_model: r.pricing_model,
                base_price: r.base_price,
                trust_score: r.trust_score,
                total_completed: r.total_completed,
                total_failed: r.total_failed,
                average_rating: r.average_rating,
                total_reviews: r.total_reviews,
                status: r.status,
                endpoint_url: r.endpoint_url,
                public_key: r.public_key,
                created_at: r.created_at,
                updated_at: r.updated_at,
            }
        })
        .collect();

    let total = agents.len();
    Ok(DiscoverResult {
        agents,
        total_matches: total,
    })
}
