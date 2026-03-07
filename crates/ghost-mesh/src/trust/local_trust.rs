//! Local trust store: per-agent trust values derived from interaction history.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Outcome of an interaction between two agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InteractionOutcome {
    TaskCompleted,
    TaskFailed,
    PolicyViolation,
    SignatureFailure,
    Timeout,
}

impl InteractionOutcome {
    /// Trust delta for this outcome. Positive = trust increase, negative = decrease.
    fn trust_delta(&self) -> f64 {
        match self {
            InteractionOutcome::TaskCompleted => 0.1,
            InteractionOutcome::TaskFailed => -0.05,
            InteractionOutcome::PolicyViolation => -0.2,
            InteractionOutcome::SignatureFailure => -0.3,
            InteractionOutcome::Timeout => -0.02,
        }
    }
}

/// A recorded interaction between two agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractionRecord {
    pub from: Uuid,
    pub to: Uuid,
    pub outcome: InteractionOutcome,
    pub timestamp: DateTime<Utc>,
}

/// Per-agent local trust values for agents it has interacted with.
///
/// Trust is derived from task completion rate, policy compliance history,
/// convergence score stability, and message signing consistency.
pub struct LocalTrustStore {
    /// Interaction history: (from, to) → list of interactions.
    interactions: BTreeMap<(Uuid, Uuid), Vec<InteractionRecord>>,
    /// Cached local trust values: (from, to) → trust in [0.0, 1.0].
    cache: BTreeMap<(Uuid, Uuid), f64>,
    /// Dirty flag: set when interactions change, cleared after recompute.
    dirty: bool,
}

impl LocalTrustStore {
    pub fn new() -> Self {
        Self {
            interactions: BTreeMap::new(),
            cache: BTreeMap::new(),
            dirty: false,
        }
    }

    /// Record an interaction between two agents.
    pub fn record_interaction(&mut self, from: Uuid, to: Uuid, outcome: InteractionOutcome) {
        // Self-interactions are excluded (no self-trust inflation).
        if from == to {
            return;
        }
        let record = InteractionRecord {
            from,
            to,
            outcome,
            timestamp: Utc::now(),
        };
        self.interactions
            .entry((from, to))
            .or_default()
            .push(record);
        self.dirty = true;
        // Invalidate cache for this pair.
        self.cache.remove(&(from, to));
    }

    /// Get the local trust value from `from` to `to`. Returns 0.0 if no interactions.
    pub fn get_local_trust(&mut self, from: Uuid, to: Uuid) -> f64 {
        if from == to {
            return 0.0;
        }
        if let Some(&cached) = self.cache.get(&(from, to)) {
            return cached;
        }
        let trust = self.compute_local_trust(from, to);
        self.cache.insert((from, to), trust);
        trust
    }

    /// Compute local trust from interaction history.
    fn compute_local_trust(&self, from: Uuid, to: Uuid) -> f64 {
        let Some(records) = self.interactions.get(&(from, to)) else {
            return 0.0;
        };
        if records.is_empty() {
            return 0.0;
        }

        // Accumulate trust deltas, clamped to [0.0, 1.0].
        let mut trust = 0.0_f64;
        for record in records {
            trust += record.outcome.trust_delta();
        }
        trust.clamp(0.0, 1.0)
    }

    /// Get all unique agent IDs that have participated in interactions.
    pub fn all_agents(&self) -> Vec<Uuid> {
        let mut agents = std::collections::BTreeSet::new();
        for &(from, to) in self.interactions.keys() {
            agents.insert(from);
            agents.insert(to);
        }
        agents.into_iter().collect()
    }

    /// Get the normalized local trust matrix row for a given agent.
    /// Returns (target_agent_id, normalized_trust) pairs.
    pub fn normalized_row(&mut self, from: Uuid) -> BTreeMap<Uuid, f64> {
        let agents = self.all_agents();
        let mut row = BTreeMap::new();
        let mut total = 0.0_f64;

        for &to in &agents {
            if to == from {
                continue;
            }
            let trust = self.get_local_trust(from, to);
            if trust > 0.0 {
                row.insert(to, trust);
                total += trust;
            }
        }

        // Normalize so row sums to 1.0.
        if total > 0.0 {
            for val in row.values_mut() {
                *val /= total;
            }
        }
        row
    }

    /// Check if the store has been modified since last recompute.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Get the number of interactions recorded for a pair.
    pub fn interaction_count(&self, from: Uuid, to: Uuid) -> usize {
        self.interactions.get(&(from, to)).map_or(0, |v| v.len())
    }
}

impl Default for LocalTrustStore {
    fn default() -> Self {
        Self::new()
    }
}
