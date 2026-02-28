//! # cortex-decay
//!
//! 5-factor multiplicative confidence decay engine.
//! Factors: temporal, citation freshness, usage frequency, importance anchor, pattern linkage.
//! Adaptive half-lives adjust based on access patterns.

pub mod adaptive;
pub mod archival;
pub mod engine;
pub mod factors;
pub mod formula;

pub use engine::DecayEngine;
pub use factors::DecayContext;
pub use formula::DecayBreakdown;
