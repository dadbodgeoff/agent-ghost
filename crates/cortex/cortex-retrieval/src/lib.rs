//! # cortex-retrieval
//!
//! Memory retrieval with convergence-aware scoring.
//! Adds `convergence_score` as the 11th scoring factor in ScorerWeights.

pub mod scorer;

pub use scorer::{RetrievalScorer, ScorerWeights};
