//! Optional message encryption: X25519-XSalsa20-Poly1305 (Req 19 AC8).
//! Encrypt-then-sign pattern. Broadcast messages cannot be encrypted.

/// Encryption is optional and feature-gated in production.
/// This module provides the interface stubs.

/// Check if a message can be encrypted (broadcast messages cannot).
pub fn can_encrypt(is_broadcast: bool) -> bool {
    !is_broadcast
}
