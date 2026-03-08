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
    let cargo = include_str!("../../../../crates/cortex/cortex-crdt/Cargo.toml");
    assert!(
        !cargo.contains("ghost-signing"),
        "cortex-crdt MUST NOT depend on ghost-signing — layer separation violated"
    );
}

#[test]
fn ghost_signing_does_not_depend_on_cortex_crdt() {
    let cargo = include_str!("../../../../crates/ghost-signing/Cargo.toml");
    assert!(
        !cargo.contains("cortex-crdt"),
        "ghost-signing MUST NOT depend on cortex-crdt — leaf crate guarantee violated"
    );
}

// ── Cross-path signature rejection ──────────────────────────────────────

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
    signed.timestamp -= chrono::Duration::hours(1);

    assert!(
        !verify_delta(&signed, &verifying),
        "backdated timestamp must invalidate signature"
    );
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

    let signed = sign_delta(delta, agent_a, &key_a);

    let vk_a = registry.get(&agent_a).unwrap();
    assert!(verify_delta(&signed, vk_a));

    let vk_b = registry.get(&agent_b).unwrap();
    assert!(!verify_delta(&signed, vk_b));
}

// ── Adversarial: mass key rotation stress ───────────────────────────────

#[test]
fn key_registry_handles_rapid_rotation() {
    let mut registry = KeyRegistry::new();
    let agent = Uuid::new_v4();

    let mut last_key = None;
    for _ in 0..1000 {
        let key = SigningKey::generate(&mut OsRng);
        registry.register(agent, key.verifying_key());
        last_key = Some(key);
    }

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

// ── Adversarial: nil UUID author ────────────────────────────────────────

#[test]
fn nil_uuid_author_tampered_to_real_uuid_fails() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();

    let delta = TestDelta {
        key: "nil_swap".into(),
        value: 0,
    };

    let mut signed = sign_delta(delta, Uuid::nil(), &key);
    signed.author = Uuid::new_v4();

    assert!(!verify_delta(&signed, &verifying));
}
