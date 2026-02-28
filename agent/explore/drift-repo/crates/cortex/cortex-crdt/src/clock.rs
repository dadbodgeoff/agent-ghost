//! Vector clock for causal ordering.
//!
//! Each agent maintains its own logical clock entry. Used by delta sync
//! to ensure causal delivery and detect concurrent modifications.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::VectorClock;
//!
//! let mut a = VectorClock::new();
//! a.increment("agent-1");
//! a.increment("agent-1");
//!
//! let mut b = VectorClock::new();
//! b.increment("agent-2");
//!
//! assert!(a.concurrent_with(&b));
//!
//! a.merge(&b);
//! assert_eq!(a.get("agent-1"), 2);
//! assert_eq!(a.get("agent-2"), 1);
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A vector clock mapping agent IDs to logical timestamps.
///
/// Provides causal ordering primitives: happens-before, concurrent detection,
/// and dominance checks. Merge is component-wise max.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    /// Agent ID → logical clock value.
    clocks: HashMap<String, u64>,
}

impl VectorClock {
    /// Create an empty vector clock.
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    /// Increment the clock entry for the given agent by 1.
    pub fn increment(&mut self, agent_id: &str) {
        let entry = self.clocks.entry(agent_id.to_string()).or_insert(0);
        *entry += 1;
    }

    /// Get the current clock value for an agent (0 if absent).
    pub fn get(&self, agent_id: &str) -> u64 {
        self.clocks.get(agent_id).copied().unwrap_or(0)
    }

    /// Merge with another clock: component-wise max.
    ///
    /// After merge, each agent's entry is the maximum of the two clocks.
    /// This satisfies commutativity, associativity, and idempotency.
    pub fn merge(&mut self, other: &Self) {
        for (agent_id, &other_val) in &other.clocks {
            let entry = self.clocks.entry(agent_id.clone()).or_insert(0);
            *entry = (*entry).max(other_val);
        }
    }

    /// Returns true if `self` happens-before `other`.
    ///
    /// All entries in `self` are ≤ corresponding entries in `other`,
    /// and at least one entry is strictly less.
    pub fn happens_before(&self, other: &Self) -> bool {
        let mut at_least_one_less = false;

        // Check all entries in self are ≤ other
        for (agent_id, &self_val) in &self.clocks {
            let other_val = other.get(agent_id);
            if self_val > other_val {
                return false;
            }
            if self_val < other_val {
                at_least_one_less = true;
            }
        }

        // Check entries in other that are not in self (they are > 0 vs our implicit 0)
        for (agent_id, &other_val) in &other.clocks {
            if !self.clocks.contains_key(agent_id) && other_val > 0 {
                at_least_one_less = true;
            }
        }

        at_least_one_less
    }

    /// Returns true if neither clock happens-before the other.
    ///
    /// This indicates concurrent modifications by different agents.
    pub fn concurrent_with(&self, other: &Self) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }

    /// Returns true if `self` dominates `other`.
    ///
    /// All entries in `self` are ≥ corresponding entries in `other`,
    /// and at least one entry is strictly greater.
    pub fn dominates(&self, other: &Self) -> bool {
        other.happens_before(self)
    }

    /// Returns all agent IDs present in this clock.
    pub fn agents(&self) -> Vec<&str> {
        self.clocks.keys().map(|s| s.as_str()).collect()
    }

    /// Returns the number of agents tracked by this clock.
    pub fn len(&self) -> usize {
        self.clocks.len()
    }

    /// Returns true if the clock has no entries.
    pub fn is_empty(&self) -> bool {
        self.clocks.is_empty()
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}
