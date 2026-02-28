//! # cortex-privacy
//!
//! Privacy patterns for the ConvergenceAwareFilter.
//! Detects emotional and attachment content for convergence-aware filtering.

pub mod emotional_patterns;

pub use emotional_patterns::{EmotionalContentDetector, EmotionalCategory};
