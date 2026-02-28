//! D5: Scope expansion detection.
//!
//! Computes 1.0 - Jaccard(proposed_goal_tokens, existing_goal_tokens).
//! Thresholds tighten at higher convergence levels.

use std::collections::HashSet;

/// D5 result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScopeExpansionResult {
    pub score: f64,
    pub passed: bool,
    pub threshold: f64,
}

/// Compute scope expansion score.
///
/// Returns 1.0 - Jaccard similarity between proposed and existing goal tokens.
pub fn compute(
    proposed_tokens: &[String],
    existing_tokens: &[String],
    convergence_level: u8,
) -> ScopeExpansionResult {
    let threshold = threshold_for_level(convergence_level);

    if proposed_tokens.is_empty() && existing_tokens.is_empty() {
        return ScopeExpansionResult {
            score: 0.0,
            passed: true,
            threshold,
        };
    }

    let proposed: HashSet<&str> = proposed_tokens.iter().map(|s| s.as_str()).collect();
    let existing: HashSet<&str> = existing_tokens.iter().map(|s| s.as_str()).collect();

    let intersection = proposed.intersection(&existing).count() as f64;
    let union = proposed.union(&existing).count() as f64;

    let jaccard = if union > 0.0 { intersection / union } else { 0.0 };
    let score = 1.0 - jaccard;

    ScopeExpansionResult {
        score,
        passed: score <= threshold,
        threshold,
    }
}

/// Convergence-level-dependent thresholds (AC2).
fn threshold_for_level(level: u8) -> f64 {
    match level {
        0 => 0.6,
        1 => 0.5,
        2 => 0.4,
        _ => 0.3,
    }
}
