//! Mesh visualization endpoints for the dashboard (T-3.2.1, T-3.2.2, T-3.2.3).
//!
//! Separate from `mesh_routes.rs` which handles A2A protocol.
//! These endpoints return trust graphs, consensus state, and delegation chains
//! for visualization in the orchestration dashboard.

use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::error::{ApiError, ApiResult};
use crate::state::AppState;

// ── Trust Graph (T-3.2.1) ──────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct TrustGraphResponse {
    pub nodes: Vec<TrustNode>,
    pub edges: Vec<TrustEdge>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrustNode {
    pub id: String,
    pub name: String,
    pub activity: f64,
    pub convergence_level: u8,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TrustEdge {
    pub source: String,
    pub target: String,
    pub trust_score: f64,
}

/// GET /api/mesh/trust-graph — return trust graph from convergence scores.
pub async fn trust_graph(State(state): State<Arc<AppState>>) -> ApiResult<TrustGraphResponse> {
    let agents = state
        .agents
        .read()
        .map_err(|_| ApiError::lock_poisoned("agents"))?;
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::internal(format!("db pool: {e}")))?;

    let all = agents.all_agents();
    let mut nodes = Vec::with_capacity(all.len());
    let mut edges = Vec::new();

    for agent in &all {
        let aid = agent.id.to_string();
        // Fetch latest convergence score for this agent.
        let row = cortex_storage::queries::convergence_score_queries::latest_by_agent(&db, &aid);
        let (score, level) = match row {
            Ok(Some(r)) => (r.composite_score, r.level as u8),
            _ => (0.0, 0),
        };

        nodes.push(TrustNode {
            id: aid.clone(),
            name: agent.name.clone(),
            activity: score,
            convergence_level: level,
        });
    }

    // T-5.7.3: Query actual trust edges from delegation_state table
    // (EigenTrust computation deferred to P3). Only emit edges where
    // actual interaction data exists, not synthetic heuristics.
    {
        let mut edge_stmt = db
            .prepare(
                "SELECT sender_id, recipient_id, \
                 CAST(COALESCE(json_extract(metadata, '$.trust_score'), 0.5) AS REAL) as trust \
                 FROM delegation_state \
                 WHERE state = 'active' \
                 LIMIT 500",
            )
            .ok();
        if let Some(ref mut stmt) = edge_stmt {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(TrustEdge {
                    source: row.get(0)?,
                    target: row.get(1)?,
                    trust_score: row.get::<_, f64>(2).unwrap_or(0.5),
                })
            }) {
                for row in rows.flatten() {
                    edges.push(row);
                }
            }
        }
    }

    Ok(Json(TrustGraphResponse { nodes, edges }))
}

// ── Consensus State (T-3.2.2) ──────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsensusResponse {
    pub rounds: Vec<ConsensusRound>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ConsensusRound {
    pub proposal_id: String,
    pub status: String,
    pub approvals: u32,
    pub rejections: u32,
    pub threshold: u32,
}

/// GET /api/mesh/consensus — return N-of-M consensus state.
pub async fn consensus_state(State(state): State<Arc<AppState>>) -> ApiResult<ConsensusResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::internal(format!("db pool: {e}")))?;

    // Query recent proposals with vote counts from dimension_scores.
    // dimension_scores is JSON; non-null indicates a proposal that went through consensus.
    let mut stmt = db
        .prepare(
            "SELECT p.id, COALESCE(p.decision, 'pending'), \
                    (SELECT COUNT(*) FROM goal_proposals p2 WHERE p2.id = p.id AND p2.decision = 'approved') as approvals, \
                    (SELECT COUNT(*) FROM goal_proposals p3 WHERE p3.id = p.id AND p3.decision = 'rejected') as rejections \
             FROM goal_proposals p \
             WHERE p.dimension_scores IS NOT NULL \
             ORDER BY p.created_at DESC LIMIT 50",
        )
        .map_err(|e| ApiError::db_error("prepare consensus", e))?;

    // Count total agents for N-of-M threshold.
    let agent_count: u32 = {
        let agents = state
            .agents
            .read()
            .map_err(|_| ApiError::lock_poisoned("agents"))?;
        agents.all_agents().len() as u32
    };
    let threshold = (agent_count / 2) + 1; // Simple majority

    let rounds: Vec<ConsensusRound> = stmt
        .query_map([], |row| {
            Ok(ConsensusRound {
                proposal_id: row.get(0)?,
                status: row.get(1)?,
                approvals: row.get::<_, u32>(2).unwrap_or(0),
                rejections: row.get::<_, u32>(3).unwrap_or(0),
                threshold,
            })
        })
        .map_err(|e| ApiError::db_error("query consensus", e))?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Json(ConsensusResponse { rounds }))
}

// ── Delegations (T-3.2.3) ──────────────────────────────────────────

#[derive(Debug, Serialize, ToSchema)]
pub struct DelegationsResponse {
    pub delegations: Vec<Delegation>,
    pub sybil_metrics: SybilMetrics,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct Delegation {
    pub delegator_id: String,
    pub delegate_id: String,
    pub scope: String,
    pub state: String,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SybilMetrics {
    pub total_delegations: usize,
    pub max_chain_depth: u32,
    pub unique_delegators: usize,
}

/// GET /api/mesh/delegations — return delegation chains and sybil metrics.
pub async fn delegations(State(state): State<Arc<AppState>>) -> ApiResult<DelegationsResponse> {
    let db = state
        .db
        .read()
        .map_err(|e| ApiError::internal(format!("db pool: {e}")))?;

    let mut stmt = db
        .prepare(
            "SELECT sender_id, recipient_id, task, state, created_at \
             FROM delegation_state ORDER BY created_at DESC LIMIT 100",
        )
        .map_err(|e| ApiError::db_error("prepare delegations", e))?;

    let delegations: Vec<Delegation> = stmt
        .query_map([], |row| {
            Ok(Delegation {
                delegator_id: row.get(0)?,
                delegate_id: row.get(1)?,
                scope: row.get::<_, String>(2).unwrap_or_default(),
                state: row.get::<_, String>(3).unwrap_or_default(),
                created_at: row.get::<_, String>(4).unwrap_or_default(),
            })
        })
        .map_err(|e| ApiError::db_error("query delegations", e))?
        .filter_map(|r| r.ok())
        .collect();

    let total = delegations.len();
    let mut delegator_set = std::collections::HashSet::new();
    for d in &delegations {
        delegator_set.insert(d.delegator_id.clone());
    }

    Ok(Json(DelegationsResponse {
        sybil_metrics: SybilMetrics {
            total_delegations: total,
            max_chain_depth: 1, // Single-level for P3
            unique_delegators: delegator_set.len(),
        },
        delegations,
    }))
}
