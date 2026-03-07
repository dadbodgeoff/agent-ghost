//! Tests for cortex-temporal: hash chains + Merkle trees.

use cortex_temporal::anchoring::merkle::MerkleTree;
use cortex_temporal::hash_chain::*;

// ── Helper ──────────────────────────────────────────────────────────────

fn make_chain(len: usize) -> Vec<ChainEvent> {
    let mut events = Vec::with_capacity(len);
    let mut prev = GENESIS_HASH;
    for i in 0..len {
        let hash = compute_event_hash(
            &format!("type_{}", i),
            &format!("{{\"i\":{}}}", i),
            "actor-1",
            &format!("2025-01-01T00:00:{:02}Z", i % 60),
            &prev,
        );
        events.push(ChainEvent {
            event_type: format!("type_{}", i),
            delta_json: format!("{{\"i\":{}}}", i),
            actor_id: "actor-1".to_string(),
            recorded_at: format!("2025-01-01T00:00:{:02}Z", i % 60),
            event_hash: hash,
            previous_hash: prev,
        });
        prev = hash;
    }
    events
}

// ── Unit tests ──────────────────────────────────────────────────────────

#[test]
fn genesis_hash_is_all_zeros() {
    assert_eq!(GENESIS_HASH, [0u8; 32]);
}

#[test]
fn single_event_chain_verifies() {
    let chain = make_chain(1);
    let result = verify_chain(&chain);
    assert!(result.is_valid);
    assert_eq!(result.verified_events, 1);
}

#[test]
fn empty_chain_is_valid() {
    let result = verify_chain(&[]);
    assert!(result.is_valid);
    assert_eq!(result.total_events, 0);
}

#[test]
fn compute_event_hash_is_deterministic() {
    let h1 = compute_event_hash("type", "{}", "actor", "2025-01-01", &GENESIS_HASH);
    let h2 = compute_event_hash("type", "{}", "actor", "2025-01-01", &GENESIS_HASH);
    assert_eq!(h1, h2);
}

#[test]
fn different_event_type_produces_different_hash() {
    let h1 = compute_event_hash("typeA", "{}", "actor", "2025-01-01", &GENESIS_HASH);
    let h2 = compute_event_hash("typeB", "{}", "actor", "2025-01-01", &GENESIS_HASH);
    assert_ne!(h1, h2);
}

#[test]
fn valid_chain_of_100_verifies() {
    let chain = make_chain(100);
    let result = verify_chain(&chain);
    assert!(result.is_valid);
    assert_eq!(result.verified_events, 100);
}

// ── Tamper detection ────────────────────────────────────────────────────

#[test]
fn tampered_event_hash_detected() {
    let mut chain = make_chain(10);
    chain[5].event_hash[0] ^= 0xFF; // flip a byte
    let result = verify_chain(&chain);
    assert!(!result.is_valid);
}

#[test]
fn tampered_previous_hash_detected() {
    let mut chain = make_chain(10);
    chain[5].previous_hash[0] ^= 0xFF;
    let result = verify_chain(&chain);
    assert!(!result.is_valid);
}

#[test]
fn duplicate_event_hash_detected() {
    let mut chain = make_chain(5);
    chain[3].event_hash = chain[1].event_hash;
    let result = verify_chain(&chain);
    assert!(!result.is_valid);
    match result.error {
        Some(ChainError::DuplicateHash { first, second }) => {
            assert_eq!(first, 1);
            assert_eq!(second, 3);
        }
        other => panic!("expected DuplicateHash, got {:?}", other),
    }
}

// ── Merkle tree tests ───────────────────────────────────────────────────

#[test]
fn merkle_single_leaf_root_equals_leaf() {
    let leaf = [42u8; 32];
    let tree = MerkleTree::from_chain(&[leaf]);
    assert_eq!(tree.root, leaf);
}

