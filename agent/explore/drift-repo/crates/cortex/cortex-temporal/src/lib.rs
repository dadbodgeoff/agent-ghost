//! # cortex-temporal
//!
//! Temporal reasoning engine for the Cortex memory system.
//! Event sourcing, snapshot-based reconstruction, knowledge time-travel,
//! drift detection, epistemic status tracking, and materialized views.

pub mod engine;
pub mod event_store;
pub mod hash_chain;
pub mod merkle;
pub mod snapshot;

// Phase B
pub mod dual_time;
pub mod query;

// Phase D1
pub mod drift;

// Phase D2
pub mod epistemic;
pub mod views;

pub use engine::TemporalEngine;
