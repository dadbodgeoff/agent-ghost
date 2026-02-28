//! Ed25519 signed deltas for CRDT operations (Req 29 AC1, AC3).

mod key_registry;
mod signed_delta;

pub use key_registry::KeyRegistry;
pub use signed_delta::{sign_delta, verify_delta, SignedDelta};
