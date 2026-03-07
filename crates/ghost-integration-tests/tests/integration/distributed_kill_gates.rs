//! Integration tests for distributed kill gates.
//!
//! Validates gate state machine, hash chain integrity, quorum resume,
//! and bridge wiring between KillSwitch and KillGate.

use std::sync::Arc;

use chrono::Utc;
use ghost_kill_gates::chain::{compute_gate_event_hash, GateEventType, GENESIS_HASH};
use ghost_kill_gates::config::KillGateConfig;
use ghost_kill_gates::gate::{GateState, KillGate};
use ghost_kill_gates::quorum::{QuorumTracker, ResumeVote};
use ghost_kill_gates::relay::{GateRelayMessage, KillGateRelay, PeerNode};
use uuid::Uuid;

// ── Gate state machine ──────────────────────────────────────────────────

#[test]
fn gate_starts_normal() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    assert_eq!(gate.state(), GateState::Normal);
    assert!(!gate.is_closed());
}

#[test]
fn gate_close_transitions_to_closed() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("test kill".into());
    assert_eq!(gate.state(), GateState::GateClosed);
    assert!(gate.is_closed());
}

#[test]
fn gate_close_records_chain_event() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    let event = gate.close("audit test".into());
    assert_eq!(event.event_type, GateEventType::Close);
    assert_eq!(event.previous_hash, GENESIS_HASH);

    let chain = gate.chain();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].event_hash, event.event_hash);
}

#[test]
fn gate_propagation_state_transition() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("propagation test".into());
    gate.begin_propagation();
    assert_eq!(gate.state(), GateState::Propagating);
    assert!(gate.is_closed());
}

#[test]
fn gate_ack_all_peers_transitions_to_confirmed() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("confirm test".into());
    gate.begin_propagation();

    let peer1 = Uuid::now_v7();
    let peer2 = Uuid::now_v7();
    let cluster_size = 3; // self + 2 peers

    assert!(!gate.record_ack(peer1, cluster_size));
    assert!(gate.record_ack(peer2, cluster_size));
    assert_eq!(gate.state(), GateState::Confirmed);
}

// ── Quorum resume ───────────────────────────────────────────────────────

#[test]
fn quorum_requires_majority() {
    let mut tracker = QuorumTracker::new(3);
    let node1 = Uuid::now_v7();
    let node2 = Uuid::now_v7();
    let node3 = Uuid::now_v7();

    assert!(!tracker.cast_vote(ResumeVote {
        node_id: node1,
        reason: "test".into(),
        initiated_by: "admin".into(),
        voted_at: Utc::now(),
    }));
    assert!(!tracker.cast_vote(ResumeVote {
        node_id: node2,
        reason: "test".into(),
        initiated_by: "admin".into(),
        voted_at: Utc::now(),
    }));
    assert!(tracker.cast_vote(ResumeVote {
        node_id: node3,
        reason: "test".into(),
        initiated_by: "admin".into(),
        voted_at: Utc::now(),
    }));
    assert!(tracker.has_quorum());
}

#[test]
fn duplicate_votes_not_double_counted() {
    let mut tracker = QuorumTracker::new(2);
    let node1 = Uuid::now_v7();

    tracker.cast_vote(ResumeVote {
        node_id: node1,
        reason: "test".into(),
        initiated_by: "admin".into(),
        voted_at: Utc::now(),
    });
    tracker.cast_vote(ResumeVote {
        node_id: node1,
        reason: "test again".into(),
        initiated_by: "admin".into(),
        voted_at: Utc::now(),
    });

    assert_eq!(tracker.vote_count(), 1);
    assert!(!tracker.has_quorum());
}

#[test]
fn gate_resume_via_quorum() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("resume test".into());

    let cluster_size = 3;
    let node1 = Uuid::now_v7();
    let node2 = Uuid::now_v7();

    assert!(!gate.cast_resume_vote(
        ResumeVote {
            node_id: node1,
            reason: "safe".into(),
            initiated_by: "admin".into(),
            voted_at: Utc::now(),
        },
        cluster_size,
    ));
    assert!(gate.cast_resume_vote(
        ResumeVote {
            node_id: node2,
            reason: "safe".into(),
            initiated_by: "admin".into(),
            voted_at: Utc::now(),
        },
        cluster_size,
    ));
    assert_eq!(gate.state(), GateState::Normal);
    assert!(!gate.is_closed());
}

