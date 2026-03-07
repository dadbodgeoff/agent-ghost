//! E2E: Multi-agent consensus lifecycle.
//!
//! Validates cortex-multiagent ConsensusShield for cross-agent state changes.

use cortex_multiagent::{
    consensus::{ConsensusConfig, ConsensusResult, Vote},
    ConsensusShield,
};
use uuid::Uuid;

/// Full consensus lifecycle: start → vote → approve.
#[test]
fn consensus_approve_lifecycle() {
    let config = ConsensusConfig {
        required_approvals: 2,
        total_participants: 3,
        timeout_seconds: 300,
    };
    let mut shield = ConsensusShield::new(config);
    let proposal = Uuid::now_v7();
    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    shield.start_round(proposal);

    // First vote — pending
    let r = shield.vote(proposal, agents[0], Vote::Approve);
    assert!(matches!(r, ConsensusResult::Pending { approvals: 1, .. }));

    // Second vote — approved (2 of 3)
    let r = shield.vote(proposal, agents[1], Vote::Approve);
    assert_eq!(r, ConsensusResult::Approved);
}

/// Consensus rejection when too many rejections.
#[test]
fn consensus_reject_lifecycle() {
    let config = ConsensusConfig {
        required_approvals: 2,
        total_participants: 3,
        timeout_seconds: 300,
    };
    let mut shield = ConsensusShield::new(config);
    let proposal = Uuid::now_v7();
    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    shield.start_round(proposal);

    shield.vote(proposal, agents[0], Vote::Reject);
    let r = shield.vote(proposal, agents[1], Vote::Reject);
    assert_eq!(r, ConsensusResult::Rejected);
}

/// Mixed votes: 1 approve, 1 abstain, 1 approve → approved.
#[test]
fn consensus_with_abstain() {
    let config = ConsensusConfig {
        required_approvals: 2,
        total_participants: 3,
        timeout_seconds: 300,
    };
    let mut shield = ConsensusShield::new(config);
    let proposal = Uuid::now_v7();
    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    shield.start_round(proposal);

    shield.vote(proposal, agents[0], Vote::Approve);
    shield.vote(proposal, agents[1], Vote::Abstain);
    let r = shield.vote(proposal, agents[2], Vote::Approve);
    assert_eq!(r, ConsensusResult::Approved);
}

/// Vote on unknown proposal → rejected.
#[test]
fn vote_on_unknown_proposal_rejected() {
    let mut shield = ConsensusShield::default();
    let r = shield.vote(Uuid::now_v7(), Uuid::now_v7(), Vote::Approve);
    assert_eq!(r, ConsensusResult::Rejected);
}

/// Multiple concurrent proposals tracked independently.
#[test]
fn concurrent_proposals_independent() {
    let config = ConsensusConfig {
        required_approvals: 2,
        total_participants: 3,
        timeout_seconds: 300,
    };
    let mut shield = ConsensusShield::new(config);

    let proposal_a = Uuid::now_v7();
    let proposal_b = Uuid::now_v7();
    let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

    shield.start_round(proposal_a);
    shield.start_round(proposal_b);

    // Approve A, reject B
    shield.vote(proposal_a, agents[0], Vote::Approve);
    shield.vote(proposal_a, agents[1], Vote::Approve);

    shield.vote(proposal_b, agents[0], Vote::Reject);
    shield.vote(proposal_b, agents[1], Vote::Reject);

    assert_eq!(shield.evaluate(proposal_a), ConsensusResult::Approved);
    assert_eq!(shield.evaluate(proposal_b), ConsensusResult::Rejected);
}
