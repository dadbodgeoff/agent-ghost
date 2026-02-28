//! Property-based tests for cortex-crdt signing (Task 3.6).

use cortex_crdt::signing::{sign_delta, verify_delta};
use ed25519_dalek::SigningKey;
use proptest::prelude::*;
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ArbitraryDelta {
    key: String,
    value: i64,
    tags: Vec<String>,
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Sign then verify always returns true (round-trip).
    #[test]
    fn sign_verify_round_trip(
        key_str in "[a-z]{1,50}",
        value in any::<i64>(),
        tag_count in 0usize..5,
    ) {
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let author = Uuid::new_v4();

        let tags: Vec<String> = (0..tag_count).map(|i| format!("tag_{i}")).collect();
        let delta = ArbitraryDelta {
            key: key_str,
            value,
            tags,
        };

        let signed = sign_delta(delta, author, &signing_key);
        prop_assert!(
            verify_delta(&signed, &verifying_key),
            "sign then verify must always succeed"
        );
    }

    /// Modifying delta content after signing always fails verification.
    #[test]
    fn tamper_detection(
        original_value in any::<i64>(),
        tampered_value in any::<i64>(),
    ) {
        prop_assume!(original_value != tampered_value);

        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();
        let author = Uuid::new_v4();

        let delta = ArbitraryDelta {
            key: "test".into(),
            value: original_value,
            tags: vec![],
        };

        let mut signed = sign_delta(delta, author, &signing_key);
        signed.delta.value = tampered_value;

        prop_assert!(
            !verify_delta(&signed, &verifying_key),
            "tampered delta must fail verification"
        );
    }

    /// Cross-key verification always fails.
    #[test]
    fn cross_key_always_fails(
        value in any::<i64>(),
    ) {
        let key_a = SigningKey::generate(&mut OsRng);
        let key_b = SigningKey::generate(&mut OsRng);
        let author = Uuid::new_v4();

        let delta = ArbitraryDelta {
            key: "cross".into(),
            value,
            tags: vec![],
        };

        let signed = sign_delta(delta, author, &key_a);
        prop_assert!(
            !verify_delta(&signed, &key_b.verifying_key()),
            "cross-key verification must always fail"
        );
    }
}
