//! Marketplace API endpoints — agent listings, contracts, wallet, reviews, discovery.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Request / Response Types ──

#[derive(Debug, Deserialize)]
pub struct RegisterAgentRequest {
    pub agent_id: String,
    pub description: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_pricing_model")]
    pub pricing_model: String,
    #[serde(default = "default_base_price")]
    pub base_price: i64,
    pub endpoint_url: Option<String>,
    pub public_key: Option<String>,
}

fn default_pricing_model() -> String {
    "per_task".to_string()
}
fn default_base_price() -> i64 {
    100
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentStatusRequest {
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct AgentListingResponse {
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListAgentsQuery {
    pub status: Option<String>,
    pub min_trust: Option<f64>,
    pub min_rating: Option<f64>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct AgentListResponse {
    pub agents: Vec<AgentListingResponse>,
}

#[derive(Debug, Deserialize)]
pub struct PublishSkillRequest {
    pub skill_name: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub author_agent_id: Option<String>,
    pub description: String,
    pub signature: Option<String>,
    #[serde(default)]
    pub price_credits: i64,
}

fn default_version() -> String {
    "0.1.0".to_string()
}

#[derive(Debug, Serialize)]
pub struct SkillListingResponse {
    pub skill_name: String,
    pub version: String,
    pub author_agent_id: Option<String>,
    pub description: String,
    pub price_credits: i64,
    pub install_count: i64,
    pub average_rating: f64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListSkillsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SkillListResponse {
    pub skills: Vec<SkillListingResponse>,
}

#[derive(Debug, Deserialize)]
pub struct ProposeContractRequest {
    pub hirer_agent_id: String,
    pub worker_agent_id: String,
    pub task_description: String,
    pub agreed_price: i64,
    pub max_duration_secs: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct ContractResponse {
    pub id: String,
    pub hirer_agent_id: String,
    pub worker_agent_id: String,
    pub state: String,
    pub task_description: String,
    pub agreed_price: i64,
    pub max_duration_secs: Option<i64>,
    pub escrow_id: Option<String>,
    pub result: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ListContractsQuery {
    pub agent_id: Option<String>,
    pub state: Option<String>,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct ContractListResponse {
    pub contracts: Vec<ContractResponse>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteContractRequest {
    pub result: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WalletResponse {
    pub agent_id: String,
    pub balance: i64,
    pub escrowed: i64,
    pub total_earned: i64,
    pub total_spent: i64,
}

#[derive(Debug, Deserialize)]
pub struct SeedWalletRequest {
    pub agent_id: String,
    #[serde(default = "default_seed_amount")]
    pub amount: i64,
}

fn default_seed_amount() -> i64 {
    10_000
}

#[derive(Debug, Deserialize)]
pub struct ListTransactionsQuery {
    pub agent_id: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct TransactionResponse {
    pub id: i64,
    pub from_agent: Option<String>,
    pub to_agent: Option<String>,
    pub amount: i64,
    pub tx_type: String,
    pub reference_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct TransactionListResponse {
    pub transactions: Vec<TransactionResponse>,
}

#[derive(Debug, Deserialize)]
pub struct SubmitReviewRequest {
    pub contract_id: String,
    pub reviewer_agent_id: String,
    pub reviewee_agent_id: String,
    pub rating: i32,
    #[serde(default)]
    pub comment: String,
}

#[derive(Debug, Serialize)]
pub struct ReviewResponse {
    pub id: i64,
    pub contract_id: String,
    pub reviewer_agent_id: String,
    pub reviewee_agent_id: String,
    pub rating: i32,
    pub comment: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ReviewListResponse {
    pub reviews: Vec<ReviewResponse>,
}

#[derive(Debug, Deserialize)]
pub struct ListReviewsQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct DiscoverRequest {
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub min_trust: Option<f64>,
    pub min_rating: Option<f64>,
    pub max_price: Option<i64>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct DiscoverResponse {
    pub agents: Vec<AgentListingResponse>,
    pub total_matches: usize,
}

#[derive(Debug, Deserialize)]
pub struct WalletQuery {
    pub agent_id: String,
}

// ── Agent Listing Handlers ──

/// POST /api/marketplace/agents — register for hire.
pub async fn register_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterAgentRequest>,
) -> ApiResult<AgentListingResponse> {
    let db = state.db.write().await;

    ghost_marketplace::listings::upsert_agent(
        &db,
        &req.agent_id,
        &req.description,
        &req.capabilities,
        &req.pricing_model,
        req.base_price,
        req.endpoint_url.as_deref(),
        req.public_key.as_deref(),
    )
    .map_err(|e| ApiError::db_error("register_agent", e))?;

    let listing = ghost_marketplace::listings::get_agent(&db, &req.agent_id)
        .map_err(|e| ApiError::db_error("get_agent", e))?
        .ok_or_else(|| ApiError::internal("agent listing created but not found"))?;

    Ok(Json(to_agent_response(listing)))
}

/// GET /api/marketplace/agents — browse/search agents.
pub async fn list_agents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListAgentsQuery>,
) -> ApiResult<AgentListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_agents", e))?;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let listings = ghost_marketplace::listings::list_agents(
        &db,
        params.status.as_deref(),
        params.min_trust,
        params.min_rating,
        limit,
        offset,
    )
    .map_err(|e| ApiError::db_error("list_agents", e))?;

    Ok(Json(AgentListResponse {
        agents: listings.into_iter().map(to_agent_response).collect(),
    }))
}

/// GET /api/marketplace/agents/:id — agent listing detail.
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<AgentListingResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_agent", e))?;

    let listing = ghost_marketplace::listings::get_agent(&db, &id)
        .map_err(|e| ApiError::db_error("get_agent", e))?
        .ok_or_else(|| ApiError::not_found(format!("agent listing {id}")))?;

    Ok(Json(to_agent_response(listing)))
}

/// PUT /api/marketplace/agents/:id/status — update agent status.
pub async fn update_agent_status(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<UpdateAgentStatusRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    let valid_statuses = ["active", "busy", "offline", "suspended"];
    if !valid_statuses.contains(&req.status.as_str()) {
        return Err(ApiError::bad_request(format!(
            "invalid status: {}. Must be one of: {}",
            req.status,
            valid_statuses.join(", ")
        )));
    }

    let updated = ghost_marketplace::listings::set_agent_status(&db, &id, &req.status)
        .map_err(|e| ApiError::db_error("update_status", e))?;

    if !updated {
        return Err(ApiError::not_found(format!("agent listing {id}")));
    }

    Ok(Json(serde_json::json!({ "updated": true })))
}

/// DELETE /api/marketplace/agents/:id — delist agent.
pub async fn delist_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::listings::set_agent_status(&db, &id, "offline")
        .map_err(|e| ApiError::db_error("delist_agent", e))?;

    Ok(Json(serde_json::json!({ "delisted": true })))
}

// ── Skill Listing Handlers ──

/// POST /api/marketplace/skills — publish skill.
pub async fn publish_skill(
    State(state): State<Arc<AppState>>,
    Json(req): Json<PublishSkillRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::listings::upsert_skill(
        &db,
        &req.skill_name,
        &req.version,
        req.author_agent_id.as_deref(),
        &req.description,
        req.signature.as_deref(),
        req.price_credits,
    )
    .map_err(|e| ApiError::db_error("publish_skill", e))?;

    Ok(Json(
        serde_json::json!({ "published": true, "skill_name": req.skill_name }),
    ))
}

/// GET /api/marketplace/skills — browse skills.
pub async fn list_skills(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListSkillsQuery>,
) -> ApiResult<SkillListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_skills", e))?;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let listings = ghost_marketplace::listings::list_skills(&db, limit, offset)
        .map_err(|e| ApiError::db_error("list_skills", e))?;

    Ok(Json(SkillListResponse {
        skills: listings
            .into_iter()
            .map(|s| SkillListingResponse {
                skill_name: s.skill_name,
                version: s.version,
                author_agent_id: s.author_agent_id,
                description: s.description,
                price_credits: s.price_credits,
                install_count: s.install_count,
                average_rating: s.average_rating,
                created_at: s.created_at,
            })
            .collect(),
    }))
}

/// GET /api/marketplace/skills/:name — skill detail.
pub async fn get_skill(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> ApiResult<SkillListingResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_skill", e))?;

    let skill = ghost_marketplace::listings::get_skill(&db, &name)
        .map_err(|e| ApiError::db_error("get_skill", e))?
        .ok_or_else(|| ApiError::not_found(format!("skill {name}")))?;

    Ok(Json(SkillListingResponse {
        skill_name: skill.skill_name,
        version: skill.version,
        author_agent_id: skill.author_agent_id,
        description: skill.description,
        price_credits: skill.price_credits,
        install_count: skill.install_count,
        average_rating: skill.average_rating,
        created_at: skill.created_at,
    }))
}

// ── Contract Handlers ──

/// POST /api/marketplace/contracts — propose hiring.
pub async fn propose_contract(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ProposeContractRequest>,
) -> ApiResult<ContractResponse> {
    let db = state.db.write().await;

    let contract_id = ghost_marketplace::contracts::propose_contract(
        &db,
        &req.hirer_agent_id,
        &req.worker_agent_id,
        &req.task_description,
        req.agreed_price,
        req.max_duration_secs,
    )
    .map_err(|e| map_marketplace_error("propose_contract", e))?;

    let contract = cortex_storage::queries::marketplace_queries::get_contract(&db, &contract_id)
        .map_err(|e| ApiError::db_error("get_contract", e))?
        .ok_or_else(|| ApiError::internal("contract created but not found"))?;

    Ok(Json(to_contract_response(contract)))
}

/// GET /api/marketplace/contracts — list contracts.
pub async fn list_contracts(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListContractsQuery>,
) -> ApiResult<ContractListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_contracts", e))?;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let contracts = cortex_storage::queries::marketplace_queries::list_contracts(
        &db,
        params.agent_id.as_deref(),
        params.state.as_deref(),
        limit,
        offset,
    )
    .map_err(|e| ApiError::db_error("list_contracts", e))?;

    Ok(Json(ContractListResponse {
        contracts: contracts.into_iter().map(to_contract_response).collect(),
    }))
}

/// GET /api/marketplace/contracts/:id — contract detail.
pub async fn get_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<ContractResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_contract", e))?;

    let contract = cortex_storage::queries::marketplace_queries::get_contract(&db, &id)
        .map_err(|e| ApiError::db_error("get_contract", e))?
        .ok_or_else(|| ApiError::not_found(format!("contract {id}")))?;

    Ok(Json(to_contract_response(contract)))
}

/// POST /api/marketplace/contracts/:id/accept — worker accepts.
pub async fn accept_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Accepted,
        None,
    )
    .map_err(|e| map_marketplace_error("accept_contract", e))?;

    Ok(Json(serde_json::json!({ "accepted": true })))
}

/// POST /api/marketplace/contracts/:id/reject — worker rejects.
pub async fn reject_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Rejected,
        None,
    )
    .map_err(|e| map_marketplace_error("reject_contract", e))?;

    Ok(Json(serde_json::json!({ "rejected": true })))
}

