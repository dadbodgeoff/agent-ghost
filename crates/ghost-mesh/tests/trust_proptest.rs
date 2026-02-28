//! Property tests for Task 14.2 — EigenTrust reputation.

use ghost_mesh::trust::eigentrust::EigenTrustComputer;
use ghost_mesh::trust::local_trust::{InteractionOutcome, LocalTrustStore};
use proptest::prelude::*;
use uuid::Uuid;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn all_trust_scores_in_valid_range(
        n_agents in 2..10usize,
        n_interactions in 1..50usize,
        seed in any::<u64>(),
    ) {
        // Generate deterministic agent IDs from seed.
        let agents: Vec<Uuid> = (0..n_agents)
            .map(|i| Uuid::from_u128((seed as u128) * 1000 + i as u128))
            .collect();

        let mut store = LocalTrustStore::new();
        // Create random interactions.
        for i in 0..n_interactions {
            let from = agents[i % n_agents];
            let to = agents[(i * 7 + 3) % n_agents];
            if from != to {
                let outcome = match i % 5 {
                    0 => InteractionOutcome::TaskCompleted,
                    1 => InteractionOutcome::TaskFailed,
                    2 => InteractionOutcome::PolicyViolation,
                    3 => InteractionOutcome::SignatureFailure,
                    _ => InteractionOutcome::Timeout,
                };
                store.record_interaction(from, to, outcome);
            }
        }

        let computer = EigenTrustComputer::default();
        let result = computer.compute_global_trust(&mut store, &[agents[0]]);

        for (&_id, &trust) in &result {
            prop_assert!(
                (0.0..=1.0).contains(&trust),
                "trust {} out of range [0.0, 1.0]", trust
            );
        }
    }

    #[test]
    fn power_iteration_converges(
        n_agents in 3..15usize,
        seed in any::<u64>(),
    ) {
        let agents: Vec<Uuid> = (0..n_agents)
            .map(|i| Uuid::from_u128((seed as u128) * 1000 + i as u128))
            .collect();

        let mut store = LocalTrustStore::new();
        // Create a connected network.
        for i in 0..n_agents {
            for j in 0..n_agents {
                if i != j {
                    store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
                }
            }
        }

        let computer = EigenTrustComputer::default();
        let result = computer.compute_global_trust(&mut store, &[agents[0]]);

        // All scores should be finite and in range.
        for (&_id, &trust) in &result {
            prop_assert!(trust.is_finite());
            prop_assert!((0.0..=1.0).contains(&trust));
        }
    }

    #[test]
    fn pre_trusted_agents_always_have_positive_trust(
        n_agents in 3..10usize,
        seed in any::<u64>(),
    ) {
        let agents: Vec<Uuid> = (0..n_agents)
            .map(|i| Uuid::from_u128((seed as u128) * 1000 + i as u128))
            .collect();

        let mut store = LocalTrustStore::new();
        for i in 0..n_agents {
            for j in 0..n_agents {
                if i != j {
                    store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
                }
            }
        }

        let pre_trusted = vec![agents[0]];
        let computer = EigenTrustComputer::default();
        let result = computer.compute_global_trust(&mut store, &pre_trusted);

        let pt_trust = result.get(&agents[0]).copied().unwrap_or(0.0);
        prop_assert!(
            pt_trust > 0.0,
            "pre-trusted agent should have positive trust, got {}", pt_trust
        );
    }
}
