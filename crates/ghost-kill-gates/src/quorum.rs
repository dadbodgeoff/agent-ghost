//! Quorum logic for distributed gate resume.
//!
//! Resume from a distributed kill requires ceil(n/2) + 1 votes.
//! Single-node resume is impossible (INV-KG-03).

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A resume vote from a single node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeVote {
    pub node_id: Uuid,
    pub reason: String,
    pub initiated_by: String,
    pub voted_at: DateTime<Utc>,
}

/// Tracks resume votes and determines quorum.
pub struct QuorumTracker {
    required: usize,
    votes: BTreeSet<Uuid>,
    vote_log: Vec<ResumeVote>,
}

impl QuorumTracker {
    pub fn new(required_votes: usize) -> Self {
        Self {
            required: required_votes.max(1),
            votes: BTreeSet::new(),
            vote_log: Vec::new(),
        }
    }

    /// Cast a resume vote. Returns true if quorum is now reached.
    pub fn cast_vote(&mut self, vote: ResumeVote) -> bool {
        self.votes.insert(vote.node_id);
        self.vote_log.push(vote);
        self.has_quorum()
    }

    /// Check if quorum has been reached.
    pub fn has_quorum(&self) -> bool {
        self.votes.len() >= self.required
    }

    /// Number of votes received so far.
    pub fn vote_count(&self) -> usize {
        self.votes.len()
    }

    /// Required votes for quorum.
    pub fn required(&self) -> usize {
        self.required
    }

    /// Reset votes (after quorum reached and gate reopened).
    pub fn reset(&mut self) {
        self.votes.clear();
        self.vote_log.clear();
    }

    /// Get all vote records.
    pub fn vote_log(&self) -> &[ResumeVote] {
        &self.vote_log
    }
}
