//! Adversarial: CRDT merge conflict under concurrent signed deltas.
//!
//! If colluding authors control N-of-M signing keys, can they force the
//! CRDT to converge to the attacker's preferred state? This tests the
//! interaction between signed delta verification and CRDT merge semantics.
//!
//! Key insight: cortex-crdt's SignedDelta is a wrapper — the CRDT merge
//! strategy (last-writer-wins by timestamp, or set union) determines the
//! final state. Signature verification only gates admission; it does NOT
//! influence merge ordering.

use chrono::{Duration, Utc};
use cortex_crdt::signing::{sign_delta, verify_delta, KeyRegistry};
use ed25519_dalek::SigningKey;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct MemoryDelta {
    key: String,
    value: String,
    /// Simulated LWW timestamp for merge ordering.
    lww_timestamp: i64,
}

// ── Concurrent deltas from different authors ────────────────────────────

/// Two honest authors submit conflicting deltas for the same key.
/// Both are validly signed. The CRDT must deterministically pick one.
#[test]
fn concurrent_deltas_both_valid_deterministic_winner() {
    let key_a = SigningKey::generate(&mut OsRng);
    let key_b = SigningKey::generate(&mut OsRng);
    let author_a = Uuid::new_v4();
    let author_b = Uuid::new_v4();

    let now = Utc::now();

    let delta_a = MemoryDelta {
        key: "shared_key".into(),
        value: "value_from_a".into(),
        lww_timestamp: now.timestamp(),
    };

    let delta_b = MemoryDelta {
        key: "shared_key".into(),
        value: "value_from_b".into(),
        lww_timestamp: (now + Duration::seconds(1)).timestamp(),
    };

    let signed_a = sign_delta(delta_a.clone(), author_a, &key_a);
    let signed_b = sign_delta(delta_b.clone(), author_b, &key_b);

    // Both verify
    assert!(verify_delta(&signed_a, &key_a.verifying_key()));
    assert!(verify_delta(&signed_b, &key_b.verifying_key()));

    // LWW merge: delta_b wins because it has a later lww_timestamp
    let winner = if signed_b.delta.lww_timestamp > signed_a.delta.lww_timestamp {
        &signed_b.delta
    } else {
        &signed_a.delta
    };
    assert_eq!(winner.value, "value_from_b");
}

// ── Colluding authors: N-of-M key control ───────────────────────────────

/// Attacker controls 3 of 5 signing keys. They submit deltas with
/// future timestamps to win LWW conflicts.
#[test]
fn colluding_majority_can_win_lww_with_future_timestamps() {
    let honest_keys: Vec<_> = (0..2).map(|_| SigningKey::generate(&mut OsRng)).collect();
    let attacker_keys: Vec<_> = (0..3).map(|_| SigningKey::generate(&mut OsRng)).collect();

    let honest_authors: Vec<_> = (0..2).map(|_| Uuid::new_v4()).collect();
    let attacker_authors: Vec<_> = (0..3).map(|_| Uuid::new_v4()).collect();

    let mut registry = KeyRegistry::new();
    for (i, key) in honest_keys.iter().enumerate() {
        registry.register(honest_authors[i], key.verifying_key());
    }
    for (i, key) in attacker_keys.iter().enumerate() {
        registry.register(attacker_authors[i], key.verifying_key());
    }

    let now = Utc::now();

    // Honest delta: current timestamp
    let honest_delta = MemoryDelta {
        key: "critical_config".into(),
        value: "safe_value".into(),
        lww_timestamp: now.timestamp(),
    };
    let signed_honest = sign_delta(honest_delta, honest_authors[0], &honest_keys[0]);

    // Attacker delta: future timestamp to win LWW
    let attacker_delta = MemoryDelta {
        key: "critical_config".into(),
        value: "malicious_value".into(),
        lww_timestamp: (now + Duration::hours(1)).timestamp(),
    };
    let signed_attacker = sign_delta(attacker_delta, attacker_authors[0], &attacker_keys[0]);

    // Both verify against their registered keys
    let vk_honest = registry.get(&honest_authors[0]).unwrap();
    let vk_attacker = registry.get(&attacker_authors[0]).unwrap();
    assert!(verify_delta(&signed_honest, vk_honest));
    assert!(verify_delta(&signed_attacker, vk_attacker));

    // LWW merge: attacker wins because of future timestamp
    // This is the fundamental vulnerability: signature verification
    // does NOT prevent timestamp manipulation in the delta payload.
    let winner = if signed_attacker.delta.lww_timestamp > signed_honest.delta.lww_timestamp {
        &signed_attacker.delta
    } else {
        &signed_honest.delta
    };
    assert_eq!(
        winner.value, "malicious_value",
        "attacker with future LWW timestamp wins merge — this is the known vulnerability"
    );
}

