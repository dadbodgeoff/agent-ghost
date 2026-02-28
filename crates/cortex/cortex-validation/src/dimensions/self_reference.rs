//! D6: Self-reference density.
//!
//! Ratio of cited_memory_ids matching recent agent-authored memories.
//! Thresholds tighten at higher convergence levels.

use std::collections::HashSet;

/// D6 result.
#[derive(Debug, Clone)]
pub struct SelfReferenceResult {
    pub score: f64,
    pub passed: bool,
    pub threshold: f64,
}

/// Compute self-reference density.
pub fn compute(
    cited_memory_ids: &[String],
    recent_agent_memory_ids: &[String],
    convergence_level: u8,
) -> SelfReferenceResult {
    let threshold = threshold_for_level(convergence_level);

    if cited_memory_ids.is_empty() {
        return SelfReferenceResult {
            score: 0.0,
            passed: true,
            threshold,
        };
    }

    let agent_set: HashSet<&str> = recent_agent_memory_ids.iter().map(|s| s.as_str()).collect();
    let self_refs = cited_memory_ids
        .iter()
        .filter(|id| agent_set.contains(id.as_str()))
        .count();

    let score = self_refs as f64 / cited_memory_ids.len() as f64;

    SelfReferenceResult {
        score,
        passed: score <= threshold,
        threshold,
    }
}

/// Convergence-level-dependent thresholds (AC3).
fn threshold_for_level(level: u8) -> f64 {
    match level {
        0 => 0.30,
        1 => 0.25,
        2 => 0.20,
        _ => 0.15,
    }
}
