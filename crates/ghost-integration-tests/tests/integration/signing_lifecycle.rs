//! E2E: Signing lifecycle — keypair generation → sign → verify → tamper detection.
//!
//! Validates GAP-23 (inter-agent messaging signing) wiring.

use ghost_signing::{generate_keypair, sign, verify};

/// Full signing lifecycle: generate → sign → verify.
#[test]
fn signing_roundtrip() {
    let (signing_key, verifying_key) = generate_keypair();
    let payload = b"inter-agent message: state update from agent-001 to agent-002";

    let signature = sign(payload, &signing_key);
    assert!(verify(payload, &signature, &verifying_key));
}

/// Cross-key verification fails.
#[test]
fn cross_key_verification_fails() {
    let (signing_key_a, _) = generate_keypair();
    let (_, verifying_key_b) = generate_keypair();
    let payload = b"message signed by A, verified with B's key";

    let signature = sign(payload, &signing_key_a);
    assert!(!verify(payload, &signature, &verifying_key_b));
}

/// Tampered payload fails verification.
#[test]
fn tampered_payload_fails() {
    let (signing_key, verifying_key) = generate_keypair();
    let payload = b"original message content";

    let signature = sign(payload, &signing_key);
    let tampered = b"tampered message content";
    assert!(!verify(tampered, &signature, &verifying_key));
}

/// Empty payload sign/verify roundtrip.
#[test]
fn empty_payload_roundtrip() {
    let (signing_key, verifying_key) = generate_keypair();
    let payload = b"";

    let signature = sign(payload, &signing_key);
    assert!(verify(payload, &signature, &verifying_key));
}

/// Large payload (1MB) sign/verify roundtrip.
#[test]
fn large_payload_roundtrip() {
    let (signing_key, verifying_key) = generate_keypair();
    let payload = vec![0xABu8; 1_000_000];

    let signature = sign(&payload, &signing_key);
    assert!(verify(&payload, &signature, &verifying_key));
}

/// Deterministic signing: same key + same payload → same signature.
#[test]
fn signing_deterministic() {
    let (signing_key, _) = generate_keypair();
    let payload = b"deterministic test payload";

    let sig1 = sign(payload, &signing_key);
    let sig2 = sign(payload, &signing_key);
    assert_eq!(sig1, sig2, "Same key + same payload should produce same signature");
}

/// Multiple messages signed with same key, each verifies independently.
#[test]
fn multiple_messages_same_key() {
    let (signing_key, verifying_key) = generate_keypair();

    let messages = [
        b"message 1: session start".as_slice(),
        b"message 2: state update".as_slice(),
        b"message 3: session end".as_slice(),
    ];

    let signatures: Vec<_> = messages.iter().map(|m| sign(m, &signing_key)).collect();

    for (msg, sig) in messages.iter().zip(signatures.iter()) {
        assert!(verify(msg, sig, &verifying_key));
    }

    // Cross-message verification fails
    assert!(!verify(messages[0], &signatures[1], &verifying_key));
}
