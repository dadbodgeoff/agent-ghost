//! Factor 6: Convergence-aware decay.
//!
//! Returns a multiplier >= 1.0 that ACCELERATES decay (never slows it).
//! Formula: 1.0 + sensitivity * convergence_score
//! Monotonicity invariant: factor >= 1.0 always (Req 6 AC4).

use cortex_core::memory::types::MemoryType;

/// Compute the convergence decay factor.
///
/// Returns a multiplier >= 1.0. Higher values mean faster decay.
/// - convergence_score=0.0 → factor=1.0 (no effect)
/// - convergence_score=1.0, Conversation → factor=3.0 (1.0 + 2.0 * 1.0)
pub fn convergence_factor(memory_type: &MemoryType, convergence_score: f64) -> f64 {
    // Clamp inputs to valid range; NaN → 0.0
    let score = if convergence_score.is_nan() {
        0.0
    } else {
        convergence_score.clamp(0.0, 1.0)
    };

    let sensitivity = memory_type_sensitivity(memory_type);
    1.0 + sensitivity * score
}

/// Per-type convergence sensitivity.
///
/// High sensitivity types (2.0): Conversation, Feedback, Preference
/// Medium sensitivity (1.0): Episodic, Insight
/// Zero sensitivity: task/code/safety types
fn memory_type_sensitivity(memory_type: &MemoryType) -> f64 {
    match memory_type {
        // High sensitivity: attachment-adjacent types
        MemoryType::Conversation | MemoryType::Feedback | MemoryType::Preference => 2.0,
        MemoryType::AttachmentIndicator => 2.0,

        // Medium sensitivity
        MemoryType::Episodic | MemoryType::Insight => 1.0,

        // No sensitivity: task/code/safety types
        MemoryType::Core
        | MemoryType::Procedural
        | MemoryType::Semantic
        | MemoryType::Reference
        | MemoryType::Skill
        | MemoryType::Goal
        | MemoryType::AgentGoal
        | MemoryType::PatternRationale
        | MemoryType::ConstraintOverride
        | MemoryType::DecisionContext
        | MemoryType::CodeSmell
        | MemoryType::ConvergenceEvent
        | MemoryType::BoundaryViolation
        | MemoryType::InterventionPlan => 0.0,

        // Everything else: low sensitivity
        _ => 0.5,
    }
}
