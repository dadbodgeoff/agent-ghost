//! Adversarial: Kill gate quorum race.
//!
//! Can a sufficiently fast Sybil cluster submit enough resume votes before
//! signature failure trust degradation fires? The QuorumTracker deduplicates
//! by node_id (BTreeSet), so the question is whether an attacker can
//! register enough distinct node_ids to reach quorum.

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

/// Attacker generates N fake node_ids and submits resume votes.
/// If N >= quorum threshold, the gate reopens.
///
/// KEY FINDING: QuorumTracker does NOT verify that node_ids correspond
/// to real cluster members. Any UUID is accepted as a vote.
#[test]
fn sybil_fake_node_ids_can_reach_quorum() {
    let gate = KillGate::new(Uuid::new_v4(), KillGateConfig::default());
    gate.close("sybil test".into());

    let cluster_size = 5; // quorum = 3

    // Attacker generates 3 fake node_ids
    let fake_nodes: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();

    let mut reached = false;
    for &fake in &fake_nodes {
        reached = gate.cast_resume_vote(make_vote(fake), cluster_size);
    }

    // This SUCCEEDS — the gate reopens because QuorumTracker accepts any UUID.
    // This is the vulnerability: no membership verification on resume votes.
    assert!(
        reached,
        "KNOWN VULNERABILITY: fake node_ids reached quorum — \
         QuorumTracker does not verify cluster membership"
    );
    assert_eq!(gate.state(), GateState::Normal);
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

/// The attack window: time between gate close and trust degradation
/// propagating to all nodes. If sybil votes arrive before trust
/// degradation fires, the gate reopens.
#[test]
fn race_window_between_close_and_trust_degradation() {
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
    // If sybil votes arrive within this window, they can reach quorum
    // before the gate is confirmed across the cluster.

    // Verify the gate is in GateClosed state (not yet Confirmed)
    assert_eq!(gate.state(), GateState::GateClosed);

    // Sybil votes can arrive before confirmation
    let cluster_size = 3; // quorum = 2
    let sybil_a = Uuid::new_v4();
    let sybil_b = Uuid::new_v4();

    gate.cast_resume_vote(make_vote(sybil_a), cluster_size);
    let reached = gate.cast_resume_vote(make_vote(sybil_b), cluster_size);

    assert!(
        reached,
        "sybil votes during propagation window can reach quorum"
    );
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
fn chain_records_close_and_resume_events() {
    let gate = KillGate::new(Uuid::new_v4(), KillGateConfig::default());

    gate.close("chain test".into());
    let chain_after_close = gate.chain().len();
    assert!(chain_after_close >= 1);

    // Resume via quorum (2-node cluster, quorum=2, but we use cluster_size=1 for simplicity)
    let voter = Uuid::new_v4();
    gate.cast_resume_vote(make_vote(voter), 1);

    let chain_after_resume = gate.chain().len();
    assert!(
        chain_after_resume > chain_after_close,
        "resume should add events to the chain"
    );
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
