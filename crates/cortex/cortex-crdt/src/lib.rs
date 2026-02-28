//! # cortex-crdt
//!
//! CRDT primitives with Ed25519 signed deltas and sybil resistance for Cortex.
//!
//! **Architectural constraint**: This crate uses `ed25519-dalek` DIRECTLY,
//! NOT `ghost-signing`. `cortex-crdt` is Layer 1; `ghost-signing` is Layer 0
//! but the wrapper types differ. The signing primitives are identical (both
//! ed25519-dalek) but the wrappers differ: cortex-crdt wraps
//! `MemoryDelta → SignedDelta`, ghost-gateway wraps `AgentMessage → signed AgentMessage`.

pub mod signing;
pub mod sybil;
