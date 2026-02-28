//! Cost tracker: per-agent daily totals, per-session totals (Req 27 AC1).

use dashmap::DashMap;
use uuid::Uuid;

/// Cost tracker with per-agent and per-session tracking.
pub struct CostTracker {
    /// Per-agent daily totals.
    agent_daily: DashMap<Uuid, f64>,
    /// Per-session totals.
    session_totals: DashMap<Uuid, f64>,
    /// Compaction cost (tracked separately from user cost).
    compaction_cost: DashMap<Uuid, f64>,
}

impl CostTracker {
    pub fn new() -> Self {
        Self {
            agent_daily: DashMap::new(),
            session_totals: DashMap::new(),
            compaction_cost: DashMap::new(),
        }
    }

    /// Record a cost for an agent in a session.
    pub fn record(&self, agent_id: Uuid, session_id: Uuid, cost: f64, is_compaction: bool) {
        *self.agent_daily.entry(agent_id).or_insert(0.0) += cost;
        *self.session_totals.entry(session_id).or_insert(0.0) += cost;
        if is_compaction {
            *self.compaction_cost.entry(agent_id).or_insert(0.0) += cost;
        }
    }

    /// Get daily total for an agent.
    pub fn get_daily_total(&self, agent_id: Uuid) -> f64 {
        self.agent_daily.get(&agent_id).map(|v| *v).unwrap_or(0.0)
    }

    /// Get session total.
    pub fn get_session_total(&self, session_id: Uuid) -> f64 {
        self.session_totals
            .get(&session_id)
            .map(|v| *v)
            .unwrap_or(0.0)
    }

    /// Get compaction cost for an agent.
    pub fn get_compaction_cost(&self, agent_id: Uuid) -> f64 {
        self.compaction_cost
            .get(&agent_id)
            .map(|v| *v)
            .unwrap_or(0.0)
    }

    /// Reset daily totals (called at midnight).
    pub fn reset_daily(&self) {
        self.agent_daily.clear();
        self.compaction_cost.clear();
    }
}

impl Default for CostTracker {
    fn default() -> Self {
        Self::new()
    }
}
