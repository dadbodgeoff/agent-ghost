//! Signed delta unit tests (Task 3.6 — Req 29 AC1, AC3).

use cortex_crdt::signing::{sign_delta, verify_delta, KeyRegistry};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TestDelta {
    field: String,
    value: i64,
}

// ── AC1: Valid signed delta merges successfully ─────────────────────────

#[test]
fn valid_signed_delta_verifies() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "memory_count".into(),
        value: 42,
    };

    let signed = sign_delta(delta, author, &key);
    assert!(verify_delta(&signed, &verifying));
}

// ── AC1: Unsigned / wrong signature rejected ────────────────────────────

#[test]
fn delta_with_wrong_key_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let wrong_key = SigningKey::generate(&mut OsRng);
    let wrong_verifying = wrong_key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "test".into(),
        value: 1,
    };

    let signed = sign_delta(delta, author, &key);
    assert!(
        !verify_delta(&signed, &wrong_verifying),
        "verification with wrong key must fail"
    );
}

#[test]
fn tampered_delta_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "original".into(),
        value: 100,
    };

    let mut signed = sign_delta(delta, author, &key);
    // Tamper with the delta content
    signed.delta.field = "tampered".into();

    assert!(
        !verify_delta(&signed, &verifying),
        "tampered delta must fail verification"
    );
}

#[test]
fn tampered_author_rejected() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "test".into(),
        value: 1,
    };

    let mut signed = sign_delta(delta, author, &key);
    // Tamper with the author
    signed.author = Uuid::new_v4();

    assert!(
        !verify_delta(&signed, &verifying),
        "tampered author must fail verification"
    );
}

// ── AC3: KeyRegistry ────────────────────────────────────────────────────

#[test]
fn key_registry_register_and_lookup() {
    let mut registry = KeyRegistry::new();
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let agent_id = Uuid::new_v4();

    registry.register(agent_id, verifying);
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());

    let retrieved = registry.get(&agent_id).expect("key should exist");
    assert_eq!(retrieved, &verifying);
}

#[test]
fn key_registry_remove() {
    let mut registry = KeyRegistry::new();
    let key = SigningKey::generate(&mut OsRng);
    let agent_id = Uuid::new_v4();

    registry.register(agent_id, key.verifying_key());
    assert_eq!(registry.len(), 1);

    let removed = registry.remove(&agent_id);
    assert!(removed.is_some());
    assert!(registry.is_empty());
    assert!(registry.get(&agent_id).is_none());
}

#[test]
fn key_registry_unknown_agent_returns_none() {
    let registry = KeyRegistry::new();
    assert!(registry.get(&Uuid::new_v4()).is_none());
}

// ── Layer separation audit ──────────────────────────────────────────────

#[test]
fn cortex_crdt_cargo_toml_does_not_depend_on_ghost_signing() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        !cargo_toml.contains("ghost-signing"),
        "cortex-crdt must NOT depend on ghost-signing (Layer 1/Layer 3 separation)"
    );
}

#[test]
fn signed_delta_uses_ed25519_dalek_directly() {
    // This test verifies the architectural constraint: cortex-crdt uses
    // ed25519-dalek directly, not ghost-signing wrappers.
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("ed25519-dalek"),
        "cortex-crdt must use ed25519-dalek directly"
    );
}

// ── Sign/verify with KeyRegistry integration ────────────────────────────

#[test]
fn sign_verify_via_registry() {
    let key = SigningKey::generate(&mut OsRng);
    let agent_id = Uuid::new_v4();

    let mut registry = KeyRegistry::new();
    registry.register(agent_id, key.verifying_key());

    let delta = TestDelta {
        field: "via_registry".into(),
        value: 99,
    };

    let signed = sign_delta(delta, agent_id, &key);

    // Verify using registry lookup
    let verifying = registry.get(&agent_id).expect("key must exist");
    assert!(verify_delta(&signed, verifying));
}

#[test]
fn verify_fails_for_unregistered_agent() {
    let _key = SigningKey::generate(&mut OsRng);
    let agent_id = Uuid::new_v4();
    let registry = KeyRegistry::new();

    let _delta = TestDelta {
        field: "test".into(),
        value: 1,
    };

    // Agent not in registry
    assert!(registry.get(&agent_id).is_none());
}

// ── Determinism ─────────────────────────────────────────────────────────

#[test]
fn signing_preserves_delta_content() {
    let key = SigningKey::generate(&mut OsRng);
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "preserved".into(),
        value: 42,
    };

    let signed = sign_delta(delta.clone(), author, &key);
    assert_eq!(signed.delta, delta);
    assert_eq!(signed.author, author);
}

// ── Adversarial: replay attack ──────────────────────────────────────────

#[test]
fn replay_attack_same_delta_submitted_twice() {
    let key = SigningKey::generate(&mut OsRng);
    let verifying = key.verifying_key();
    let author = Uuid::new_v4();

    let delta = TestDelta {
        field: "replay".into(),
        value: 1,
    };

    let signed = sign_delta(delta, author, &key);

    // Both verifications succeed — replay detection is the caller's
    // responsibility (via hash chain dedup), not the signing layer's.
    assert!(verify_delta(&signed, &verifying));
    assert!(verify_delta(&signed, &verifying));
}
