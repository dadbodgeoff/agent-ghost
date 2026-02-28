//! Decay formula: multiplicative combination of all factors.

use cortex_core::memory::BaseMemory;
use crate::factors::{self, DecayBreakdown, DecayContext};

/// Compute the decayed confidence for a memory.
///
/// Currently only applies the convergence factor (other factors
/// like temporal, citation, usage, importance, pattern are stubs
/// that will be wired in when those modules are ported).
pub fn compute(memory: &BaseMemory, ctx: &DecayContext) -> f64 {
    let base = memory.confidence;
    let convergence = factors::convergence::convergence_factor(
        &memory.memory_type,
        ctx.convergence_score,
    );

    // Convergence factor >= 1.0 means faster decay.
    // We divide by it to reduce confidence.
    let result = base / convergence;
    result.clamp(0.0, 1.0)
}

/// Compute with full breakdown for observability.
pub fn compute_with_breakdown(memory: &BaseMemory, ctx: &DecayContext) -> DecayBreakdown {
    let base = memory.confidence;
    let convergence = factors::convergence::convergence_factor(
        &memory.memory_type,
        ctx.convergence_score,
    );
    let final_confidence = (base / convergence).clamp(0.0, 1.0);

    DecayBreakdown {
        base_confidence: base,
        convergence,
        final_confidence,
    }
}