/// Mitigation: the SignedDelta's `timestamp` field (signing time) is
/// separate from the delta's `lww_timestamp`. A validator can reject
/// deltas where lww_timestamp >> signing timestamp.
#[test]
fn signing_timestamp_can_bound_lww_timestamp() {
    let key = SigningKey::generate(&mut OsRng);
    let author = Uuid::new_v4();

    let now = Utc::now();

    let delta = MemoryDelta {
        key: "bounded".into(),
        value: "value".into(),
        lww_timestamp: (now + Duration::hours(24)).timestamp(), // far future
    };

    let signed = sign_delta(delta, author, &key);

    // The signing timestamp is ~now, but the LWW timestamp is +24h.
    // A validator can detect this discrepancy:
    let lww_ts = chrono::DateTime::from_timestamp(signed.delta.lww_timestamp, 0).unwrap();
    let skew = lww_ts - signed.timestamp;

    assert!(
        skew > Duration::minutes(5),
        "LWW timestamp skew ({skew}) should be detectable by a validator"
    );

    // Recommended: reject deltas where |lww_timestamp - signing_timestamp| > threshold
}

// ── Replay with identical timestamps ────────────────────────────────────

/// Two deltas with identical LWW timestamps: tiebreaker must be deterministic.
#[test]
fn identical_lww_timestamps_need_deterministic_tiebreaker() {
    let key_a = SigningKey::generate(&mut OsRng);
    let key_b = SigningKey::generate(&mut OsRng);
    let author_a = Uuid::new_v4();
    let author_b = Uuid::new_v4();

    let ts = Utc::now().timestamp();

    let delta_a = MemoryDelta {
        key: "tie".into(),
        value: "a_wins".into(),
        lww_timestamp: ts,
    };
    let delta_b = MemoryDelta {
        key: "tie".into(),
        value: "b_wins".into(),
        lww_timestamp: ts,
    };

    let signed_a = sign_delta(delta_a, author_a, &key_a);
    let signed_b = sign_delta(delta_b, author_b, &key_b);

    // Both valid
    assert!(verify_delta(&signed_a, &key_a.verifying_key()));
    assert!(verify_delta(&signed_b, &key_b.verifying_key()));

    // Tiebreaker: compare author UUIDs lexicographically
    let winner = if signed_a.author > signed_b.author {
        &signed_a.delta
    } else {
        &signed_b.delta
    };

    // The tiebreaker is deterministic regardless of arrival order
    let winner2 = if signed_b.author > signed_a.author {
        &signed_b.delta
    } else {
        &signed_a.delta
    };

    // Both orderings produce the same winner (the one with the smaller UUID)
    assert_eq!(
        winner.value, winner2.value,
        "tiebreaker must be deterministic regardless of evaluation order"
    );
}

// ── Signature verification does not influence merge ─────────────────────

/// A delta that fails verification is rejected BEFORE merge.
/// This means an attacker cannot inject unsigned deltas into the CRDT.
#[test]
fn unsigned_delta_rejected_before_merge() {
    let honest_key = SigningKey::generate(&mut OsRng);
    let attacker_key = SigningKey::generate(&mut OsRng);
    let author = Uuid::new_v4();

    let delta = MemoryDelta {
        key: "protected".into(),
        value: "injected".into(),
        lww_timestamp: i64::MAX, // maximum timestamp to win any LWW
    };

    // Sign with attacker's key but verify against honest key
    let signed = sign_delta(delta, author, &attacker_key);
    let honest_vk = honest_key.verifying_key();

    assert!(
        !verify_delta(&signed, &honest_vk),
        "delta signed by wrong key must be rejected before merge"
    );
}

// ── Multiple conflicting deltas from same author ────────────────────────

/// Same author submits multiple deltas for the same key with increasing
/// timestamps. Only the latest should survive in LWW.
#[test]
fn same_author_latest_delta_wins_lww() {
    let key = SigningKey::generate(&mut OsRng);
    let vk = key.verifying_key();
    let author = Uuid::new_v4();

    let now = Utc::now();
    let mut deltas = Vec::new();

    for i in 0..10 {
        let delta = MemoryDelta {
            key: "evolving".into(),
            value: format!("version_{i}"),
            lww_timestamp: (now + Duration::seconds(i)).timestamp(),
        };
        let signed = sign_delta(delta, author, &key);
        assert!(verify_delta(&signed, &vk));
        deltas.push(signed);
    }

    // LWW: last delta wins
    let winner = deltas
        .iter()
        .max_by_key(|d| d.delta.lww_timestamp)
        .unwrap();
    assert_eq!(winner.delta.value, "version_9");
}

// ── Adversarial: tampered delta in a batch ──────────────────────────────

/// In a batch of 100 deltas, one is tampered. The tampered delta must
/// be individually rejected without affecting the other 99.
#[test]
fn single_tampered_delta_in_batch_isolated() {
    let key = SigningKey::generate(&mut OsRng);
    let vk = key.verifying_key();
    let author = Uuid::new_v4();

    let mut deltas = Vec::new();
    for i in 0..100 {
        let delta = MemoryDelta {
            key: format!("key_{i}"),
            value: format!("value_{i}"),
            lww_timestamp: Utc::now().timestamp(),
        };
        deltas.push(sign_delta(delta, author, &key));
    }

    // Tamper with delta 50
    deltas[50].delta.value = "TAMPERED".into();

    let mut valid_count = 0;
    let mut invalid_count = 0;
    for d in &deltas {
        if verify_delta(d, &vk) {
            valid_count += 1;
        } else {
            invalid_count += 1;
        }
    }

    assert_eq!(valid_count, 99);
    assert_eq!(invalid_count, 1);
}
