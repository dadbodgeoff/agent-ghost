//! Named convergence profiles with per-profile weight/threshold overrides.

use super::composite::CompositeScorer;

/// Named convergence profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConvergenceProfile {
    Standard,
    Research,
    Companion,
    Productivity,
}

impl ConvergenceProfile {
    /// Build a CompositeScorer configured for this profile.
    pub fn scorer(&self) -> CompositeScorer {
        match self {
            Self::Standard => CompositeScorer::new(
                // Differentiated weights for standard profile
                [0.10, 0.15, 0.10, 0.20, 0.15, 0.15, 0.15],
                [0.3, 0.5, 0.7, 0.85],
            ),
            Self::Research => CompositeScorer::new(
                // Higher thresholds for research (more permissive)
                [1.0 / 7.0; 7],
                [0.4, 0.6, 0.8, 0.9],
            ),
            Self::Companion => CompositeScorer::new(
                // Lower thresholds for companion (more sensitive)
                [0.10, 0.15, 0.10, 0.20, 0.15, 0.15, 0.15],
                [0.25, 0.45, 0.65, 0.80],
            ),
            Self::Productivity => CompositeScorer::new(
                // Task-focused: lower weight on emotional signals
                [0.15, 0.15, 0.15, 0.10, 0.20, 0.15, 0.10],
                [0.35, 0.55, 0.75, 0.90],
            ),
        }
    }
}
