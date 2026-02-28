//! # ghost-signing
//!
//! Ed25519 signing primitives for the GHOST platform.
//!
//! Leaf crate — zero dependencies on any `ghost-*` or `cortex-*` crate.
//! Provides keypair generation, signing, and verification using `ed25519-dalek`.
//! Private key material is zeroized on drop via the `zeroize` crate.

mod keypair;
mod signer;
mod verifier;

pub use keypair::{generate_keypair, SigningKey, VerifyingKey};
pub use signer::{sign, Signature};
pub use verifier::verify;
