//! Agent and skill listing management.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::error::MarketplaceResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentListing {
    pub agent_id: String,
    pub description: String,
    pub capabilities: Vec<String>,
    pub pricing_model: String,
    pub base_price: i64,
    pub trust_score: f64,
    pub total_completed: i64,
    pub total_failed: i64,
    pub average_rating: f64,
    pub total_reviews: i64,
    pub status: String,
    pub endpoint_url: Option<String>,
    pub public_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillListing {
    pub skill_name: String,
    pub version: String,
    pub author_agent_id: Option<String>,
    pub description: String,
    pub signature: Option<String>,
    pub price_credits: i64,
    pub install_count: i64,
    pub average_rating: f64,
    pub created_at: String,
}

/// Register or update an agent listing.
pub fn upsert_agent(
    conn: &Connection,
    agent_id: &str,
    description: &str,
    capabilities: &[String],
    pricing_model: &str,
    base_price: i64,
    endpoint_url: Option<&str>,
    public_key: Option<&str>,
) -> MarketplaceResult<()> {
    let caps_json = serde_json::to_string(capabilities).unwrap_or_else(|_| "[]".to_string());
    cortex_storage::queries::marketplace_queries::upsert_agent_listing(
        conn,
        agent_id,
        description,
        &caps_json,
        pricing_model,
        base_price,
        endpoint_url,
        public_key,
    )?;
    Ok(())
}

/// List agent listings with filters.
pub fn list_agents(
    conn: &Connection,
    status: Option<&str>,
    min_trust: Option<f64>,
    min_rating: Option<f64>,
    limit: u32,
    offset: u32,
) -> MarketplaceResult<Vec<AgentListing>> {
    let rows = cortex_storage::queries::marketplace_queries::list_agent_listings(
        conn, status, min_trust, min_rating, limit, offset,
    )?;
    Ok(rows.into_iter().map(row_to_agent_listing).collect())
}

/// Get a single agent listing.
pub fn get_agent(conn: &Connection, agent_id: &str) -> MarketplaceResult<Option<AgentListing>> {
    let row = cortex_storage::queries::marketplace_queries::get_agent_listing(conn, agent_id)?;
    Ok(row.map(row_to_agent_listing))
}

/// Set listing status (active, busy, offline, suspended).
pub fn set_agent_status(
    conn: &Connection,
    agent_id: &str,
    status: &str,
) -> MarketplaceResult<bool> {
    Ok(
        cortex_storage::queries::marketplace_queries::update_agent_listing_status(
            conn, agent_id, status,
        )?,
    )
}

/// Publish or update a skill listing.
pub fn upsert_skill(
    conn: &Connection,
    skill_name: &str,
    version: &str,
    author_agent_id: Option<&str>,
    description: &str,
    signature: Option<&str>,
    price_credits: i64,
) -> MarketplaceResult<()> {
    cortex_storage::queries::marketplace_queries::upsert_skill_listing(
        conn,
        skill_name,
        version,
        author_agent_id,
        description,
        signature,
        price_credits,
    )?;
    Ok(())
}

/// List skill listings.
pub fn list_skills(
    conn: &Connection,
    limit: u32,
    offset: u32,
) -> MarketplaceResult<Vec<SkillListing>> {
    let rows =
        cortex_storage::queries::marketplace_queries::list_skill_listings(conn, limit, offset)?;
    Ok(rows
        .into_iter()
        .map(|r| SkillListing {
            skill_name: r.skill_name,
            version: r.version,
            author_agent_id: r.author_agent_id,
            description: r.description,
            signature: r.signature,
            price_credits: r.price_credits,
            install_count: r.install_count,
            average_rating: r.average_rating,
            created_at: r.created_at,
        })
        .collect())
}

/// Get a single skill listing.
pub fn get_skill(conn: &Connection, skill_name: &str) -> MarketplaceResult<Option<SkillListing>> {
    let row = cortex_storage::queries::marketplace_queries::get_skill_listing(conn, skill_name)?;
    Ok(row.map(|r| SkillListing {
        skill_name: r.skill_name,
        version: r.version,
        author_agent_id: r.author_agent_id,
        description: r.description,
        signature: r.signature,
        price_credits: r.price_credits,
        install_count: r.install_count,
        average_rating: r.average_rating,
        created_at: r.created_at,
    }))
}

fn row_to_agent_listing(
    r: cortex_storage::queries::marketplace_queries::AgentListingRow,
) -> AgentListing {
    let capabilities: Vec<String> = serde_json::from_str(&r.capabilities).unwrap_or_default();
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
}
