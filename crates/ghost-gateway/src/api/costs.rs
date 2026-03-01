//! Cost tracking API endpoints (Req 27 AC1).
//!
//! Reads from the in-memory CostTracker which holds per-agent daily totals,
//! per-session totals, and compaction costs.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct AgentCostInfo {
    pub agent_id: String,
    pub agent_name: String,
    pub daily_total: f64,
    pub compaction_cost: f64,
    pub spending_cap: f64,
    pub cap_remaining: f64,
    pub cap_utilization_pct: f64,
}

/// GET /api/costs — per-agent cost summary.
///
/// Returns daily spend, compaction cost, spending cap, and utilization
/// for each registered agent. Data comes from the in-memory CostTracker
/// which is fed by ghost-llm cost calculations after each LLM call.
pub async fn get_costs(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AgentCostInfo>>, StatusCode> {
    let agents = match state.agents.read() {
        Ok(guard) => guard,
        Err(e) => {
            tracing::error!(error = %e, "Agent registry RwLock poisoned in get_costs");
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };
    let costs: Vec<AgentCostInfo> = agents
        .all_agents()
        .iter()
        .map(|a| {
            let daily = state.cost_tracker.get_daily_total(a.id);
            let compaction = state.cost_tracker.get_compaction_cost(a.id);
            let remaining = (a.spending_cap - daily).max(0.0);
            let utilization = if a.spending_cap > 0.0 {
                (daily / a.spending_cap * 100.0).min(100.0)
            } else {
                0.0
            };
            AgentCostInfo {
                agent_id: a.id.to_string(),
                agent_name: a.name.clone(),
                daily_total: daily,
                compaction_cost: compaction,
                spending_cap: a.spending_cap,
                cap_remaining: remaining,
                cap_utilization_pct: utilization,
            }
        })
        .collect();
    Ok(Json(costs))
}
