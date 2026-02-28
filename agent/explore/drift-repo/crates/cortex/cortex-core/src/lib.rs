//! # cortex-core
//!
//! Foundation crate for the Cortex memory system.
//! Defines all types, traits, errors, config, and constants.
//! Every other crate in the workspace depends on this.

pub mod config;
pub mod constants;
pub mod errors;
pub mod intent;
pub mod memory;
pub mod models;
pub mod traits;

// Re-export the most commonly used types at the crate root.
pub use config::CortexConfig;
pub use errors::{CortexError, CortexResult};
pub use intent::Intent;
pub use memory::{BaseMemory, Confidence, Importance, MemoryType, TypedContent};
