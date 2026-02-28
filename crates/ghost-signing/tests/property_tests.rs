//! Property-based tests for ghost-signing.
//!
//! Proptest strategies exercise the signing/verification contract across
//! random payloads, ensuring the cryptographic invariants hold universally.

use proptest::prelude::*;

use ghost_signing::{generate_keypair, sign, verify};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// AC6: For 1000 random byte payloads (0–64KB), sign then verify returns true.
    #[test]
    fn round_trip_holds_for_random_payloads(
        data in proptest::collection::vec(any::<u8>(), 0..=65_536)
    ) {
        let (sk, vk) = generate_keypair();
        let sig = sign(&data, &sk);
        prop_assert!(verify(&data, &sig, &vk), "round-trip failed for {} bytes", data.len());
    }

    /// Cross-key: For 1000 random payloads, sign with key A, verify with key B → false.
    #[test]
    fn cross_key_verification_fails(
        data in proptest::collection::vec(any::<u8>(), 0..=65_536)
    ) {
        let (sk_a, _vk_a) = generate_keypair();
        let (_sk_b, vk_b) = generate_keypair();
        let sig = sign(&data, &sk_a);
        prop_assert!(!verify(&data, &sig, &vk_b), "cross-key verify should fail");
    }

    /// AC7: For 1000 random payloads, sign, mutate 1 random byte, verify → false.
    #[test]
    fn single_byte_mutation_detected(
        data in proptest::collection::vec(any::<u8>(), 1..=65_536),
        flip_index in any::<prop::sample::Index>(),
        flip_value in any::<u8>(),
    ) {
        let (sk, vk) = generate_keypair();
        let sig = sign(&data, &sk);

        let mut mutated = data.clone();
        let idx = flip_index.index(mutated.len());
        // Ensure we actually change the byte.
        let original = mutated[idx];
        mutated[idx] = if flip_value == original {
            original.wrapping_add(1)
        } else {
            flip_value
        };

        prop_assert!(!verify(&mutated, &sig, &vk), "tampered payload should fail verification");
    }
}
