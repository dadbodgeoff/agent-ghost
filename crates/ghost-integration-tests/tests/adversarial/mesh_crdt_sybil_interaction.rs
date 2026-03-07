//! Adversarial stress tests: EigenTrust × cortex-crdt Sybil defense interaction.
//!
//! Tests the boundary where mesh trust meets CRDT sybil defense.

use chrono::Utc;
use cortex_crdt::sybil::SybilGuard;
use ghost_mesh::trust::eigentrust::{EigenTrustComputer, EigenTrustConfig, TrustPolicy};
use ghost_mesh::trust::local_trust::{InteractionOutcome, LocalTrustStore};
use uuid::Uuid;

fn make_agents(n: usize) -> Vec<Uuid> {
    (0..n).map(|_| Uuid::new_v4()).collect()
}

// ── Sybil colluding agents bounded by pre-trusted anchoring ─────────────

#[test]
fn sybil_colluding_agents_trust_bounded() {
    let mut store = LocalTrustStore::new();
    let honest = make_agents(3);
    let sybils = make_agents(5);

    for i in 0..honest.len() {
        for j in 0..honest.len() {
            if i != j {
                for _ in 0..10 {
                    store.record_interaction(
                        honest[i],
                        honest[j],
                        InteractionOutcome::TaskCompleted,
                    );
                }
            }
        }
    }

    for i in 0..sybils.len() {
        for j in 0..sybils.len() {
            if i != j {
                for _ in 0..50 {
                    store.record_interaction(
                        sybils[i],
                        sybils[j],
                        InteractionOutcome::TaskCompleted,
                    );
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
    let avg_honest: f64 = honest.iter().map(|a| trust[a]).sum::<f64>() / honest.len() as f64;
    let avg_sybil: f64 = sybils.iter().map(|a| trust[a]).sum::<f64>() / sybils.len() as f64;

    assert!(avg_honest > avg_sybil);
}

// ── SybilGuard spawn limit constrains EigenTrust attack ─────────────────

#[test]
fn spawn_limit_constrains_eigentrust_sybil_attack() {
    let mut guard = SybilGuard::new();
    let mut store = LocalTrustStore::new();
    let attacker = Uuid::new_v4();
    let now = Utc::now();

    let mut sybils = Vec::new();
    for _ in 0..3 {
        let child = Uuid::new_v4();
        guard.register_spawn(attacker, child, now).unwrap();
        sybils.push(child);
    }
    assert!(guard.register_spawn(attacker, Uuid::new_v4(), now).is_err());

    // Young sybils capped at 0.6
    for &sybil in &sybils {
        guard.set_trust(sybil, 1.0);
        assert!(guard.effective_trust(&sybil) <= 0.6);
    }

    let honest = make_agents(5);
    for i in 0..honest.len() {
        for j in 0..honest.len() {
            if i != j {
                for _ in 0..10 {
                    store.record_interaction(
                        honest[i],
                        honest[j],
                        InteractionOutcome::TaskCompleted,
                    );
                }
            }
        }
    }

    for i in 0..sybils.len() {
        for j in 0..sybils.len() {
            if i != j {
                for _ in 0..50 {
                    store.record_interaction(
                        sybils[i],
                        sybils[j],
                        InteractionOutcome::TaskCompleted,
                    );
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
    let avg_honest: f64 = honest.iter().map(|a| trust[a]).sum::<f64>() / honest.len() as f64;
    let avg_sybil: f64 = sybils.iter().map(|a| trust[a]).sum::<f64>() / sybils.len() as f64;

    assert!(avg_honest > avg_sybil);
}

// ── Signature failures tank trust ───────────────────────────────────────

#[test]
fn signature_failures_rapidly_decrease_trust() {
    let mut store = LocalTrustStore::new();
    let honest = Uuid::new_v4();
    let attacker = Uuid::new_v4();

    for _ in 0..10 {
        store.record_interaction(honest, attacker, InteractionOutcome::TaskCompleted);
    }
    let trust_before = store.get_local_trust(honest, attacker);

    for _ in 0..5 {
        store.record_interaction(honest, attacker, InteractionOutcome::SignatureFailure);
    }
    let trust_after = store.get_local_trust(honest, attacker);

    assert!(trust_after < trust_before);
}

// ── Large network convergence ───────────────────────────────────────────

#[test]
fn eigentrust_converges_in_100_agent_network() {
    let mut store = LocalTrustStore::new();
    let agents = make_agents(100);

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
    for &score in trust.values() {
        assert!((0.0..=1.0).contains(&score));
    }
}
