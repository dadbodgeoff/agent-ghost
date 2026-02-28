//! Ed25519 signing and verification for CRDT deltas.
//! Wraps the stateless MergeEngine — signatures are verified BEFORE
//! deltas reach MergeEngine::apply_delta().

pub mod key_registry;
pub mod signed_delta;
pub mod verifier;

pub use key_registry::KeyRegistry;
pub use signed_delta::SignedDelta;
pub use verifier::SignedDeltaVerifier;