/// POST /api/marketplace/contracts/:id/start — begin work.
pub async fn start_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::InProgress,
        None,
    )
    .map_err(|e| map_marketplace_error("start_contract", e))?;

    Ok(Json(serde_json::json!({ "started": true })))
}

/// POST /api/marketplace/contracts/:id/complete — deliver result, release escrow.
pub async fn complete_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CompleteContractRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Completed,
        req.result.as_deref(),
    )
    .map_err(|e| map_marketplace_error("complete_contract", e))?;

    Ok(Json(serde_json::json!({ "completed": true })))
}

/// POST /api/marketplace/contracts/:id/dispute — dispute contract.
pub async fn dispute_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Disputed,
        None,
    )
    .map_err(|e| map_marketplace_error("dispute_contract", e))?;

    Ok(Json(serde_json::json!({ "disputed": true })))
}

/// POST /api/marketplace/contracts/:id/cancel — hirer cancels, refund escrow.
pub async fn cancel_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Canceled,
        None,
    )
    .map_err(|e| map_marketplace_error("cancel_contract", e))?;

    Ok(Json(serde_json::json!({ "canceled": true })))
}

/// POST /api/marketplace/contracts/:id/resolve — resolve disputed contract.
/// Pass `{"result": "release_to_worker"}` to pay the worker, otherwise refunds hirer.
pub async fn resolve_contract(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<CompleteContractRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::contracts::transition_contract(
        &db,
        &id,
        ghost_marketplace::contracts::ContractState::Resolved,
        req.result.as_deref(),
    )
    .map_err(|e| map_marketplace_error("resolve_contract", e))?;

    Ok(Json(serde_json::json!({ "resolved": true })))
}

