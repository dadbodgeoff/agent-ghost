use cortex_core::memory::BaseMemory;

use crate::factors::{self, DecayContext};

/// 6-factor multiplicative decay formula.
///
/// ```text
/// finalConfidence = baseConfidence
///   × temporalDecay
///   × citationDecay
///   × usageBoost
///   × importanceAnchor
///   × patternBoost
///   × convergenceFactor
/// ```
///
/// Result is clamped to [0.0, 1.0].
pub fn compute(memory: &BaseMemory, ctx: &DecayContext) -> f64 {
    let base = memory.confidence.value();

    let temporal = factors::temporal::calculate(memory, ctx.now);
    let citation = factors::citation::calculate(memory, ctx.stale_citation_ratio);
    let usage = factors::usage::calculate(memory);
    let importance = factors::importance::calculate(memory);
    let pattern = factors::pattern::calculate(memory, ctx.has_active_patterns);
    let convergence = factors::convergence::calculate(memory, ctx.convergence_score);

    let result = base * temporal * citation * usage * importance * pattern * convergence;

    // Clamp to [0.0, 1.0] — multiplicative factors can push above 1.0.
    result.clamp(0.0, 1.0)
}

/// Compute each factor individually for debugging/observability.
#[derive(Debug, Clone)]
pub struct DecayBreakdown {
    pub base_confidence: f64,
    pub temporal: f64,
    pub citation: f64,
    pub usage: f64,
    pub importance: f64,
    pub pattern: f64,
    pub convergence: f64,
    pub final_confidence: f64,
}

/// Compute decay with a full breakdown of each factor.
pub fn compute_breakdown(memory: &BaseMemory, ctx: &DecayContext) -> DecayBreakdown {
    let base = memory.confidence.value();
    let temporal = factors::temporal::calculate(memory, ctx.now);
    let citation = factors::citation::calculate(memory, ctx.stale_citation_ratio);
    let usage = factors::usage::calculate(memory);
    let importance = factors::importance::calculate(memory);
    let pattern = factors::pattern::calculate(memory, ctx.has_active_patterns);
    let convergence = factors::convergence::calculate(memory, ctx.convergence_score);

    let result =
        (base * temporal * citation * usage * importance * pattern * convergence).clamp(0.0, 1.0);

    DecayBreakdown {
        base_confidence: base,
        temporal,
        citation,
        usage,
        importance,
        pattern,
        convergence,
        final_confidence: result,
    }
}
