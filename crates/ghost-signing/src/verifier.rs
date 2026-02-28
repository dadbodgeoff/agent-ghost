//! Ed25519 signature verification.

use ed25519_dalek::Verifier as _;

use crate::keypair::VerifyingKey;
use crate::signer::Signature;

/// Verify an Ed25519 signature against a message and public key.
///
/// Uses constant-time comparison internally (provided by `ed25519-dalek`).
/// Returns `false` for any malformed input — never panics.
pub fn verify(data: &[u8], sig: &Signature, key: &VerifyingKey) -> bool {
    key.inner().verify(data, sig.inner()).is_ok()
}
