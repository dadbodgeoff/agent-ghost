//! Mesh visualization endpoints for the dashboard (T-3.2.1, T-3.2.2, T-3.2.3).
//!
//! Separate from `mesh_routes.rs` which handles A2A protocol.
//! These endpoints return trust graphs, consensus state, and delegation chains
//! for visualization in the orchestration dashboard.

use std::collections::{HashMap, HashSet};
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
    let db = state.db.read()?;

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

    // Trust graph edges are derived from actual delegation relationships.
    // The weight is an explicit delegation-confidence proxy based on the
    // current persisted state of each delegation row.
    let mut stmt = db.prepare(
        "SELECT sender_id, recipient_id, state
         FROM delegation_state
         ORDER BY updated_at DESC, created_at DESC
         LIMIT 500",
    )?;

    let mut edge_support: HashMap<(String, String), Vec<String>> = HashMap::new();
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;

    for row in rows {
        let (sender_id, recipient_id, delegation_state) = row?;
        edge_support
            .entry((sender_id, recipient_id))
            .or_default()
            .push(delegation_state);
    }

    for ((source, target), states) in edge_support {
        let trust_score = compute_edge_trust_score(&states);
        if trust_score > 0.0 {
            edges.push(TrustEdge {
                source,
                target,
                trust_score,
            });
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
    let db = state.db.read()?;

    // ADE currently persists proposal lifecycle transitions rather than
    // N-of-M peer vote rows. This endpoint therefore reports the canonical
    // lifecycle state for recent proposals, with approval/rejection counts
    // modeled as terminal decision signals instead of fabricated vote totals.
    let mut stmt = db
        .prepare(
            "SELECT p.id, COALESCE(t.to_state, 'pending_review') AS current_state
             FROM goal_proposals_v2 p
             LEFT JOIN goal_proposal_transitions t
               ON t.rowid = (
                   SELECT rowid
                   FROM goal_proposal_transitions latest
                   WHERE latest.proposal_id = p.id
                   ORDER BY latest.rowid DESC
                   LIMIT 1
               )
             ORDER BY p.created_at DESC
             LIMIT 50",
        )
        .map_err(|e| ApiError::db_error("prepare consensus", e))?;

    let rounds: Vec<ConsensusRound> = stmt
        .query_map([], |row| {
            let state = row.get::<_, String>(1)?;
            let (status, approvals, rejections) = map_consensus_round(&state);
            Ok(ConsensusRound {
                proposal_id: row.get(0)?,
                status,
                approvals,
                rejections,
                threshold: 1,
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
    let db = state.db.read()?;

    let mut stmt = db
        .prepare(
            "SELECT sender_id, recipient_id, task, state, created_at
             FROM delegation_state
             ORDER BY updated_at DESC, created_at DESC
             LIMIT 100",
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

    let active_delegations: Vec<&Delegation> = delegations
        .iter()
        .filter(|delegation| is_active_delegation_state(&delegation.state))
        .collect();
    let total = active_delegations.len();
    let delegator_set: HashSet<String> = active_delegations
        .iter()
        .map(|delegation| delegation.delegator_id.clone())
        .collect();
    let max_chain_depth = compute_max_chain_depth(&active_delegations);

    Ok(Json(DelegationsResponse {
        sybil_metrics: SybilMetrics {
            total_delegations: total,
            max_chain_depth,
            unique_delegators: delegator_set.len(),
        },
        delegations,
    }))
}

fn compute_edge_trust_score(states: &[String]) -> f64 {
    if states.is_empty() {
        return 0.0;
    }

    let total_weight: f64 = states
        .iter()
        .map(|state| delegation_state_weight(state))
        .sum();
    (total_weight / states.len() as f64).clamp(0.0, 1.0)
}

fn delegation_state_weight(state: &str) -> f64 {
    match state {
        "Completed" => 1.0,
        "Accepted" => 0.75,
        "Offered" => 0.5,
        "Rejected" => 0.15,
        "Disputed" => 0.0,
        _ => 0.0,
    }
}

fn map_consensus_round(state: &str) -> (String, u32, u32) {
    match state {
        "approved" | "auto_applied" => ("approved".to_string(), 1, 0),
        "rejected" | "auto_rejected" | "timed_out" => ("rejected".to_string(), 0, 1),
        "superseded" => ("superseded".to_string(), 0, 0),
        other => (other.to_string(), 0, 0),
    }
}

fn is_active_delegation_state(state: &str) -> bool {
    matches!(state, "Offered" | "Accepted")
}

fn compute_max_chain_depth(delegations: &[&Delegation]) -> u32 {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for delegation in delegations {
        adjacency
            .entry(delegation.delegator_id.as_str())
            .or_default()
            .push(delegation.delegate_id.as_str());
    }

    let mut memo = HashMap::new();
    let mut max_depth = 0;
    for node in adjacency.keys().copied() {
        let mut visiting = HashSet::new();
        max_depth = max_depth.max(depth_from(node, &adjacency, &mut memo, &mut visiting));
    }
    max_depth
}

fn depth_from<'a>(
    node: &'a str,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
    memo: &mut HashMap<&'a str, u32>,
    visiting: &mut HashSet<&'a str>,
) -> u32 {
    if let Some(depth) = memo.get(node) {
        return *depth;
    }
    if !visiting.insert(node) {
        return 0;
    }

    let depth = adjacency
        .get(node)
        .map(|neighbors| {
            neighbors
                .iter()
                .map(|neighbor| 1 + depth_from(neighbor, adjacency, memo, visiting))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0);

    visiting.remove(node);
    memo.insert(node, depth);
    depth
}
