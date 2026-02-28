//! # cortex-decay
//!
//! Memory decay engine with 6-factor multiplicative formula.
//! Factor 6 (convergence) accelerates decay for attachment-adjacent memories.

pub mod factors;
pub mod formula;
