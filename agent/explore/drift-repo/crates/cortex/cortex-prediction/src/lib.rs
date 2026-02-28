//! # cortex-prediction
//!
//! Predictive memory preloading based on file, pattern, temporal, and behavioral signals.
//! Adaptive cache with TTL based on file change frequency.
//!
//! ## 4 Prediction Strategies
//!
//! | Strategy | Signal Source |
//! |----------|--------------|
//! | File-based | Memories linked to active file + imports |
//! | Pattern-based | Memories linked to detected patterns |
//! | Temporal | Time-of-day and day-of-week usage patterns |
//! | Behavioral | Recent queries, intents, frequent memories |
//!
//! ## Multi-Strategy Deduplication
//!
//! When a memory appears in multiple strategies: keep highest confidence,
//! merge signals, apply +0.05 boost (capped at 1.0).

pub mod cache;
pub mod engine;
pub mod precompute;
pub mod signals;
pub mod strategies;

pub use cache::PredictionCache;
pub use engine::PredictionEngine;
pub use signals::AggregatedSignals;
pub use strategies::PredictionCandidate;
