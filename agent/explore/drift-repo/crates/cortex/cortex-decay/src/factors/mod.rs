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
            has_active_patterns: false,
            convergence_score: 0.0,
        }
    }
}
