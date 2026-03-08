//! Adversarial: Kill gate quorum race.
//!
//! Distributed resume now fails closed unless authenticated cluster membership
//! is configured. These tests verify the disabled-by-default guard and keep
//! one authenticated positive control so the quorum path still has coverage.

use chrono::Utc;
use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::{GateState, KillGate};
use ghost_kill_gates::quorum::{QuorumTracker, ResumeVote};
use uuid::Uuid;

fn make_vote(node_id: Uuid) -> ResumeVote {
    ResumeVote {
        node_id,
        reason: "resume request".into(),
        initiated_by: "test".into(),
        voted_at: Utc::now(),
    }
}

// ── Quorum deduplication: same node_id counted once ─────────────────────

#[test]
fn quorum_tracker_deduplicates_by_node_id() {
    let mut tracker = QuorumTracker::new(3);
    let node = Uuid::new_v4();

    for _ in 0..100 {
        tracker.cast_vote(make_vote(node));
    }

    assert_eq!(
        tracker.vote_count(),
        1,
        "100 votes from same node should count as 1"
    );
    assert!(!tracker.has_quorum());
}

#[test]
fn quorum_requires_distinct_nodes() {
    let mut tracker = QuorumTracker::new(3);

    let nodes: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
    for &node in &nodes {
        tracker.cast_vote(make_vote(node));
    }

    assert_eq!(tracker.vote_count(), 3);
    assert!(tracker.has_quorum());
}

// ── Sybil quorum attack: attacker creates fake node_ids ─────────────────

/// Fake node IDs are rejected outright while authenticated cluster membership
/// is disabled. This keeps the old Sybil path fail-closed by default.
#[test]
fn sybil_fake_node_ids_cannot_reach_quorum_without_authenticated_membership() {
    let gate = KillGate::new(Uuid::new_v4(), KillGateConfig::default());
    gate.close("sybil test".into());

    let cluster_size = 5; // quorum = 3

    // Attacker generates 3 fake node_ids
    let fake_nodes: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();

    let mut reached = false;
    for &fake in &fake_nodes {
        reached = gate.cast_resume_vote(make_vote(fake), cluster_size);
    }

    assert!(
        !reached,
        "resume must stay disabled until authenticated cluster membership is configured"
    );
    assert_eq!(gate.state(), GateState::GateClosed);
}

// ── Mitigation: quorum with known cluster members ───────────────────────

/// If we filter votes to only known cluster members, sybil votes are rejected.
/// This simulates the mitigation that SHOULD exist.
#[test]
fn filtered_quorum_rejects_unknown_nodes() {
    let cluster_members: Vec<Uuid> = (0..5).map(|_| Uuid::new_v4()).collect();
    let mut tracker = QuorumTracker::new(3);

    // Attacker submits votes with fake node_ids
    let fake_nodes: Vec<Uuid> = (0..10).map(|_| Uuid::new_v4()).collect();

    for &fake in &fake_nodes {
        // Simulate membership check: only accept votes from known members
        if cluster_members.contains(&fake) {
            tracker.cast_vote(make_vote(fake));
        }
    }

    assert_eq!(tracker.vote_count(), 0, "no fake votes should be accepted");
    assert!(!tracker.has_quorum());
}

// ── Race condition: votes vs trust degradation ──────────────────────────

/// The propagation race no longer matters while resume votes are disabled
/// without authenticated cluster membership.
#[test]
fn race_window_cannot_bypass_disabled_resume_guard() {
    let gate = KillGate::new(Uuid::new_v4(), KillGateConfig::default());
    gate.close("race test".into());

    // Propagation timeout is 500ms by default
    let config = KillGateConfig::default();
    assert_eq!(
        config.max_propagation.as_millis(),
        500,
        "propagation timeout should be 500ms"
    );

    // The race window is at most max_propagation (500ms).
    // Verify the gate is in GateClosed state (not yet Confirmed)
    assert_eq!(gate.state(), GateState::GateClosed);

    // Sybil votes still fail closed before confirmation.
    let cluster_size = 3; // quorum = 2
    let sybil_a = Uuid::new_v4();
    let sybil_b = Uuid::new_v4();

    gate.cast_resume_vote(make_vote(sybil_a), cluster_size);
    let reached = gate.cast_resume_vote(make_vote(sybil_b), cluster_size);

    assert!(
        !reached,
        "resume votes must stay rejected during the propagation window when membership is unauthenticated"
    );
    assert_eq!(gate.state(), GateState::GateClosed);
}

