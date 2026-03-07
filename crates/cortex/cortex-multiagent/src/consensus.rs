//! ConsensusShield: N-of-M multi-source validation.
//!
//! Requires agreement from N out of M agents before accepting
//! cross-agent state changes (memory writes, goal changes, etc.).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A vote from an agent on a proposed change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Vote {
    Approve,
    Reject,
    Abstain,
}

/// Configuration for consensus requirements.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsensusConfig {
    /// Minimum approvals required (N).
    pub required_approvals: usize,
    /// Total participants (M).
    pub total_participants: usize,
    /// Timeout in seconds before auto-rejecting.
    pub timeout_seconds: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            required_approvals: 2,
            total_participants: 3,
            timeout_seconds: 300,
        }
    }
}

/// Result of consensus evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsensusResult {
    /// Sufficient approvals received.
    Approved,
    /// Too many rejections — cannot reach quorum.
    Rejected,
    /// Still waiting for votes.
    Pending { approvals: usize, rejections: usize },
}

/// Tracks votes for a specific proposal across agents.
#[derive(Debug, Clone)]
pub struct ConsensusRound {
    pub proposal_id: Uuid,
    pub config: ConsensusConfig,
    pub votes: BTreeMap<Uuid, Vote>,
}

/// ConsensusShield: multi-source validation gate.
pub struct ConsensusShield {
    config: ConsensusConfig,
    rounds: BTreeMap<Uuid, ConsensusRound>,
}

impl ConsensusShield {
    pub fn new(config: ConsensusConfig) -> Self {
        Self {
            config,
            rounds: BTreeMap::new(),
        }
    }

    /// Start a new consensus round for a proposal.
    pub fn start_round(&mut self, proposal_id: Uuid) {
        self.rounds.insert(
            proposal_id,
            ConsensusRound {
                proposal_id,
                config: self.config.clone(),
                votes: BTreeMap::new(),
            },
        );
    }

    /// Record a vote from an agent.
    pub fn vote(&mut self, proposal_id: Uuid, agent_id: Uuid, vote: Vote) -> ConsensusResult {
        let round = match self.rounds.get_mut(&proposal_id) {
            Some(r) => r,
            None => return ConsensusResult::Rejected,
        };

        round.votes.insert(agent_id, vote);
        self.evaluate(proposal_id)
    }

    /// Evaluate current consensus state.
    pub fn evaluate(&self, proposal_id: Uuid) -> ConsensusResult {
        let round = match self.rounds.get(&proposal_id) {
            Some(r) => r,
            None => return ConsensusResult::Rejected,
        };

        let approvals = round
            .votes
            .values()
            .filter(|v| **v == Vote::Approve)
            .count();
        let rejections = round.votes.values().filter(|v| **v == Vote::Reject).count();

        if approvals >= round.config.required_approvals {
            ConsensusResult::Approved
        } else if rejections > round.config.total_participants - round.config.required_approvals {
            ConsensusResult::Rejected
        } else {
            ConsensusResult::Pending {
                approvals,
                rejections,
            }
        }
    }
}

impl Default for ConsensusShield {
    fn default() -> Self {
        Self::new(ConsensusConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consensus_requires_n_of_m_agreement() {
        let config = ConsensusConfig {
            required_approvals: 2,
            total_participants: 3,
            timeout_seconds: 300,
        };
        let mut shield = ConsensusShield::new(config);
        let proposal = Uuid::now_v7();
        let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

        shield.start_round(proposal);

        // First approval — still pending
        let r = shield.vote(proposal, agents[0], Vote::Approve);
        assert!(matches!(r, ConsensusResult::Pending { approvals: 1, .. }));

        // Second approval — approved
        let r = shield.vote(proposal, agents[1], Vote::Approve);
        assert_eq!(r, ConsensusResult::Approved);
    }

    #[test]
    fn consensus_rejected_when_too_many_rejections() {
        let config = ConsensusConfig {
            required_approvals: 2,
            total_participants: 3,
            timeout_seconds: 300,
        };
        let mut shield = ConsensusShield::new(config);
        let proposal = Uuid::now_v7();
        let agents: Vec<Uuid> = (0..3).map(|_| Uuid::now_v7()).collect();

        shield.start_round(proposal);

        // Two rejections — cannot reach quorum
        shield.vote(proposal, agents[0], Vote::Reject);
        let r = shield.vote(proposal, agents[1], Vote::Reject);
        assert_eq!(r, ConsensusResult::Rejected);
    }
}
