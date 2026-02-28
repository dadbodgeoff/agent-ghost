//! # cortex-core
//!
//! Core types, traits, configuration, and error definitions for the Cortex
//! memory system. This crate is the single source of truth for shared data
//! structures consumed by all downstream `cortex-*` and `ghost-*` crates.
//!
//! Layer 1A in the GHOST dependency hierarchy.

pub mod config;
pub mod memory;
pub mod models;
pub mod safety;
pub mod traits;
