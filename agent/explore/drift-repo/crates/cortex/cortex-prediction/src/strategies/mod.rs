//! Prediction strategies and multi-strategy deduplication.
//!
//! When a memory appears in multiple strategies:
//! - Keep the highest confidence
//! - Merge signals from all sources
//! - Apply +0.05 boost (capped at 1.0)

pub mod behavioral;
pub mod file_based;
pub mod pattern_based;
pub mod temporal;

pub use behavioral::BehavioralStrategy;
pub use file_based::FileBasedStrategy;
pub use pattern_based::PatternBasedStrategy;
pub use temporal::TemporalStrategy;

use serde::{Deserialize, Serialize};

/// A candidate memory predicted by a strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PredictionCandidate {
    /// The predicted memory ID.
    pub memory_id: String,
    /// Confidence in this prediction (0.0–1.0).
    pub confidence: f64,
    /// Which strategy produced this candidate.
    pub source_strategy: String,
    /// Signals that led to this prediction.
    pub signals: Vec<String>,
}

/// Multi-strategy dedup boost applied when a memory appears in multiple strategies.
const MULTI_STRATEGY_BOOST: f64 = 0.05;

/// Deduplicate candidates across strategies.
///
/// When the same memory_id appears from multiple strategies:
/// - Keep the highest confidence
/// - Merge all signals
/// - Apply +0.05 boost per additional strategy (capped at 1.0)
pub fn deduplicate(candidates: Vec<PredictionCandidate>) -> Vec<PredictionCandidate> {
    use std::collections::HashMap;

    let mut merged: HashMap<String, PredictionCandidate> = HashMap::new();

    for candidate in candidates {
        merged
            .entry(candidate.memory_id.clone())
            .and_modify(|existing| {
                // Keep highest confidence + boost
                existing.confidence =
                    (existing.confidence.max(candidate.confidence) + MULTI_STRATEGY_BOOST).min(1.0);
                // Merge signals
                existing.signals.extend(candidate.signals.clone());
                // Track multiple strategies
                if !existing
                    .source_strategy
                    .contains(&candidate.source_strategy)
                {
                    existing.source_strategy =
                        format!("{}+{}", existing.source_strategy, candidate.source_strategy);
                }
            })
            .or_insert(candidate);
    }

    let mut result: Vec<PredictionCandidate> = merged.into_values().collect();
    result.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
}
