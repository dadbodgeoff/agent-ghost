//! # read-only-pipeline
//!
//! Assembles immutable, convergence-filtered agent state snapshots consumed
//! by the prompt compiler at Layer L6. The snapshot is built once per agent
//! run and never mutated during the run.
//!
//! Filtering uses the RAW composite score (not intervention level) per A5.

pub mod assembler;
pub mod formatter;
pub mod snapshot;
