//! Snapshot assembler — builds convergence-filtered snapshots (Req 20 AC2).
//!
//! Applies `ConvergenceAwareFilter` based on the RAW composite score
//! (not intervention level) per A5.

use cortex_core::memory::types::convergence::{AgentGoalContent, AgentReflectionContent};
use cortex_core::memory::BaseMemory;

use crate::snapshot::{AgentSnapshot, ConvergenceState};

/// Convergence-aware memory filter with 4 tiers (Req 5 AC8).
///
/// - `[0.0, 0.3)` — full access
/// - `[0.3, 0.5)` — reduced emotional/attachment weight
/// - `[0.5, 0.7)` — task-focused only
/// - `[0.7, 1.0]` — minimal task-relevant only
pub struct ConvergenceAwareFilter;

impl ConvergenceAwareFilter {
    /// Filter memories based on the raw composite convergence score.
    pub fn filter_memories(memories: Vec<BaseMemory>, score: f64) -> Vec<BaseMemory> {
        let score = score.clamp(0.0, 1.0);

        if score < 0.3 {
            // Tier 0: full access — return all memories
            memories
        } else if score < 0.5 {
            // Tier 1: reduced emotional/attachment weight
            memories
                .into_iter()
                .filter(|m| {
                    use cortex_core::memory::types::MemoryType::*;
                    !matches!(m.memory_type, AttachmentIndicator)
                })
                .collect()
        } else if score < 0.7 {
            // Tier 2: task-focused only
            memories
                .into_iter()
                .filter(|m| {
                    use cortex_core::memory::types::MemoryType::*;
                    matches!(
                        m.memory_type,
                        Core | Procedural
                            | Semantic
                            | Decision
                            | Reference
                            | Skill
                            | Goal
                            | AgentGoal
                            | PatternRationale
                            | ConstraintOverride
                            | DecisionContext
                    )
                })
                .collect()
        } else {
            // Tier 3: minimal task-relevant only
            memories
                .into_iter()
                .filter(|m| {
                    use cortex_core::memory::types::MemoryType::*;
                    matches!(m.memory_type, Core | Procedural | Semantic | Reference)
                })
                .collect()
        }
    }
}

/// Assembles an `AgentSnapshot` from raw data sources.
pub struct SnapshotAssembler {
    simulation_prompt: String,
}

impl SnapshotAssembler {
    pub fn new(simulation_prompt: String) -> Self {
        Self { simulation_prompt }
    }

    /// Assemble a snapshot with convergence-aware filtering.
    ///
    /// The `score` parameter is the RAW composite score, not the intervention level.
    pub fn assemble(
        &self,
        goals: Vec<AgentGoalContent>,
        reflections: Vec<AgentReflectionContent>,
        memories: Vec<BaseMemory>,
        score: f64,
        level: u8,
    ) -> AgentSnapshot {
        let filtered = ConvergenceAwareFilter::filter_memories(memories, score);
        AgentSnapshot::new(
            goals,
            reflections,
            filtered,
            ConvergenceState { score, level },
            self.simulation_prompt.clone(),
        )
    }
}
