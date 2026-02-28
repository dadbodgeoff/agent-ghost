use cortex_core::errors::CortexResult;
use cortex_core::memory::BaseMemory;
use cortex_core::traits::IDecayEngine;

use crate::archival::{self, ArchivalDecision};
use crate::factors::DecayContext;
use crate::formula;

/// Decay engine implementing the 5-factor multiplicative decay formula
/// with adaptive half-lives and archival triggers.
pub struct DecayEngine {
    /// Archival threshold (default 0.15).
    archival_threshold: f64,
}

impl DecayEngine {
    /// Create a new DecayEngine with the default archival threshold.
    pub fn new() -> Self {
        Self {
            archival_threshold: archival::DEFAULT_ARCHIVAL_THRESHOLD,
        }
    }

    /// Create with a custom archival threshold.
    pub fn with_threshold(threshold: f64) -> Self {
        Self {
            archival_threshold: threshold,
        }
    }

    /// Get the archival threshold.
    pub fn archival_threshold(&self) -> f64 {
        self.archival_threshold
    }

    /// Calculate decay with full context (stale citations, active patterns).
    pub fn calculate_with_context(
        &self,
        memory: &BaseMemory,
        ctx: &DecayContext,
    ) -> CortexResult<f64> {
        Ok(formula::compute(memory, ctx))
    }

    /// Calculate decay with a full breakdown of each factor.
    pub fn calculate_breakdown(
        &self,
        memory: &BaseMemory,
        ctx: &DecayContext,
    ) -> formula::DecayBreakdown {
        formula::compute_breakdown(memory, ctx)
    }

    /// Evaluate archival eligibility after decay.
    pub fn evaluate_archival(
        &self,
        memory: &BaseMemory,
        decayed_confidence: f64,
    ) -> ArchivalDecision {
        archival::evaluate(memory, decayed_confidence, self.archival_threshold)
    }

    /// Process a batch of memories: compute decay and evaluate archival for each.
    pub fn process_batch(
        &self,
        memories: &[BaseMemory],
        ctx: &DecayContext,
    ) -> Vec<(f64, ArchivalDecision)> {
        memories
            .iter()
            .map(|m| {
                let decayed = formula::compute(m, ctx);
                let decision = archival::evaluate(m, decayed, self.archival_threshold);
                (decayed, decision)
            })
            .collect()
    }
}

impl Default for DecayEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl IDecayEngine for DecayEngine {
    fn calculate(&self, memory: &BaseMemory) -> CortexResult<f64> {
        // Default context: current time, no stale citations, no active patterns.
        let ctx = DecayContext::default();
        self.calculate_with_context(memory, &ctx)
    }
}
