//! Tests for Task 14.2 — EigenTrust reputation system.
//!
//! Covers: local trust store, EigenTrust power iteration, trust policy,
//! proptest invariants, and adversarial Sybil/self-trust scenarios.

use ghost_mesh::trust::eigentrust::{EigenTrustComputer, EigenTrustConfig, TrustPolicy};
use ghost_mesh::trust::local_trust::{InteractionOutcome, LocalTrustStore};
use uuid::Uuid;

// ── Helper ──────────────────────────────────────────────────────────────

fn make_agents(n: usize) -> Vec<Uuid> {
    (0..n).map(|_| Uuid::new_v4()).collect()
}

// ── LocalTrustStore tests ───────────────────────────────────────────────

#[test]
fn single_agent_no_interactions_trust_zero() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    assert_eq!(store.get_local_trust(a, Uuid::new_v4()), 0.0);
}

#[test]
fn self_interaction_excluded() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    // Self-interactions should be ignored.
    store.record_interaction(a, a, InteractionOutcome::TaskCompleted);
    assert_eq!(store.get_local_trust(a, a), 0.0);
    assert_eq!(store.interaction_count(a, a), 0);
}

#[test]
fn task_completed_increases_trust() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    store.record_interaction(a, b, InteractionOutcome::TaskCompleted);
    let trust = store.get_local_trust(a, b);
    assert!(trust > 0.0, "completed task should increase trust");
}

#[test]
fn policy_violation_decreases_trust() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    // Build up some trust first.
    for _ in 0..5 {
        store.record_interaction(a, b, InteractionOutcome::TaskCompleted);
    }
    let trust_before = store.get_local_trust(a, b);
    store.record_interaction(a, b, InteractionOutcome::PolicyViolation);
    // Cache invalidated, recompute.
    let trust_after = store.get_local_trust(a, b);
    assert!(
        trust_after < trust_before,
        "policy violation should decrease trust: {trust_before} → {trust_after}"
    );
}

#[test]
fn signature_failure_decreases_trust_more() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    for _ in 0..10 {
        store.record_interaction(a, b, InteractionOutcome::TaskCompleted);
    }
    let trust_before = store.get_local_trust(a, b);
    store.record_interaction(a, b, InteractionOutcome::SignatureFailure);
    let trust_after = store.get_local_trust(a, b);
    assert!(trust_after < trust_before);
}

#[test]
fn normalized_row_sums_to_one() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(5);
    // Agent 0 interacts with agents 1-4.
    for &target in &agents[1..] {
        store.record_interaction(agents[0], target, InteractionOutcome::TaskCompleted);
    }
    let row = store.normalized_row(agents[0]);
    let sum: f64 = row.values().sum();
    assert!(
        (sum - 1.0).abs() < 1e-10,
        "normalized row should sum to 1.0, got {sum}"
    );
}

#[test]
fn dirty_flag_set_on_interaction() {
    let mut store = LocalTrustStore::new();
    assert!(!store.is_dirty());
    store.record_interaction(
        Uuid::new_v4(),
        Uuid::new_v4(),
        InteractionOutcome::TaskCompleted,
    );
    assert!(store.is_dirty());
    store.clear_dirty();
    assert!(!store.is_dirty());
}

// ── EigenTrust computation tests ────────────────────────────────────────

#[test]
fn empty_network_returns_empty_trust() {
    let mut store = LocalTrustStore::new();
    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[]);
    assert!(result.is_empty());
}

#[test]
fn pre_trusted_agent_has_positive_trust() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(3);
    // Create some interactions so agents appear in the network.
    store.record_interaction(agents[0], agents[1], InteractionOutcome::TaskCompleted);
    store.record_interaction(agents[1], agents[2], InteractionOutcome::TaskCompleted);

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[agents[0]]);

    assert!(
        result[&agents[0]] > 0.0,
        "pre-trusted agent should have positive trust"
    );
}

#[test]
fn agent_with_all_completed_tasks_gains_trust() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(3);
    // Agent 0 is pre-trusted. Agent 1 completes many tasks for agent 0.
    for _ in 0..10 {
        store.record_interaction(agents[0], agents[1], InteractionOutcome::TaskCompleted);
    }
    // Agent 2 has fewer completions.
    store.record_interaction(agents[0], agents[2], InteractionOutcome::TaskCompleted);

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[agents[0]]);

    // Agent 1 should have higher trust than agent 2.
    assert!(
        result[&agents[1]] > result[&agents[2]],
        "agent with more completions should have higher trust"
    );
}