// ── Wallet Handlers ──

/// GET /api/marketplace/wallet — balance query.
pub async fn get_wallet(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WalletQuery>,
) -> ApiResult<WalletResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("get_wallet", e))?;

    let wallet = cortex_storage::queries::marketplace_queries::get_wallet(&db, &params.agent_id)
        .map_err(|e| ApiError::db_error("get_wallet", e))?
        .ok_or_else(|| ApiError::not_found(format!("wallet for {}", params.agent_id)))?;

    Ok(Json(WalletResponse {
        agent_id: wallet.agent_id,
        balance: wallet.balance,
        escrowed: wallet.escrowed,
        total_earned: wallet.total_earned,
        total_spent: wallet.total_spent,
    }))
}

/// POST /api/marketplace/wallet/seed — self-service credit seeding (alpha).
pub async fn seed_wallet(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SeedWalletRequest>,
) -> ApiResult<WalletResponse> {
    let db = state.db.write().await;

    cortex_storage::queries::marketplace_queries::seed_wallet(&db, &req.agent_id, req.amount)
        .map_err(|e| ApiError::db_error("seed_wallet", e))?;

    let wallet = cortex_storage::queries::marketplace_queries::get_wallet(&db, &req.agent_id)
        .map_err(|e| ApiError::db_error("get_wallet", e))?
        .ok_or_else(|| ApiError::internal("wallet seeded but not found"))?;

    Ok(Json(WalletResponse {
        agent_id: wallet.agent_id,
        balance: wallet.balance,
        escrowed: wallet.escrowed,
        total_earned: wallet.total_earned,
        total_spent: wallet.total_spent,
    }))
}

