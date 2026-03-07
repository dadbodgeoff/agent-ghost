//! Decay formula: multiplicative combination of all factors.

use crate::factors::{self, DecayBreakdown, DecayContext};
use cortex_core::memory::BaseMemory;

/// Compute the decayed confidence for a memory.
///
/// Applies all 6 decay factors multiplicatively. Each factor is >= 1.0,
/// so the combined factor is always >= 1.0, and dividing base confidence
/// by it always reduces (or maintains) the confidence.
pub fn compute(memory: &BaseMemory, ctx: &DecayContext) -> f64 {
    let base = memory.confidence;

    let f1 = factors::temporal::temporal_factor(&memory.memory_type, memory.created_at, ctx.now);
    let f2 = factors::citation::citation_factor(ctx.stale_citation_ratio);
    let f3 = factors::usage::usage_factor(memory.access_count, memory.last_accessed, ctx.now);
    let f4 = factors::importance::importance_factor(&memory.importance);
    let f5 = factors::pattern::pattern_factor(ctx.has_active_patterns);
    let f6 = factors::convergence::convergence_factor(&memory.memory_type, ctx.convergence_score);

    let combined = f1 * f2 * f3 * f4 * f5 * f6;
    (base / combined).clamp(0.0, 1.0)
}

/// Compute with full breakdown for observability.
pub fn compute_with_breakdown(memory: &BaseMemory, ctx: &DecayContext) -> DecayBreakdown {
    let base = memory.confidence;

    let temporal =
        factors::temporal::temporal_factor(&memory.memory_type, memory.created_at, ctx.now);
    let citation = factors::citation::citation_factor(ctx.stale_citation_ratio);
    let usage = factors::usage::usage_factor(memory.access_count, memory.last_accessed, ctx.now);
    let importance = factors::importance::importance_factor(&memory.importance);
    let pattern = factors::pattern::pattern_factor(ctx.has_active_patterns);
    let convergence =
        factors::convergence::convergence_factor(&memory.memory_type, ctx.convergence_score);

    let combined_factor = temporal * citation * usage * importance * pattern * convergence;
    let final_confidence = (base / combined_factor).clamp(0.0, 1.0);

    DecayBreakdown {
        base_confidence: base,
        temporal,
        citation,
        usage,
        importance,
        pattern,
        convergence,
        combined_factor,
        final_confidence,
    }
}