// ── Effective quorum calculation ────────────────────────────────────────

#[test]
fn quorum_is_majority_plus_one() {
    let config = KillGateConfig::default();

    assert_eq!(config.effective_quorum(1), 1);
    assert_eq!(config.effective_quorum(2), 2);
    assert_eq!(config.effective_quorum(3), 2);
    assert_eq!(config.effective_quorum(4), 3);
    assert_eq!(config.effective_quorum(5), 3);
    assert_eq!(config.effective_quorum(10), 6);
}

#[test]
fn quorum_zero_cluster_returns_one() {
    let config = KillGateConfig::default();
    assert_eq!(config.effective_quorum(0), 1);
}

#[test]
fn custom_quorum_clamped_to_cluster_size() {
    let config = KillGateConfig {
        quorum_size: Some(100),
        ..KillGateConfig::default()
    };
    // Custom quorum of 100 clamped to cluster size of 5
    assert_eq!(config.effective_quorum(5), 5);
}

#[test]
fn custom_quorum_minimum_one() {
    let config = KillGateConfig {
        quorum_size: Some(0),
        ..KillGateConfig::default()
    };
    assert_eq!(config.effective_quorum(5), 1);
}

// ── Gate state transitions ──────────────────────────────────────────────

#[test]
fn gate_transitions_through_expected_states() {
    let node = Uuid::new_v4();
    let gate = KillGate::new(node, KillGateConfig::default());

    assert_eq!(gate.state(), GateState::Normal);

    gate.close("test".into());
    assert_eq!(gate.state(), GateState::GateClosed);

    gate.begin_propagation();
    // Transitions to Propagating
    assert_eq!(gate.state(), GateState::Propagating);

    // Ack from one peer in a 2-node cluster
    let peer = Uuid::new_v4();
    let all_acked = gate.record_ack(peer, 2);
    assert!(all_acked);
    assert_eq!(gate.state(), GateState::Confirmed);
}

// ── Chain integrity after resume ────────────────────────────────────────

#[test]
fn authenticated_quorum_records_close_and_resume_events() {
    let mut config = KillGateConfig::default();
    config.authenticated_cluster_membership = true;
    let gate = KillGate::new(Uuid::new_v4(), config);

    gate.close("chain test".into());
    let chain_after_close = gate.chain().len();
    assert!(chain_after_close >= 1);

    let voter_a = Uuid::new_v4();
    let voter_b = Uuid::new_v4();
    assert!(!gate.cast_resume_vote(make_vote(voter_a), 3));
    assert!(gate.cast_resume_vote(make_vote(voter_b), 3));

    let chain_after_resume = gate.chain();
    assert!(
        chain_after_resume.len() > chain_after_close,
        "resume should add events to the chain"
    );
    assert!(
        chain_after_resume.iter().any(|event| matches!(
            event.event_type,
            ghost_kill_gates::chain::GateEventType::ResumeConfirmed
        )),
        "authenticated quorum resume should record a ResumeConfirmed event"
    );
    assert_eq!(gate.state(), GateState::Normal);
}

// ── Duplicate ack from same peer ────────────────────────────────────────

#[test]
fn duplicate_ack_from_same_peer_counted_once() {
    let gate = KillGate::new(Uuid::new_v4(), KillGateConfig::default());
    gate.close("dedup ack test".into());
    gate.begin_propagation();

    let peer = Uuid::new_v4();
    let cluster_size = 3; // need 2 acks

    // Same peer acks 10 times
    for _ in 0..10 {
        gate.record_ack(peer, cluster_size);
    }

    // Should NOT be confirmed (only 1 unique peer acked, need 2)
    assert_ne!(
        gate.state(),
        GateState::Confirmed,
        "duplicate acks from same peer should not confirm"
    );
}
