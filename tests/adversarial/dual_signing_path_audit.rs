//! Adversarial stress tests: Dual signing path audit.
//!
//! cortex-crdt uses ed25519-dalek directly; ghost-signing wraps the same
//! primitive with different semantics. These tests verify the two paths
//! cannot be accidentally conflated and that each path rejects the other's
//! artifacts.

use cortex_crdt::signing::{sign_delta, verify_delta, KeyRegistry};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestDelta {
    key: String,
    value: i64,
}

// ── Layer separation: compile-time guarantee ────────────────────────────

#[test]
fn cortex_crdt_does_not_depend_on_ghost_signing() {
    let cargo = include_str!("../../crates/cortex/cortex-crdt/Cargo.toml");
    assert!(
        !cargo.contains("ghost-signing"),
        "cortex-crdt MUST NOT depend on ghost-signing — layer separation violated"
    );
}

#[test]
fn ghost_signing_does_not_depend_on_cortex_crdt() {
    let cargo = include_str!("../../crates/ghost-signing/Cargo.toml");
    assert!(
        !cargo.contains("cortex-crdt"),
        "ghost-signing MUST NOT depend on cortex-crdt — leaf crate guarantee violated"
    );
}

// ── Cross-path signature rejection ──────────────────────────────────────

/// ghost-signing produces raw `sign(bytes)` signatures over arbitrary data.
/// cortex-crdt produces `sign(canonical_bytes(delta, author, timestamp))`.
/// Even with the same underlying ed25519 key, a raw signature over just the
/// delta JSON MUST NOT verify as a cortex-crdt SignedDelta because the
/// canonical format includes `author || timestamp`.
#[test]
fn raw_ed25519_sig_over_json_does_not_verify_as_crdt_delta() {
    use ed25519_dalek::Signer;

    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        key: "cross_path".into(),
        value: 42,
    };

    // Sign the delta JSON directly (ghost-signing semantics: raw bytes)
    let raw_bytes = serde_json::to_vec(&delta).unwrap();
    let raw_sig = key.sign(&raw_bytes);

    // Construct a SignedDelta with the raw signature
    let forged = cortex_crdt::signing::SignedDelta {
        delta,
        author,
        signature: raw_sig,
        timestamp: chrono::Utc::now(),
    };

    assert!(
        !verify_delta(&forged, &verifying),
        "raw ed25519 signature over JSON must NOT verify as a cortex-crdt SignedDelta \
         — canonical bytes include author UUID and timestamp, not just the delta"
    );
}

// ── Canonical bytes determinism ─────────────────────────────────────────

#[test]
fn canonical_bytes_are_deterministic_across_calls() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        key: "determinism".into(),
        value: 99,
    };

    let signed = sign_delta(delta.clone(), author, &key);

    // Verify multiple times — deterministic canonical bytes means
    // verification is idempotent
    for _ in 0..100 {
        assert!(verify_delta(&signed, &verifying));
    }
}

// ── Key registry isolation ──────────────────────────────────────────────

#[test]
fn key_registry_rejects_verification_with_wrong_agent_key() {
    let key_a = SigningKey::generate(&mut OsRng);
    let key_b = SigningKey::generate(&mut OsRng);
    let agent_a = Uuid::new_v4();
    let agent_b = Uuid::new_v4();

    let mut registry = KeyRegistry::new();
    registry.register(agent_a, key_a.verifying_key());
    registry.register(agent_b, key_b.verifying_key());

    let delta = TestDelta {
        key: "isolation".into(),
        value: 1,
    };

    // Sign as agent_a
    let signed = sign_delta(delta, agent_a, &key_a);

    // Verify with agent_a's key: should pass
    let vk_a = registry.get(&agent_a).unwrap();
    assert!(verify_delta(&signed, vk_a));

    // Verify with agent_b's key: must fail
    let vk_b = registry.get(&agent_b).unwrap();
    assert!(
        !verify_delta(&signed, vk_b),
        "delta signed by agent_a must not verify with agent_b's key"
    );
}

// ── Timestamp tampering ─────────────────────────────────────────────────

#[test]
fn timestamp_tampering_invalidates_signature() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        key: "timestamp".into(),
        value: 1,
    };

    let mut signed = sign_delta(delta, author, &key);

    // Tamper with timestamp (backdate by 1 hour)
    signed.timestamp = signed.timestamp - chrono::Duration::hours(1);

    assert!(
        !verify_delta(&signed, &verifying),
        "backdated timestamp must invalidate signature — timestamp is part of canonical bytes"
    );
}

// ── Adversarial: mass key rotation stress ───────────────────────────────

#[test]
fn key_registry_handles_rapid_rotation() {
    let mut registry = KeyRegistry::new();
    let agent = Uuid::new_v4();

    // Simulate rapid key rotation: register 1000 different keys for same agent
    let mut last_key = None;
    for _ in 0..1000 {
        let key = SigningKey::generate(&mut OsRng);
        registry.register(agent, key.verifying_key());
        last_key = Some(key);
    }

    // Only the last key should be in the registry
    assert_eq!(registry.len(), 1);

    let key = last_key.unwrap();
    let delta = TestDelta {
        key: "rotation".into(),
        value: 1,
    };
    let signed = sign_delta(delta, agent, &key);
    let vk = registry.get(&agent).unwrap();
    assert!(verify_delta(&signed, vk));
}

// ── Adversarial: empty and edge-case deltas ─────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EmptyDelta {}

#[test]
fn empty_delta_signs_and_verifies() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let signed = sign_delta(EmptyDelta {}, author, &key);
    assert!(verify_delta(&signed, &verifying));
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct LargeDelta {
    data: Vec<u8>,
}

#[test]
fn large_delta_signs_and_verifies() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = LargeDelta {
        data: vec![0xAB; 1_000_000], // 1MB payload
    };

    let signed = sign_delta(delta, author, &key);
    assert!(verify_delta(&signed, &verifying));
}

// ── Adversarial: nil UUID author ────────────────────────────────────────

#[test]
fn nil_uuid_author_still_produces_valid_signature() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let nil_author = Uuid::nil();

    let delta = TestDelta {
        key: "nil_author".into(),
        value: 0,
    };

    let signed = sign_delta(delta, nil_author, &key);
    assert!(verify_delta(&signed, &verifying));
    assert_eq!(signed.author, Uuid::nil());
}

#[test]
fn nil_uuid_author_tampered_to_real_uuid_fails() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();

    let delta = TestDelta {
        key: "nil_swap".into(),
        value: 0,
    };

    let mut signed = sign_delta(delta, Uuid::nil(), &key);
    signed.author = Uuid::new_v4(); // swap nil → real

    assert!(
        !verify_delta(&signed, &verifying),
        "swapping nil author to real UUID must invalidate signature"
    );
}
