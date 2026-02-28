//! Decay factors. Each factor returns a multiplier in (0.0, 1.0].

pub mod convergence;

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
            has_active_patterns: false,
            convergence_score: 0.0,
        }
    }
}

/// Breakdown of all decay factors for observability.
#[derive(Debug, Clone)]
pub struct DecayBreakdown {
    pub base_confidence: f64,
    pub convergence: f64,
    pub final_confidence: f64,
}