// ── Hash chain integrity ────────────────────────────────────────────────

#[test]
fn chain_events_are_linked() {
    let gate = KillGate::new(Uuid::now_v7(), KillGateConfig::default());
    gate.close("event 1".into());
    gate.close("event 2".into());

    let chain = gate.chain();
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].previous_hash, GENESIS_HASH);
    assert_eq!(chain[1].previous_hash, chain[0].event_hash);
}

#[test]
fn chain_hash_is_deterministic() {
    let node_id = Uuid::now_v7();
    let now = Utc::now();
    let payload = r#"{"reason":"test"}"#;

    let h1 = compute_gate_event_hash(GateEventType::Close, &node_id, &now, payload, &GENESIS_HASH);
    let h2 = compute_gate_event_hash(GateEventType::Close, &node_id, &now, payload, &GENESIS_HASH);
    assert_eq!(h1, h2);
}

// ── Relay message processing ────────────────────────────────────────────

#[test]
fn relay_propagates_close_to_peer() {
    let node_a = Uuid::now_v7();
    let node_b = Uuid::now_v7();

    let gate_a = Arc::new(KillGate::new(node_a, KillGateConfig::default()));
    let gate_b = Arc::new(KillGate::new(node_b, KillGateConfig::default()));

    let mut relay_a = KillGateRelay::new(Arc::clone(&gate_a));
    let mut relay_b = KillGateRelay::new(Arc::clone(&gate_b));

    relay_a.add_peer(PeerNode {
        node_id: node_b,
        endpoint: "http://b".into(),
        last_heartbeat: None,
        is_alive: true,
    });
    relay_b.add_peer(PeerNode {
        node_id: node_a,
        endpoint: "http://a".into(),
        last_heartbeat: None,
        is_alive: true,
    });

    // Node A closes gate
    let event = gate_a.close("critical failure".into());
    let msg = relay_a.build_close_notification(event);

    // Node B receives and processes
    let ack = relay_b.process_message(msg);
    assert!(
        gate_b.is_closed(),
        "Peer gate should be closed after propagation"
    );
    assert!(ack.is_some(), "Peer should send ack");

    // Node A receives ack
    if let Some(ack_msg) = ack {
        relay_a.process_message(ack_msg);
    }
}

// ── Config ──────────────────────────────────────────────────────────────

#[test]
fn effective_quorum_auto_calculation() {
    let config = KillGateConfig::default();
    assert_eq!(config.effective_quorum(1), 1);
    assert_eq!(config.effective_quorum(2), 2);
    assert_eq!(config.effective_quorum(3), 2);
    assert_eq!(config.effective_quorum(4), 3);
    assert_eq!(config.effective_quorum(5), 3);
    assert_eq!(config.effective_quorum(0), 1);
}

#[test]
fn effective_quorum_manual_override() {
    let config = KillGateConfig {
        quorum_size: Some(5),
        ..KillGateConfig::default()
    };
    assert_eq!(config.effective_quorum(10), 5);
    // Clamped to cluster size
    assert_eq!(config.effective_quorum(3), 3);
}

// ── Snapshot ────────────────────────────────────────────────────────────

#[test]
fn snapshot_reflects_current_state() {
    let node_id = Uuid::now_v7();
    let gate = KillGate::new(node_id, KillGateConfig::default());

    let snap = gate.snapshot();
    assert_eq!(snap.state, GateState::Normal);
    assert_eq!(snap.node_id, node_id);
    assert!(snap.closed_at.is_none());
    assert_eq!(snap.chain_length, 0);

    gate.close("snapshot test".into());
    let snap = gate.snapshot();
    assert_eq!(snap.state, GateState::GateClosed);
    assert!(snap.closed_at.is_some());
    assert_eq!(snap.chain_length, 1);
}
