//! E2E: Hash chain lifecycle — event insertion → chain verification → Merkle anchoring.
//!
//! Validates GAP-16 (ITP flow) and GAP-18 (proposal lifecycle) wiring.

use cortex_temporal::hash_chain::{compute_event_hash, verify_chain, ChainEvent, GENESIS_HASH};
use cortex_temporal::anchoring::merkle::MerkleTree;
use uuid::Uuid;

/// Full hash chain lifecycle: create events → compute hashes → verify chain → build Merkle tree.
#[test]
fn full_hash_chain_to_merkle_lifecycle() {
    // Phase 1: Build a chain of 100 events
    let mut events = Vec::new();
    let mut prev_hash = GENESIS_HASH;

    for i in 0..100 {
        let event_type = if i % 3 == 0 {
            "InteractionMessage"
        } else if i % 3 == 1 {
            "AgentStateSnapshot"
        } else {
            "ConvergenceAlert"
        };

        let delta_json = format!(r#"{{"seq":{},"content":"event {}"}}"#, i, i);
        let actor_id = "agent-001";
        let recorded_at = format!("2026-02-28T12:{:02}:00Z", i % 60);

        let hash = compute_event_hash(event_type, &delta_json, actor_id, &recorded_at, &prev_hash);

        events.push(ChainEvent {
            event_type: event_type.to_string(),
            delta_json,
            actor_id: actor_id.to_string(),
            recorded_at,
            event_hash: hash,
            previous_hash: prev_hash,
        });

        prev_hash = hash;
    }

    // Phase 2: Verify the chain
    let result = verify_chain(&events);
    assert!(result.is_valid, "Chain should verify: {:?}", result.error);

    // Phase 3: Build Merkle tree from chain hashes
    let leaves: Vec<[u8; 32]> = events.iter().map(|e| e.event_hash).collect();
    let tree = MerkleTree::from_chain(&leaves);

    // Phase 4: Verify inclusion proofs for random events
    for idx in [0, 25, 50, 75, 99] {
        let proof = tree.inclusion_proof(idx);
        assert!(
            !proof.is_empty(),
            "Should generate inclusion proof for event {}", idx
        );
        let valid = MerkleTree::verify_proof(&tree.root, &leaves[idx], &proof, idx);
        assert!(valid, "Inclusion proof should verify for event {}", idx);
    }
}

/// Tamper detection: modify one event in the middle, verify chain fails.
#[test]
fn tamper_detection_mid_chain() {
    let mut events = Vec::new();
    let mut prev_hash = GENESIS_HASH;

    for i in 0..20 {
        let hash = compute_event_hash(
            "InteractionMessage",
            &format!(r#"{{"seq":{}}}"#, i),
            "agent-001",
            "2026-02-28T12:00:00Z",
            &prev_hash,
        );
        events.push(ChainEvent {
            event_type: "InteractionMessage".to_string(),
            delta_json: format!(r#"{{"seq":{}}}"#, i),
            actor_id: "agent-001".to_string(),
            recorded_at: "2026-02-28T12:00:00Z".to_string(),
            event_hash: hash,
            previous_hash: prev_hash,
        });
        prev_hash = hash;
    }

    // Verify clean chain
    assert!(verify_chain(&events).is_valid);

    // Tamper with event 10
    events[10].delta_json = r#"{"seq":10,"tampered":true}"#.to_string();

    // Chain should now fail
    let result = verify_chain(&events);
    assert!(!result.is_valid, "Tampered chain should fail verification");
}

/// Genesis hash is all zeros.
#[test]
fn genesis_hash_is_zero() {
    assert_eq!(GENESIS_HASH, [0u8; 32]);
}

/// Empty chain verifies (vacuously true).
#[test]
fn empty_chain_verifies() {
    let result = verify_chain(&[]);
    assert!(result.is_valid);
}

/// Single-event chain with genesis as previous verifies.
#[test]
fn single_event_chain() {
    let hash = compute_event_hash(
        "SessionStart",
        r#"{"session":"abc"}"#,
        "agent-001",
        "2026-02-28T12:00:00Z",
        &GENESIS_HASH,
    );
    let events = vec![ChainEvent {
        event_type: "SessionStart".to_string(),
        delta_json: r#"{"session":"abc"}"#.to_string(),
        actor_id: "agent-001".to_string(),
        recorded_at: "2026-02-28T12:00:00Z".to_string(),
        event_hash: hash,
        previous_hash: GENESIS_HASH,
    }];
    assert!(verify_chain(&events).is_valid);
}
