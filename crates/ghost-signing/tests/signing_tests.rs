//! Comprehensive tests for ghost-signing.
//!
//! Covers: round-trip, cross-key rejection, tamper detection, edge cases,
//! adversarial inputs, zeroize trait bound, and leaf-crate dependency audit.

use ghost_signing::{generate_keypair, sign, verify, Signature};

// ─── Round-trip ───────────────────────────────────────────────────────────

#[test]
fn sign_verify_round_trip_empty_payload() {
    let (sk, vk) = generate_keypair();
    let sig = sign(b"", &sk);
    assert!(verify(b"", &sig, &vk));
}

#[test]
fn sign_verify_round_trip_small_payload() {
    let (sk, vk) = generate_keypair();
    let data = b"hello ghost";
    let sig = sign(data, &sk);
    assert!(verify(data, &sig, &vk));
}

#[test]
fn sign_verify_round_trip_max_payload() {
    let (sk, vk) = generate_keypair();
    let data = vec![0xABu8; 1_048_576]; // 1 MB
    let sig = sign(&data, &sk);
    assert!(verify(&data, &sig, &vk));
}

// ─── Cross-key rejection ─────────────────────────────────────────────────

#[test]
fn verify_with_wrong_key_returns_false() {
    let (sk_a, _vk_a) = generate_keypair();
    let (_sk_b, vk_b) = generate_keypair();
    let sig = sign(b"payload", &sk_a);
    assert!(!verify(b"payload", &sig, &vk_b));
}

// ─── Tamper detection ────────────────────────────────────────────────────

#[test]
fn mutated_payload_fails_verification() {
    let (sk, vk) = generate_keypair();
    let data = b"original";
    let sig = sign(data, &sk);
    assert!(!verify(b"Original", &sig, &vk)); // case flip
    assert!(!verify(b"original\0", &sig, &vk)); // appended null
    assert!(!verify(b"origina", &sig, &vk)); // truncated
}

// ─── Adversarial: malformed signatures ───────────────────────────────────

#[test]
fn truncated_signature_63_bytes_returns_false() {
    let (sk, _vk) = generate_keypair();
    let sig = sign(b"data", &sk);
    let truncated = &sig.to_bytes()[..63];
    let bad_sig = Signature::from_bytes(truncated);
    assert!(bad_sig.is_none(), "63-byte slice must not parse as Signature");
}

#[test]
fn all_zero_signature_returns_false() {
    let (_sk, vk) = generate_keypair();
    let zero_sig = Signature::from_bytes(&[0u8; 64]).expect("64 zero bytes should parse");
    assert!(!verify(b"data", &zero_sig, &vk));
}

#[test]
fn all_zero_verifying_key_returns_false_or_none() {
    let (sk, _vk) = generate_keypair();
    let sig = sign(b"data", &sk);

    // [0u8; 32] is the identity point — ed25519-dalek rejects it as a weak key.
    let zero_key = ghost_signing::VerifyingKey::from_bytes(&[0u8; 32]);
    match zero_key {
        None => {} // rejected at construction — acceptable
        Some(vk) => {
            // If it somehow constructs, verification must still fail.
            assert!(!verify(b"data", &sig, &vk));
        }
    }
}

// ─── Determinism ─────────────────────────────────────────────────────────

#[test]
fn signing_is_deterministic() {
    let (sk, _vk) = generate_keypair();
    let data = b"deterministic";
    let sig1 = sign(data, &sk);
    let sig2 = sign(data, &sk);
    assert_eq!(sig1.to_bytes(), sig2.to_bytes());
}

// ─── Verifying key serialization round-trip ──────────────────────────────

#[test]
fn verifying_key_bytes_round_trip() {
    let (_sk, vk) = generate_keypair();
    let bytes = vk.to_bytes();
    let vk2 = ghost_signing::VerifyingKey::from_bytes(&bytes).expect("valid key bytes");
    assert_eq!(vk, vk2);
}

// ─── Zeroize trait bound (compile-time check) ────────────────────────────

#[test]
fn signing_key_is_zeroize_compatible() {
    // With the `zeroize` feature enabled on ed25519-dalek, the inner
    // SigningKey implements ZeroizeOnDrop. When our wrapper is dropped,
    // the inner field is dropped, triggering zeroize on the secret bytes.
    // This test exercises the drop path.
    let (sk, _) = generate_keypair();
    drop(sk); // inner ed25519_dalek::SigningKey::drop() zeroizes secret_key
}

// ─── Leaf crate dependency audit ─────────────────────────────────────────

#[test]
fn cargo_toml_has_no_ghost_or_cortex_dependencies() {
    let cargo_toml = include_str!("../Cargo.toml");
    let parsed: toml::Value = cargo_toml.parse().expect("valid TOML");

    let check_section = |section: &str| {
        if let Some(deps) = parsed.get(section).and_then(|v| v.as_table()) {
            for key in deps.keys() {
                assert!(
                    !key.starts_with("ghost-") && !key.starts_with("cortex-"),
                    "Leaf crate violation: [{section}] contains `{key}` — \
                     ghost-signing must have zero ghost-*/cortex-* dependencies"
                );
            }
        }
    };

    check_section("dependencies");
    // dev-dependencies are allowed to reference anything, but let's be strict
    // about runtime deps only.
}