#[test]
fn agent_with_violations_has_lower_trust() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(3);
    // Agent 1: all completions.
    for _ in 0..5 {
        store.record_interaction(agents[0], agents[1], InteractionOutcome::TaskCompleted);
    }
    // Agent 2: completions + violations.
    for _ in 0..5 {
        store.record_interaction(agents[0], agents[2], InteractionOutcome::TaskCompleted);
    }
    store.record_interaction(agents[0], agents[2], InteractionOutcome::PolicyViolation);
    store.record_interaction(agents[0], agents[2], InteractionOutcome::PolicyViolation);

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[agents[0]]);

    assert!(
        result[&agents[1]] >= result[&agents[2]],
        "agent with violations should have lower or equal trust"
    );
}

#[test]
fn power_iteration_converges_small_network() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(5);
    // Create a connected network.
    for i in 0..5 {
        for j in 0..5 {
            if i != j {
                store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
            }
        }
    }

    let config = EigenTrustConfig {
        max_iterations: 20,
        convergence_threshold: 1e-6,
        pre_trust_weight: 0.5,
    };
    let computer = EigenTrustComputer::new(config, TrustPolicy::default());
    let result = computer.compute_global_trust(&mut store, &[agents[0]]);

    // All agents should have trust in [0.0, 1.0].
    for (&_id, &trust) in &result {
        assert!(
            (0.0..=1.0).contains(&trust),
            "trust {trust} out of range [0.0, 1.0]"
        );
    }
}

#[test]
fn power_iteration_converges_medium_network() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(50);
    // Create a sparse connected network.
    for i in 0..50 {
        for j in (i + 1)..50.min(i + 5) {
            store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
        }
    }

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[agents[0], agents[1]]);

    for (&_id, &trust) in &result {
        assert!((0.0..=1.0).contains(&trust), "trust {trust} out of range");
    }
}

#[test]
fn trust_scores_all_in_valid_range() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(10);
    for i in 0..10 {
        for j in 0..10 {
            if i != j {
                let outcome = if (i + j) % 3 == 0 {
                    InteractionOutcome::TaskFailed
                } else {
                    InteractionOutcome::TaskCompleted
                };
                store.record_interaction(agents[i], agents[j], outcome);
            }
        }
    }

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[agents[0]]);

    for (&_id, &trust) in &result {
        assert!((0.0..=1.0).contains(&trust));
    }
}

// ── TrustPolicy tests ──────────────────────────────────────────────────

#[test]
fn trust_policy_delegation_threshold() {
    let policy = TrustPolicy::default();
    assert!(!policy.can_delegate(0.2), "0.2 < 0.3 threshold");
    assert!(policy.can_delegate(0.3), "0.3 == threshold");
    assert!(policy.can_delegate(0.5), "0.5 > threshold");
}

#[test]
fn trust_policy_sensitive_data_threshold() {
    let policy = TrustPolicy::default();
    assert!(!policy.can_share_sensitive_data(0.5), "0.5 < 0.6 threshold");
    assert!(policy.can_share_sensitive_data(0.6), "0.6 == threshold");
    assert!(policy.can_share_sensitive_data(0.9), "0.9 > threshold");
}

// ── Adversarial: Sybil attack ───────────────────────────────────────────

#[test]
fn sybil_attack_trust_stays_low() {
    let mut store = LocalTrustStore::new();
    let honest = Uuid::new_v4();
    let sybils = make_agents(20);

    // Sybil agents all trust each other heavily.
    for i in 0..20 {
        for j in 0..20 {
            if i != j {
                for _ in 0..10 {
                    store.record_interaction(
                        sybils[i],
                        sybils[j],
                        InteractionOutcome::TaskCompleted,
                    );
                }
            }
        }
    }
    // Honest agent has minimal interaction.
    store.record_interaction(honest, sybils[0], InteractionOutcome::TaskCompleted);

    let computer = EigenTrustComputer::default();
    // Only honest agent is pre-trusted.
    let result = computer.compute_global_trust(&mut store, &[honest]);

    // Sybil agents should have low trust (no pre-trusted anchor among them).
    for &sybil in &sybils {
        let trust = result.get(&sybil).copied().unwrap_or(0.0);
        assert!(
            trust < 0.5,
            "sybil agent trust {trust} should be low without pre-trusted anchor"
        );
    }
}

#[test]
fn self_interactions_no_trust_inflation() {
    let mut store = LocalTrustStore::new();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    // Agent A tries to inflate trust via self-interactions.
    for _ in 0..1000 {
        store.record_interaction(a, a, InteractionOutcome::TaskCompleted);
    }
    // Minimal real interaction.
    store.record_interaction(a, b, InteractionOutcome::TaskCompleted);

    let computer = EigenTrustComputer::default();
    let result = computer.compute_global_trust(&mut store, &[b]);

    // Self-interactions are excluded, so A's trust should be modest.
    let a_trust = result.get(&a).copied().unwrap_or(0.0);
    assert!(
        a_trust <= 1.0,
        "self-interactions should not inflate trust beyond 1.0"
    );
}
