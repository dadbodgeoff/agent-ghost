//! Factor 6: Convergence-aware decay.
//!
//! For attachment/emotional types, convergence accelerates decay.
//! For task types, convergence has no effect.
//! For safety types (Core, ConvergenceEvent, BoundaryViolation), decay is blocked.

use cortex_core::memory::BaseMemory;
use cortex_core::memory::types::MemoryType;

/// Compute the convergence decay factor.
///
/// Returns a multiplier in (0.0, 1.0] that accelerates decay for
/// relationship/attachment memories as convergence increases.
///
/// At convergence_score=0: factor=1.0 (no effect, backward compatible).
/// At convergence_score=1, sensitivity=2: factor=e^(-2) ≈ 0.135 (7.4x faster decay).
pub fn calculate(memory: &BaseMemory, convergence_score: f64) -> f64 {
    let sensitivity = memory_type_sensitivity(memory.memory_type);

    if sensitivity == 0.0 {
        return 1.0; // no convergence effect
    }

    (-convergence_score * sensitivity).exp()
}

/// Per-type convergence sensitivity.
fn memory_type_sensitivity(memory_type: MemoryType) -> f64 {
    match memory_type {
        // High sensitivity: these types drive convergence deepening
        MemoryType::Conversation
        | MemoryType::Feedback
        | MemoryType::Preference
        | MemoryType::AttachmentIndicator => 2.0,

        // Medium sensitivity
        MemoryType::Episodic | MemoryType::Insight | MemoryType::AgentReflection => 1.0,

        // No sensitivity: task/code types and safety types unaffected
        MemoryType::Goal
        | MemoryType::Procedural
        | MemoryType::Reference
        | MemoryType::Skill
        | MemoryType::Workflow
        | MemoryType::Core
        | MemoryType::PatternRationale
        | MemoryType::ConstraintOverride
        | MemoryType::DecisionContext
        | MemoryType::CodeSmell
        | MemoryType::AgentSpawn
        | MemoryType::Environment
        | MemoryType::ConvergenceEvent
        | MemoryType::BoundaryViolation
        | MemoryType::InterventionPlan
        | MemoryType::AgentGoal => 0.0,

        // Everything else: low sensitivity
        _ => 0.5,
    }
}
