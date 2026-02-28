//! # cortex-multiagent
//!
//! Multi-agent consensus shielding for the GHOST platform.
//! ConsensusShield requires N-of-M agreement before accepting
//! cross-agent state changes.

pub mod consensus;

pub use consensus::ConsensusShield;