/// GET /api/marketplace/wallet/transactions — transaction history.
pub async fn list_transactions(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListTransactionsQuery>,
) -> ApiResult<TransactionListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_transactions", e))?;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let transactions = cortex_storage::queries::marketplace_queries::list_transactions(
        &db,
        &params.agent_id,
        limit,
        offset,
    )
    .map_err(|e| ApiError::db_error("list_transactions", e))?;

    Ok(Json(TransactionListResponse {
        transactions: transactions
            .into_iter()
            .map(|t| TransactionResponse {
                id: t.id,
                from_agent: t.from_agent,
                to_agent: t.to_agent,
                amount: t.amount,
                tx_type: t.tx_type,
                reference_id: t.reference_id,
                created_at: t.created_at,
            })
            .collect(),
    }))
}

// ── Review Handlers ──

/// POST /api/marketplace/reviews — submit review.
pub async fn submit_review(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SubmitReviewRequest>,
) -> ApiResult<serde_json::Value> {
    let db = state.db.write().await;

    ghost_marketplace::reviews::submit_review(
        &db,
        &req.contract_id,
        &req.reviewer_agent_id,
        &req.reviewee_agent_id,
        req.rating,
        &req.comment,
    )
    .map_err(|e| map_marketplace_error("submit_review", e))?;

    Ok(Json(serde_json::json!({ "submitted": true })))
}

