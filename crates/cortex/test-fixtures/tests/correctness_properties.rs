//! Req 41: Correctness properties verified via proptest.
//!
//! Each property runs 1000+ cases. Failures are reproducible via proptest shrinking.

use cortex_convergence::scoring::baseline::BaselineState;
use cortex_convergence::scoring::composite::CompositeScorer;
use cortex_decay::factors::convergence::convergence_factor;
use cortex_temporal::hash_chain::{compute_event_hash, verify_chain, GENESIS_HASH};
use cortex_test_fixtures::strategies::*;
use proptest::prelude::*;

fn calibrated_baseline() -> BaselineState {
    let mut baseline = BaselineState::new(10);
    for i in 0..10 {
        let v = (i as f64) / 10.0;
        baseline.record_session(&[v, v, v, v, v, v, v]);
    }
    assert!(!baseline.is_calibrating);
    baseline
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn signal_range_invariant(signals in signal_array_strategy()) {
        for (i, &s) in signals.iter().enumerate() {
            prop_assert!((0.0..=1.0).contains(&s),
                "Signal {} = {} outside [0.0, 1.0]", i, s);
        }
    }

    #[test]
    fn convergence_score_bounded(signals in signal_array_strategy()) {
        let scorer = CompositeScorer::default();
        let baseline = calibrated_baseline();
        let result = scorer.score(&signals, &baseline, None, None);
        prop_assert!((0.0..=1.0).contains(&result.score),
            "Composite score {} outside [0.0, 1.0]", result.score);
    }

    #[test]
    fn decay_factor_ge_one(
        memory_type in memory_type_strategy(),
        score in convergence_score_strategy()
    ) {
        let factor = convergence_factor(&memory_type, score);
        prop_assert!(factor >= 1.0,
            "Convergence factor {} < 1.0 for {:?} at score {}",
            factor, memory_type, score);
    }


    #[test]
    fn tamper_detection(
        chain in event_chain_strategy(2, 50),
        tamper_idx in 0usize..50,
    ) {
        if chain.is_empty() { return Ok(()); }
        let idx = tamper_idx % chain.len();
        let mut tampered = chain.clone();
        tampered[idx].delta_json.push('X');
        let result = verify_chain(&tampered);
        prop_assert!(!result.is_valid,
            "Tampered chain should fail verification at index {}", idx);
    }

    #[test]
    fn hash_chain_roundtrip(chain in event_chain_strategy(1, 100)) {
        let result = verify_chain(&chain);
        prop_assert!(result.is_valid,
            "Valid chain should verify: {:?}", result.error);
    }

    #[test]
    fn amplified_score_bounded(signals in signal_array_strategy()) {
        let scorer = CompositeScorer::default();
        let baseline = calibrated_baseline();
        let meso = vec![0.3, 0.5, 0.7, 0.9];
        let result = scorer.score(&signals, &baseline, Some(&meso), Some(&[1.0; 7]));
        prop_assert!((0.0..=1.0).contains(&result.score),
            "Amplified score {} outside [0.0, 1.0]", result.score);
    }

    #[test]
    fn decay_factor_monotonic_in_score(
        memory_type in memory_type_strategy(),
        score_a in convergence_score_strategy(),
        score_b in convergence_score_strategy()
    ) {
        let (lo, hi) = if score_a <= score_b { (score_a, score_b) } else { (score_b, score_a) };
        let factor_lo = convergence_factor(&memory_type, lo);
        let factor_hi = convergence_factor(&memory_type, hi);
        prop_assert!(factor_hi >= factor_lo - f64::EPSILON,
            "Factor should be monotonic: f({})={} > f({})={}",
            lo, factor_lo, hi, factor_hi);
    }

    #[test]
    fn hash_deterministic(
        event_type in "[a-z]{3,10}",
        delta_json in "[a-zA-Z0-9]{5,50}",
        actor_id in "[a-z]{3,8}",
        recorded_at in "[0-9]{10}"
    ) {
        let h1 = compute_event_hash(&event_type, &delta_json, &actor_id, &recorded_at, &GENESIS_HASH);
        let h2 = compute_event_hash(&event_type, &delta_json, &actor_id, &recorded_at, &GENESIS_HASH);
        prop_assert_eq!(h1, h2, "Hash should be deterministic");
    }

    #[test]
    fn different_event_type_different_hash(
        type_a in "[a-z]{3,10}",
        type_b in "[a-z]{3,10}",
        delta in "[a-zA-Z0-9]{5,50}",
        actor in "[a-z]{3,8}",
        time in "[0-9]{10}"
    ) {
        prop_assume!(type_a != type_b);
        let h1 = compute_event_hash(&type_a, &delta, &actor, &time, &GENESIS_HASH);
        let h2 = compute_event_hash(&type_b, &delta, &actor, &time, &GENESIS_HASH);
        prop_assert_ne!(h1, h2, "Different event types should produce different hashes");
    }

    #[test]
    fn proposal_serde_roundtrip(proposal in proposal_strategy()) {
        let json = serde_json::to_string(&proposal).unwrap();
        let deserialized: cortex_core::traits::convergence::Proposal =
            serde_json::from_str(&json).unwrap();
        prop_assert_eq!(proposal, deserialized);
    }

    #[test]
    fn trigger_event_serde_roundtrip(event in trigger_event_strategy()) {
        let json = serde_json::to_string(&event).unwrap();
        let deserialized: cortex_core::safety::trigger::TriggerEvent =
            serde_json::from_str(&json).unwrap();
        prop_assert_eq!(event, deserialized);
    }
}
