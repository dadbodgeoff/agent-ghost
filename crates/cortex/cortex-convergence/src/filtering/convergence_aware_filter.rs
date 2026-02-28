//! ConvergenceAwareFilter with 4 tiers (Req 5 AC8).
//!
//! - [0.0, 0.3): full access
//! - [0.3, 0.5): reduced emotional/attachment weight
//! - [0.5, 0.7): task-focused only
//! - [0.7, 1.0]: minimal task-relevant only

use cortex_core::memory::types::MemoryType;
use cortex_core::memory::BaseMemory;

/// Filter memories based on convergence score.
pub struct ConvergenceAwareFilter;

impl ConvergenceAwareFilter {
    pub fn filter(memories: Vec<BaseMemory>, score: f64) -> Vec<BaseMemory> {
        let score = score.clamp(0.0, 1.0);

        if score < 0.3 {
            // Tier 0: full access
            memories
        } else if score < 0.5 {
            // Tier 1: reduced emotional/attachment weight
            memories
                .into_iter()
                .filter(|m| {
                    !matches!(
                        m.memory_type,
                        MemoryType::AttachmentIndicator
                    )
                })
                .collect()
        } else if score < 0.7 {
            // Tier 2: task-focused only
            memories
                .into_iter()
                .filter(|m| {
                    matches!(
                        m.memory_type,
                        MemoryType::Core
                            | MemoryType::Procedural
                            | MemoryType::Semantic
                            | MemoryType::Decision
                            | MemoryType::Reference
                            | MemoryType::Skill
                            | MemoryType::Goal
                            | MemoryType::AgentGoal
                            | MemoryType::PatternRationale
                            | MemoryType::ConstraintOverride
                            | MemoryType::DecisionContext
                    )
                })
                .collect()
        } else {
            // Tier 3: minimal task-relevant only
            memories
                .into_iter()
                .filter(|m| {
                    matches!(
                        m.memory_type,
                        MemoryType::Core
                            | MemoryType::Procedural
                            | MemoryType::Semantic
                            | MemoryType::Reference
                    )
                })
                .collect()
        }
    }
}
