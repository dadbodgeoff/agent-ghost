//! # ghost-mesh
//!
//! A2A-compatible agent network protocol with EigenTrust reputation,
//! cascade circuit breakers, and memory poisoning defense.
//!
//! GHOST agents can discover, delegate to, and collaborate with other
//! GHOST and A2A agents. All inter-agent communication is signed with
//! Ed25519 (via ghost-signing).

pub mod discovery;
pub mod error;
pub mod protocol;
pub mod safety;
pub mod traits;
pub mod transport;
pub mod trust;
pub mod types;
