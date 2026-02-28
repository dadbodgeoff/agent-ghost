//! Grow-only counter (G-Counter) CRDT.
//!
//! Each agent maintains its own counter. The total value is the sum of all
//! agent counters. Merge takes the per-agent maximum.
//!
//! Used for: `access_count`, `retrieval_count` — fields that only ever increase.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::GCounter;
//!
//! let mut a = GCounter::new();
//! a.increment("agent-1");
//! a.increment("agent-1");
//!
//! let mut b = GCounter::new();
//! b.increment("agent-2");
//!
//! a.merge(&b);
//! assert_eq!(a.value(), 3); // 2 + 1
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A grow-only counter where each agent maintains its own monotonically
/// increasing count. Merge = per-agent max. Value = sum of all counts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GCounter {
    /// Agent ID → agent's counter value.
    counts: HashMap<String, u64>,
}

/// Delta for G-Counter sync: entries where the sender is ahead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GCounterDelta {
    /// Agent ID → counter value (only entries where sender > receiver).
    pub counts: HashMap<String, u64>,
}

impl GCounter {
    /// Create a new empty G-Counter.
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    /// Increment the counter for the given agent by 1.
    pub fn increment(&mut self, agent_id: &str) {
        let entry = self.counts.entry(agent_id.to_string()).or_insert(0);
        *entry += 1;
    }

    /// Get the total value (sum of all agent counters).
    pub fn value(&self) -> u64 {
        self.counts.values().sum()
    }

    /// Get the counter value for a specific agent.
    pub fn agent_value(&self, agent_id: &str) -> u64 {
        self.counts.get(agent_id).copied().unwrap_or(0)
    }

    /// Merge with another G-Counter: per-agent max.
    ///
    /// Convergence guarantee: monotonically increasing. No lost increments.
    /// `merge(A, B).value() >= max(A.value(), B.value())`.
    pub fn merge(&mut self, other: &Self) {
        for (agent_id, &other_val) in &other.counts {
            let entry = self.counts.entry(agent_id.clone()).or_insert(0);
            *entry = (*entry).max(other_val);
        }
    }

    /// Compute the delta since another G-Counter's state.
    ///
    /// Returns entries where `self` is ahead of `other` (for delta sync).
    pub fn delta_since(&self, other: &Self) -> GCounterDelta {
        let mut counts = HashMap::new();
        for (agent_id, &self_val) in &self.counts {
            let other_val = other.agent_value(agent_id);
            if self_val > other_val {
                counts.insert(agent_id.clone(), self_val);
            }
        }
        GCounterDelta { counts }
    }
}

impl Default for GCounter {
    fn default() -> Self {
        Self::new()
    }
}
