//! RunContext — per-run immutable context (A21.1).

use std::time::{Duration, Instant};

use read_only_pipeline::snapshot::AgentSnapshot;
use uuid::Uuid;

/// Per-run context assembled once before the recursive loop.
///
/// The convergence snapshot is immutable for the entire run (A25.1 Hazard 1).
#[derive(Debug, Clone)]
pub struct RunContext {
    pub agent_id: Uuid,
    pub session_id: Uuid,
    pub session_started_at: Instant,
    pub recursion_depth: u32,
    pub max_recursion_depth: u32,
    pub total_tokens: usize,
    pub total_cost: f64,
    pub tool_call_count: u32,
    pub proposal_count: u32,
    /// Convergence snapshot — immutable for entire run.
    pub snapshot: AgentSnapshot,
    /// Intervention level — constant for entire run (A25.1 Hazard 1).
    pub intervention_level: u8,
    /// Circuit breaker state at run start.
    pub cb_failures: u32,
    /// Damage counter at run start.
    pub damage_count: u32,
    /// Spending cap for this agent.
    pub spending_cap: f64,
    /// Current daily spend.
    pub daily_spend: f64,
    /// Whether kill switch is active.
    pub kill_switch_active: bool,
    /// Model context window size.
    pub context_window: usize,
}

impl RunContext {
    /// Elapsed duration for the current live run.
    pub fn session_duration(&self) -> Duration {
        self.session_started_at.elapsed()
    }

    /// Check if spending cap would be exceeded by an estimated cost.
    pub fn would_exceed_cap(&self, estimated_cost: f64) -> bool {
        (self.daily_spend + self.total_cost + estimated_cost) > self.spending_cap
    }

    /// Check if recursion depth is exceeded.
    pub fn is_recursion_exceeded(&self) -> bool {
        self.recursion_depth >= self.max_recursion_depth
    }
}
