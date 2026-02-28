//! # cortex-learning
//!
//! Correction analysis pipeline: diff analysis → categorization → principle extraction → dedup → memory creation.
//! Active learning loop selects uncertain memories for user validation.

pub mod active_learning;
pub mod analysis;
pub mod calibration;
pub mod deduplication;
pub mod engine;
pub mod extraction;

pub use analysis::{CorrectionCategory, DiffAnalysis};
pub use engine::LearningEngine;
