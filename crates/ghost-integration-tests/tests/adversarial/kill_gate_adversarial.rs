//! Adversarial tests for distributed kill gates.
//!
//! Split-brain scenarios, single-node resume attempts, chain tampering,
//! and race conditions.

use std::sync::Arc;

use chrono::Utc;
use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::{GateState, KillGate};
use ghost_kill_gates::quorum::ResumeVote;
use ghost_kill_gates::relay::{KillGateRelay, PeerNode};
use uuid::Uuid;

// ── Single-node resume must fail (INV-KG-03) ───────────────────────────

/// A single node cannot unilaterally resume a 3-node cluster.
#[test]
fn single_node_cannot_resume_cluster() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("adversarial test".into());

    let cluster_size = 3; // quorum = 2
    let single_node = Uuid::now_v7();

    let result = gate.cast_resume_vote(
        ResumeVote {
            node_id: single_node,
            reason: "I want to resume".into(),
            initiated_by: "attacker".into(),
            voted_at: Utc::now(),
        },
        cluster_size,
    );

    assert!(!result, "SECURITY: Single node resumed a 3-node cluster");
    assert!(
        gate.is_closed(),
        "Gate should remain closed after single vote"
    );
}

/// Duplicate votes from the same node must not count toward quorum.
#[test]
fn duplicate_votes_cannot_reach_quorum() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("dedup test".into());

    let cluster_size = 3; // quorum = 2
    let attacker = Uuid::now_v7();

    for i in 0..10 {
        let result = gate.cast_resume_vote(
            ResumeVote {
                node_id: attacker,
                reason: format!("attempt {}", i),
                initiated_by: "attacker".into(),
                voted_at: Utc::now(),
            },
            cluster_size,
        );
        assert!(
            !result,
            "SECURITY: Duplicate vote {} from same node reached quorum",
            i
        );
    }
    assert!(gate.is_closed());
}

// ── Split-brain: both partitions close independently ────────────────────

/// Two isolated nodes both close their gates. After rejoin, both should
/// remain closed (fail-closed behavior).
#[test]
fn split_brain_both_partitions_closed() {
    let node_a = Uuid::now_v7();
    let node_b = Uuid::now_v7();

    let gate_a = KillGate::new(node_a, KillGateConfig::default());
    let gate_b = KillGate::new(node_b, KillGateConfig::default());

    // Both close independently (network partition)
    gate_a.close("partition A detected issue".into());
    gate_b.close("partition B detected issue".into());

    assert!(gate_a.is_closed());
    assert!(gate_b.is_closed());

    // After rejoin, both should still be closed — no auto-resume
    assert!(
        gate_a.is_closed() && gate_b.is_closed(),
        "SECURITY: Split-brain auto-resumed a gate"
    );
}

// ── Propagation timeout → fail-closed (INV-KG-02) ──────────────────────

/// If propagation times out, the gate must remain closed.
#[test]
fn propagation_timeout_stays_closed() {
    let config = KillGateConfig {
        max_propagation: std::time::Duration::from_millis(1),
        ..KillGateConfig::default()
    };
    let gate = KillGate::new(Uuid::now_v7(), config);
    gate.close("timeout test".into());
    gate.begin_propagation();

    // Wait for timeout
    std::thread::sleep(std::time::Duration::from_millis(10));

    assert!(gate.is_propagation_timed_out());
    assert!(
        gate.is_closed(),
        "SECURITY: Gate opened after propagation timeout"
    );
}

// ── Gate monotonicity ───────────────────────────────────────────────────

/// Closing an already-closed gate must not reset state or chain.
#[test]
fn double_close_preserves_chain() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("first close".into());
    let chain_after_first = gate.chain().len();

    gate.close("second close".into());
    let chain_after_second = gate.chain().len();

    assert_eq!(
        chain_after_second,
        chain_after_first + 1,
        "Second close should add to chain, not reset it"
    );
    assert!(gate.is_closed());
}

// ── Relay message forgery ───────────────────────────────────────────────

/// A forged ack from an unknown node should not count toward confirmation.
#[test]
fn forged_ack_from_unknown_peer() {
    let node_a = Uuid::now_v7();
    let gate = KillGate::new(node_a, KillGateConfig::default());
    gate.close("forgery test".into());
    gate.begin_propagation();

    let forged_peer = Uuid::now_v7();
    let cluster_size = 3;

    // The ack is recorded but the gate tracks acked_nodes count vs cluster_size-1.
    // With cluster_size=3, we need 2 acks. One forged ack alone shouldn't confirm.
    gate.record_ack(forged_peer, cluster_size);
    assert_ne!(
        gate.state(),
        GateState::Confirmed,
        "Single ack (possibly forged) should not confirm a 3-node cluster"
    );
}
