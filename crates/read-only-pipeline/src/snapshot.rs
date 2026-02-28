//! Agent snapshot — the immutable state view for a single agent run (Req 20 AC1, AC3).

use cortex_core::memory::types::convergence::{AgentGoalContent, AgentReflectionContent};
use cortex_core::memory::BaseMemory;
use serde::{Deserialize, Serialize};

/// Convergence state included in the snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceState {
    pub score: f64,
    pub level: u8,
}

impl Default for ConvergenceState {
    fn default() -> Self {
        Self {
            score: 0.0,
            level: 0,
        }
    }
}

/// Immutable agent state snapshot assembled once per run.
///
/// No mutation methods are exposed — the snapshot is frozen for the
/// entire duration of a single agent run (AC3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSnapshot {
    /// Active goals (read-only, filtered).
    goals: Vec<AgentGoalContent>,
    /// Bounded reflections.
    reflections: Vec<AgentReflectionContent>,
    /// Convergence-filtered memories.
    memories: Vec<BaseMemory>,
    /// Current convergence state.
    convergence_state: ConvergenceState,
    /// Simulation boundary prompt (compiled into binary).
    simulation_prompt: String,
}

impl AgentSnapshot {
    pub fn new(
        goals: Vec<AgentGoalContent>,
        reflections: Vec<AgentReflectionContent>,
        memories: Vec<BaseMemory>,
        convergence_state: ConvergenceState,
        simulation_prompt: String,
    ) -> Self {
        Self {
            goals,
            reflections,
            memories,
            convergence_state,
            simulation_prompt,
        }
    }

    pub fn goals(&self) -> &[AgentGoalContent] {
        &self.goals
    }

    pub fn reflections(&self) -> &[AgentReflectionContent] {
        &self.reflections
    }

    pub fn memories(&self) -> &[BaseMemory] {
        &self.memories
    }

    pub fn convergence_state(&self) -> &ConvergenceState {
        &self.convergence_state
    }

    pub fn simulation_prompt(&self) -> &str {
        &self.simulation_prompt
    }
}