#[test]
fn merkle_two_leaves_inclusion_proof() {
    let leaves: Vec<[u8; 32]> = (0..2).map(|i| [i as u8; 32]).collect();
    let tree = MerkleTree::from_chain(&leaves);
    for i in 0..2 {
        let proof = tree.inclusion_proof(i);
        assert!(
            MerkleTree::verify_proof(&tree.root, &leaves[i], &proof, i),
            "proof failed for leaf {}",
            i
        );
    }
}

#[test]
fn merkle_1000_leaves_random_proof() {
    let chain = make_chain(1000);
    let leaves: Vec<[u8; 32]> = chain.iter().map(|e| e.event_hash).collect();
    let tree = MerkleTree::from_chain(&leaves);
    // Verify a few random indices
    for &idx in &[0, 1, 499, 500, 998, 999] {
        let proof = tree.inclusion_proof(idx);
        assert!(
            MerkleTree::verify_proof(&tree.root, &leaves[idx], &proof, idx),
            "proof failed for leaf {}",
            idx
        );
    }
}

#[test]
fn merkle_wrong_root_returns_false() {
    let leaves: Vec<[u8; 32]> = (0..4).map(|i| [i as u8; 32]).collect();
    let tree = MerkleTree::from_chain(&leaves);
    let proof = tree.inclusion_proof(0);
    let wrong_root = [0xFF; 32];
    assert!(!MerkleTree::verify_proof(
        &wrong_root,
        &leaves[0],
        &proof,
        0
    ));
}

#[test]
fn merkle_wrong_leaf_returns_false() {
    let leaves: Vec<[u8; 32]> = (0..4).map(|i| [i as u8; 32]).collect();
    let tree = MerkleTree::from_chain(&leaves);
    let proof = tree.inclusion_proof(0);
    let wrong_leaf = [0xFF; 32];
    assert!(!MerkleTree::verify_proof(
        &tree.root,
        &wrong_leaf,
        &proof,
        0
    ));
}

#[test]
fn merkle_empty_chain() {
    let tree = MerkleTree::from_chain(&[]);
    assert_eq!(tree.root, [0u8; 32]);
    assert!(tree.leaves.is_empty());
}

// ── Proptest ────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn chain_round_trip(len in 1usize..200) {
            let chain = make_chain(len);
            let result = verify_chain(&chain);
            prop_assert!(result.is_valid, "valid chain of {} events should verify", len);
            prop_assert_eq!(result.verified_events, len);
        }

        #[test]
        fn tamper_single_byte_detected(
            len in 2usize..100,
            tamper_idx in 0usize..100,
            byte_idx in 0usize..32,
        ) {
            let mut chain = make_chain(len);
            let tamper_idx = tamper_idx % len;
            // Tamper with the delta_json to change the expected hash
            chain[tamper_idx].delta_json.push('X');
            let result = verify_chain(&chain);
            prop_assert!(!result.is_valid, "tampered chain should not verify");
        }

        #[test]
        fn tamper_previous_hash_detected(
            len in 2usize..100,
            tamper_idx in 1usize..100,
        ) {
            let mut chain = make_chain(len);
            let tamper_idx = tamper_idx % (len - 1) + 1; // skip index 0
            chain[tamper_idx].previous_hash[0] ^= 0xFF;
            let result = verify_chain(&chain);
            prop_assert!(!result.is_valid, "tampered previous_hash should not verify");
        }

        #[test]
        fn merkle_inclusion_proof_round_trip(len in 1usize..200) {
            let chain = make_chain(len);
            let leaves: Vec<[u8; 32]> = chain.iter().map(|e| e.event_hash).collect();
            let tree = MerkleTree::from_chain(&leaves);
            // Verify first and last leaf
            let proof_first = tree.inclusion_proof(0);
            prop_assert!(MerkleTree::verify_proof(&tree.root, &leaves[0], &proof_first, 0));
            let last = len - 1;
            let proof_last = tree.inclusion_proof(last);
            prop_assert!(MerkleTree::verify_proof(&tree.root, &leaves[last], &proof_last, last));
        }
    }
}
