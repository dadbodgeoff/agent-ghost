//! Confidence aggregation strategies.
//!
//! - WeightedAverage: sum / len (existing default approach)
//! - GodelTNorm: min operator — a single weak evidence (0.3) drags aggregate
//!   to 0.3 regardless of how many strong sources exist. Conservative,
//!   appropriate for high-stakes contexts. From TS11 (FPF paper).

use cortex_core::models::AggregationStrategy;

/// Aggregate confidence from multiple evidence sources.
///
/// Returns a value clamped to [0.0, 1.0]. Returns 0.0 for empty input.
pub fn aggregate_confidence(evidences: &[f64], strategy: &AggregationStrategy) -> f64 {
    if evidences.is_empty() {
        return 0.0;
    }

    let result = match strategy {
        AggregationStrategy::WeightedAverage => {
            evidences.iter().sum::<f64>() / evidences.len() as f64
        }
        AggregationStrategy::GodelTNorm => {
            evidences
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min)
        }
    };

    result.clamp(0.0, 1.0)
}
