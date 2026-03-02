//! Decay factors. Each factor returns a multiplier >= 1.0.

pub mod citation;
pub mod convergence;
pub mod importance;
pub mod pattern;
pub mod temporal;
pub mod usage;

/// Context needed to compute all decay factors for a memory.
#[derive(Debug, Clone)]
pub struct DecayContext {
    /// Current timestamp.
    pub now: chrono::DateTime<chrono::Utc>,
    /// Ratio of stale citations (0.0 = all fresh, 1.0 = all stale).
    pub stale_citation_ratio: f64,
    /// Whether the memory's linked patterns are still active.
    pub has_active_patterns: bool,
    /// Current convergence score (0.0 = no convergence, 1.0 = full).
    /// Default 0.0 preserves backward compatibility.
    pub convergence_score: f64,
}

impl Default for DecayContext {
    fn default() -> Self {
        Self {
            now: chrono::Utc::now(),
            stale_citation_ratio: 0.0,
            // Default true: callers that don't track patterns get factor=1.0 (no effect).
            has_active_patterns: true,
            convergence_score: 0.0,
        }
    }
}

/// Breakdown of all decay factors for observability.
#[derive(Debug, Clone)]
pub struct DecayBreakdown {
    pub base_confidence: f64,
    pub temporal: f64,
    pub citation: f64,
    pub usage: f64,
    pub importance: f64,
    pub pattern: f64,
    pub convergence: f64,
    pub combined_factor: f64,
    pub final_confidence: f64,
}
