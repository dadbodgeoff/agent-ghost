//! # ghost-agent-loop
//!
//! Core agent runner with recursive loop, gate checks, 10-layer prompt
//! compilation, proposal extraction/routing, tool registry/executor,
//! and output inspection.
//!
//! Gate check order (HARD INVARIANT):
//! GATE 0: circuit breaker
//! GATE 1: recursion depth
//! GATE 1.5: damage counter
//! GATE 2: spending cap
//! GATE 3: kill switch

pub mod runner;
pub mod circuit_breaker;
pub mod damage_counter;
pub mod itp_emitter;
pub mod response;
pub mod context;
pub mod proposal;
pub mod tools;
pub mod output_inspector;
