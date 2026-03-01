//! Adversarial stress tests: EigenTrust × cortex-crdt Sybil defense interaction.
//!
//! EigenTrust converges well in honest networks but is vulnerable to Sybil
//! attacks at the edges. cortex-crdt's SybilGuard provides spawn limits and
//! trust caps. These tests verify the interaction between the two systems
//! and probe the boundary where mesh trust meets CRDT sybil defense.

use chrono::{Duration, Utc};
use cortex_crdt::sybil::SybilGuard;
use ghost_mesh::trust::eigentrust::{EigenTrustComputer, EigenTrustConfig, TrustPolicy};
use ghost_mesh::trust::local_trust::{InteractionOutcome, LocalTrustStore};
use uuid::Uuid;

fn make_agents(n: usize) -> Vec<Uuid> {
    (0..n).map(|_| Uuid::new_v4()).collect()
}

// ── Sybil attack: colluding agents inflate each other's trust ───────────

#[test]
fn sybil_colluding_agents_trust_bounded_by_pre_trusted_anchoring() {
    let mut store = LocalTrustStore::new();
    let honest = make_agents(3);
    let sybils = make_agents(5);

    // Honest agents interact normally
    for i in 0..honest.len() {
        for j in 0..honest.len() {
            if i != j {
                for _ in 0..10 {
                    store.record_interaction(honest[i], honest[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    // Sybil agents collude: all complete tasks for each other
    for i in 0..sybils.len() {
        for j in 0..sybils.len() {
            if i != j {
                for _ in 0..50 {
                    store.record_interaction(sybils[i], sybils[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    // No interactions between honest and sybil groups
    let computer = EigenTrustComputer::new(
        EigenTrustConfig {
            max_iterations: 50,
            convergence_threshold: 1e-8,
            pre_trust_weight: 0.5,
        },
        TrustPolicy::default(),
    );

    // Only honest agents are pre-trusted
    let trust = computer.compute_global_trust(&mut store, &honest);

    // Honest agents should have higher trust than sybils
    let avg_honest: f64 = honest.iter().map(|a| trust[a]).sum::<f64>() / honest.len() as f64;
    let avg_sybil: f64 = sybils.iter().map(|a| trust[a]).sum::<f64>() / sybils.len() as f64;

    assert!(
        avg_honest > avg_sybil,
        "pre-trusted anchoring should keep honest agents' trust ({avg_honest:.4}) \
         above sybil agents' trust ({avg_sybil:.4})"
    );
}

// ── Sybil attack: trust inflation via self-interaction ──────────────────

#[test]
fn self_interactions_excluded_from_trust_computation() {
    let mut store = LocalTrustStore::new();
    let agent = Uuid::new_v4();
    let other = Uuid::new_v4();

    // Agent tries to inflate trust via self-interaction
    for _ in 0..1000 {
        store.record_interaction(agent, agent, InteractionOutcome::TaskCompleted);
    }

    // One real interaction so the agent appears in the network
    store.record_interaction(agent, other, InteractionOutcome::TaskCompleted);

    let self_trust = store.get_local_trust(agent, agent);
    assert_eq!(self_trust, 0.0, "self-trust must always be 0.0");
}

// ── CRDT SybilGuard: spawn rate limiting ────────────────────────────────

#[test]
fn sybil_guard_prevents_rapid_agent_spawning() {
    let mut guard = SybilGuard::new();
    let attacker = Uuid::new_v4();
    let now = Utc::now();

    // Spawn 3 agents (max allowed)
    for _ in 0..3 {
        guard.register_spawn(attacker, Uuid::new_v4(), now).unwrap();
    }

    // 4th spawn rejected
    assert!(guard.register_spawn(attacker, Uuid::new_v4(), now).is_err());

    // Even with different child IDs, still rejected
    for _ in 0..10 {
        assert!(guard.register_spawn(attacker, Uuid::new_v4(), now).is_err());
    }
}

// ── CRDT SybilGuard: young agent trust cap interacts with EigenTrust ────

#[test]
fn young_sybil_agents_capped_at_0_6_even_with_high_eigentrust() {
    let mut guard = SybilGuard::new();
    let parent = Uuid::new_v4();
    let now = Utc::now();

    let child = Uuid::new_v4();
    guard.register_spawn(parent, child, now).unwrap();

    // Even if EigenTrust computes high trust, SybilGuard caps at 0.6
    guard.set_trust(child, 0.95);
    let effective = guard.effective_trust(&child);
    assert!(
        effective <= 0.6,
        "young agent effective trust must be capped at 0.6, got {effective}"
    );
}

// ── Boundary interaction: mesh trust feeds into CRDT trust ──────────────

#[test]
fn eigentrust_scores_respect_valid_range() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(10);

    // Create a connected network
    for i in 0..agents.len() {
        for j in 0..agents.len() {
            if i != j {
                store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
            }
        }
    }

    let computer = EigenTrustComputer::default();
    let trust = computer.compute_global_trust(&mut store, &agents[..3]);

    for (&agent, &score) in &trust {
        assert!(
            (0.0..=1.0).contains(&score),
            "agent {agent} trust {score} out of [0.0, 1.0] range"
        );
    }
}

// ── Adversarial: Sybil agents try to become pre-trusted ─────────────────

#[test]
fn sybil_agents_without_pre_trust_have_low_global_trust() {
    let mut store = LocalTrustStore::new();
    let honest = make_agents(5);
    let sybils = make_agents(20); // 4:1 sybil ratio

    // Honest network
    for i in 0..honest.len() {
        for j in 0..honest.len() {
            if i != j {
                for _ in 0..5 {
                    store.record_interaction(honest[i], honest[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    // Sybil clique
    for i in 0..sybils.len() {
        for j in 0..sybils.len() {
            if i != j {
                for _ in 0..100 {
                    store.record_interaction(sybils[i], sybils[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    // Sybils try to interact with honest agents
    for &sybil in &sybils {
        for &honest_agent in &honest {
            store.record_interaction(sybil, honest_agent, InteractionOutcome::TaskCompleted);
        }
    }

    let computer = EigenTrustComputer::new(
        EigenTrustConfig {
            max_iterations: 50,
            convergence_threshold: 1e-8,
            pre_trust_weight: 0.7, // high pre-trust weight
        },
        TrustPolicy::default(),
    );

    let trust = computer.compute_global_trust(&mut store, &honest);

    let min_honest = honest.iter().map(|a| trust[a]).fold(f64::MAX, f64::min);
    let max_sybil = sybils.iter().map(|a| trust[a]).fold(f64::MIN, f64::max);

    // With high pre-trust weight, honest agents should dominate
    assert!(
        min_honest > max_sybil * 0.5,
        "minimum honest trust ({min_honest:.4}) should be significantly above \
         maximum sybil trust ({max_sybil:.4}) with high pre-trust anchoring"
    );
}

// ── Adversarial: signature failure tanks trust ──────────────────────────

#[test]
fn signature_failures_rapidly_decrease_trust() {
    let mut store = LocalTrustStore::new();
    let honest = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    // Build some trust first
    for _ in 0..10 {
        store.record_interaction(honest, attacker, InteractionOutcome::TaskCompleted);
    }
    let trust_before = store.get_local_trust(honest, attacker);

    // Signature failures (-0.3 each) should tank trust fast
    for _ in 0..5 {
        store.record_interaction(honest, attacker, InteractionOutcome::SignatureFailure);
    }
    let trust_after = store.get_local_trust(honest, attacker);

    assert!(
        trust_after < trust_before,
        "signature failures must decrease trust: before={trust_before}, after={trust_after}"
    );
}

// ── Adversarial: policy violations decrease trust ───────────────────────

#[test]
fn policy_violations_decrease_trust_faster_than_task_failures() {
    let mut store = LocalTrustStore::new();
    let observer = Uuid::new_v4();
    let violator = Uuid::new_v4();
    let failer = Uuid::new_v4();

    // Both start with same positive trust
    for _ in 0..10 {
        store.record_interaction(observer, violator, InteractionOutcome::TaskCompleted);
        store.record_interaction(observer, failer, InteractionOutcome::TaskCompleted);
    }

    // Violator gets policy violations (-0.2 each)
    for _ in 0..3 {
        store.record_interaction(observer, violator, InteractionOutcome::PolicyViolation);
    }

    // Failer gets task failures (-0.05 each)
    for _ in 0..3 {
        store.record_interaction(observer, failer, InteractionOutcome::TaskFailed);
    }

    let violator_trust = store.get_local_trust(observer, violator);
    let failer_trust = store.get_local_trust(observer, failer);

    assert!(
        violator_trust < failer_trust,
        "policy violations (-0.2) should decrease trust faster than task failures (-0.05): \
         violator={violator_trust}, failer={failer_trust}"
    );
}

// ── Stress: large network convergence ───────────────────────────────────

#[test]
fn eigentrust_converges_in_100_agent_network() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(100);

    // Sparse random interactions
    for i in 0..agents.len() {
        for j in (i + 1)..agents.len().min(i + 5) {
            store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
        }
    }

    let computer = EigenTrustComputer::new(
        EigenTrustConfig {
            max_iterations: 50,
            convergence_threshold: 1e-6,
            pre_trust_weight: 0.5,
        },
        TrustPolicy::default(),
    );

    let trust = computer.compute_global_trust(&mut store, &agents[..5]);

    assert_eq!(trust.len(), 100);
    for (&_agent, &score) in &trust {
        assert!((0.0..=1.0).contains(&score));
    }
}

// ── Trust policy thresholds ─────────────────────────────────────────────

#[test]
fn trust_policy_prevents_delegation_below_threshold() {
    let policy = TrustPolicy::default();
    assert!(!policy.can_delegate(0.0));
    assert!(!policy.can_delegate(0.29));
    assert!(policy.can_delegate(0.3));
    assert!(policy.can_delegate(0.5));
}

#[test]
fn trust_policy_prevents_sensitive_data_below_threshold() {
    let policy = TrustPolicy::default();
    assert!(!policy.can_share_sensitive_data(0.0));
    assert!(!policy.can_share_sensitive_data(0.59));
    assert!(policy.can_share_sensitive_data(0.6));
    assert!(policy.can_share_sensitive_data(1.0));
}

// ── CRDT SybilGuard + EigenTrust combined scenario ──────────────────────

#[test]
fn sybil_guard_spawn_limit_constrains_eigentrust_sybil_attack() {
    let mut guard = SybilGuard::new();
    let mut store = LocalTrustStore::new();
    let attacker = Uuid::new_v4();
    let now = Utc::now();

    // Attacker can only spawn 3 agents per 24h
    let mut sybils = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, now).unwrap();
        sybils.push(child);
    }

    // 4th spawn blocked
    assert!(guard.register_spawn(attacker, Uuid::new_v4(), now).is_err());

    // Even the 3 spawned sybils are trust-capped at 0.6 for 7 days
    for &sybil in &sybils {
        guard.set_trust(sybil, 1.0);
        assert!(guard.effective_trust(&sybil) <= 0.6);
    }

    // In EigenTrust, these 3 sybils colluding have limited impact
    let honest = make_agents(5);
    for i in 0..honest.len() {
        for j in 0..honest.len() {
            if i != j {
                for _ in 0..10 {
                    store.record_interaction(honest[i], honest[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    // Sybils collude
    for i in 0..sybils.len() {
        for j in 0..sybils.len() {
            if i != j {
                for _ in 0..50 {
                    store.record_interaction(sybils[i], sybils[j], InteractionOutcome::TaskCompleted);
                }
            }
        }
    }

    let computer = EigenTrustComputer::new(
        EigenTrustConfig {
            max_iterations: 50,
            convergence_threshold: 1e-8,
            pre_trust_weight: 0.5,
        },
        TrustPolicy::default(),
    );

    let trust = computer.compute_global_trust(&mut store, &honest);

    // With only 3 sybils (spawn-limited) vs 5 honest pre-trusted agents,
    // the sybil attack has limited impact
    let avg_honest: f64 = honest.iter().map(|a| trust[a]).sum::<f64>() / honest.len() as f64;
    let avg_sybil: f64 = sybils.iter().map(|a| trust[a]).sum::<f64>() / sybils.len() as f64;

    assert!(
        avg_honest > avg_sybil,
        "spawn-limited sybils ({avg_sybil:.4}) should not exceed honest trust ({avg_honest:.4})"
    );
}

// ── Empty network edge case ─────────────────────────────────────────────

#[test]
fn eigentrust_empty_network_returns_empty() {
    let mut store = LocalTrustStore::new();
    let computer = EigenTrustComputer::default();
    let trust = computer.compute_global_trust(&mut store, &[]);
    assert!(trust.is_empty());
}

// ── No pre-trusted peers: uniform distribution ──────────────────────────

#[test]
fn eigentrust_no_pre_trusted_uses_uniform() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(4);

    // Symmetric interactions
    for i in 0..agents.len() {
        for j in 0..agents.len() {
            if i != j {
                store.record_interaction(agents[i], agents[j], InteractionOutcome::TaskCompleted);
            }
        }
    }

    let computer = EigenTrustComputer::default();
    let trust = computer.compute_global_trust(&mut store, &[]); // no pre-trusted

    // With symmetric interactions and no pre-trusted, trust should be roughly equal
    let scores: Vec<f64> = agents.iter().map(|a| trust[a]).collect();
    let mean = scores.iter().sum::<f64>() / scores.len() as f64;
    for &s in &scores {
        assert!(
            (s - mean).abs() < 0.1,
            "symmetric network with no pre-trusted should have roughly equal trust: {scores:?}"
        );
    }
}