/// GET /api/marketplace/reviews/:agent_id — reviews for an agent.
pub async fn list_reviews(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
    Query(params): Query<ListReviewsQuery>,
) -> ApiResult<ReviewListResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("list_reviews", e))?;
    let limit = params.limit.unwrap_or(50);
    let offset = params.offset.unwrap_or(0);

    let reviews = ghost_marketplace::reviews::list_reviews(&db, &agent_id, limit, offset)
        .map_err(|e| ApiError::db_error("list_reviews", e))?;

    Ok(Json(ReviewListResponse {
        reviews: reviews
            .into_iter()
            .map(|r| ReviewResponse {
                id: r.id,
                contract_id: r.contract_id,
                reviewer_agent_id: r.reviewer_agent_id,
                reviewee_agent_id: r.reviewee_agent_id,
                rating: r.rating,
                comment: r.comment,
                created_at: r.created_at,
            })
            .collect(),
    }))
}

// ── Discovery Handler ──

/// POST /api/marketplace/discover — capability-based agent matching.
pub async fn discover_agents(
    State(state): State<Arc<AppState>>,
    Json(req): Json<DiscoverRequest>,
) -> ApiResult<DiscoverResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::db_error("discover_agents", e))?;

    let request = ghost_marketplace::discovery::DiscoverRequest {
        capabilities: req.capabilities,
        min_trust: req.min_trust,
        min_rating: req.min_rating,
        max_price: req.max_price,
        limit: req.limit,
    };

    let result = ghost_marketplace::discovery::discover_agents(&db, &request)
        .map_err(|e| ApiError::db_error("discover_agents", e))?;

    Ok(Json(DiscoverResponse {
        total_matches: result.total_matches,
        agents: result.agents.into_iter().map(to_agent_response).collect(),
    }))
}

// ── Helpers ──

fn to_agent_response(l: ghost_marketplace::listings::AgentListing) -> AgentListingResponse {
    AgentListingResponse {
        agent_id: l.agent_id,
        description: l.description,
        capabilities: l.capabilities,
        pricing_model: l.pricing_model,
        base_price: l.base_price,
        trust_score: l.trust_score,
        total_completed: l.total_completed,
        total_failed: l.total_failed,
        average_rating: l.average_rating,
        total_reviews: l.total_reviews,
        status: l.status,
        endpoint_url: l.endpoint_url,
        created_at: l.created_at,
        updated_at: l.updated_at,
    }
}

fn to_contract_response(
    c: cortex_storage::queries::marketplace_queries::ContractRow,
) -> ContractResponse {
    ContractResponse {
        id: c.id,
        hirer_agent_id: c.hirer_agent_id,
        worker_agent_id: c.worker_agent_id,
        state: c.state,
        task_description: c.task_description,
        agreed_price: c.agreed_price,
        max_duration_secs: c.max_duration_secs,
        escrow_id: c.escrow_id,
        result: c.result,
        created_at: c.created_at,
        updated_at: c.updated_at,
    }
}

/// Map marketplace errors to consistent API errors.
fn map_marketplace_error(context: &str, e: ghost_marketplace::error::MarketplaceError) -> ApiError {
    match &e {
        ghost_marketplace::error::MarketplaceError::InvalidTransition { .. } => {
            ApiError::conflict(e.to_string())
        }
        ghost_marketplace::error::MarketplaceError::NotFound(_) => {
            ApiError::not_found(e.to_string())
        }
        ghost_marketplace::error::MarketplaceError::Validation(_) => {
            ApiError::bad_request(e.to_string())
        }
        ghost_marketplace::error::MarketplaceError::InsufficientBalance { .. } => {
            ApiError::bad_request(e.to_string())
        }
        _ => ApiError::db_error(context, e),
    }
}
