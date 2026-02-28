//! Task 15.2 — Post-v1 proptest strategy validation.
//!
//! Verifies that all 11 new strategies from Phase 15 produce valid instances
//! without panics, and that key invariants hold across 1000 samples.

use cortex_test_fixtures::strategies::*;
use proptest::prelude::*;

// ═══════════════════════════════════════════════════════════════════════
// Unit: Each new strategy produces valid instances (no panics on 1000)
// ═══════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn egress_config_strategy_no_panic(config in egress_config_strategy()) {
        // Valid: policy is one of the 3 modes, domains are strings.
        let _ = serde_json::to_string(&config).unwrap();
    }

    #[test]
    fn domain_pattern_strategy_no_panic(domain in domain_pattern_strategy()) {
        prop_assert!(!domain.is_empty(), "domain should not be empty");
    }

    #[test]
    fn oauth_ref_id_strategy_no_panic(ref_id in oauth_ref_id_strategy()) {
        let json = serde_json::to_string(&ref_id).unwrap();
        let _: ghost_oauth::types::OAuthRefId = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn token_set_strategy_no_panic(ts in token_set_strategy()) {
        // TokenSet has SecretString — verify it exists and scopes are valid.
        let _ = ts.is_expired();
        prop_assert!(ts.scopes.len() < 10);
    }

    #[test]
    fn agent_card_strategy_no_panic(card in agent_card_strategy()) {
        prop_assert!(!card.name.is_empty(), "card name should not be empty");
        prop_assert!(!card.signature.is_empty(), "card should be signed");
    }

    #[test]
    fn mesh_task_strategy_no_panic(task in mesh_task_strategy()) {
        let json = serde_json::to_string(&task).unwrap();
        let _: ghost_mesh::types::MeshTask = serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn interaction_outcome_strategy_no_panic(outcome in interaction_outcome_strategy()) {
        let json = serde_json::to_string(&outcome).unwrap();
        let _: ghost_mesh::trust::local_trust::InteractionOutcome =
            serde_json::from_str(&json).unwrap();
    }

    #[test]
    fn trust_matrix_strategy_no_panic(matrix in trust_matrix_strategy()) {
        // No self-trust entries.
        for ((a, b), v) in &matrix {
            prop_assert_ne!(a, b, "trust matrix should not contain self-trust");
            prop_assert!((0.0..=1.0).contains(v), "trust value {} outside [0,1]", v);
        }
    }

    #[test]
    fn tool_call_plan_strategy_no_panic(plan in tool_call_plan_strategy()) {
        // Plan should have 0..8 calls.
        prop_assert!(plan.calls.len() <= 8);
    }

    #[test]
    fn spotlighting_config_strategy_no_panic(config in spotlighting_config_strategy()) {
        let _ = serde_json::to_string(&config).unwrap();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Proptest: signal_array_8_strategy always produces values in [0.0, 1.0]
// ═══════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn signal_array_8_all_in_range(signals in signal_array_8_strategy()) {
        for (i, &s) in signals.iter().enumerate() {
            prop_assert!(
                (0.0..=1.0).contains(&s),
                "Signal {} = {} outside [0.0, 1.0]", i, s
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Proptest: agent_card_strategy produces cards with valid signatures
// ═══════════════════════════════════════════════════════════════════════

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn agent_card_strategy_valid_signatures(card in agent_card_strategy()) {
        prop_assert!(
            card.verify_signature(),
            "agent_card_strategy should produce cards with valid signatures"
        );
    }
}
