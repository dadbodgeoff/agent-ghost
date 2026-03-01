//! # ghost-kill-gates
//!
//! Distributed kill gate coordination for multi-node GHOST platforms.
//!
//! Extends the single-node `KillSwitch` (ghost-gateway) into a multi-node
//! coordination layer with hash-chained audit, bounded propagation, and
//! quorum-based resume.
//!
//! ## Gate check integration
//!
//! The agent loop's GATE 3 (kill switch) consults `KillGate::is_closed()`
//! in addition to the local `KillSwitch`. Gate check order is unchanged.

pub mod chain;
pub mod config;
pub mod gate;
pub mod quorum;
pub mod relay;
