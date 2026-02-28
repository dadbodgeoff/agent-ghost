//! Multi-Value Register (MV-Register) CRDT.
//!
//! Preserves all concurrent values for manual resolution. When two agents
//! write concurrently (neither happens-before the other), both values are
//! kept. The `is_conflicted()` flag surfaces to the UI for manual resolution.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::{MVRegister, VectorClock};
//!
//! let mut reg = MVRegister::new();
//!
//! let mut clock_a = VectorClock::new();
//! clock_a.increment("agent-a");
//! reg.set("value-a".to_string(), &clock_a);
//!
//! let mut clock_b = VectorClock::new();
//! clock_b.increment("agent-b");
//!
//! let mut reg_b = MVRegister::new();
//! reg_b.set("value-b".to_string(), &clock_b);
//!
//! reg.merge(&reg_b);
//! assert!(reg.is_conflicted());
//! assert_eq!(reg.get().len(), 2);
//! ```

use crate::clock::VectorClock;
use serde::{Deserialize, Serialize};

/// A multi-value register that preserves all concurrent values.
///
/// Values are pruned when a new write dominates (happens-after) existing entries.
/// Concurrent writes are all preserved until explicit `resolve()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MVRegister<T> {
    /// Concurrent values with their causal context.
    values: Vec<(T, VectorClock)>,
}

impl<T: Clone + PartialEq> MVRegister<T> {
    /// Create a new empty MV-Register.
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Set a value with the given causal context.
    ///
    /// Prunes any existing entries whose clock is dominated by the new clock.
    /// The new value is added alongside any concurrent entries.
    pub fn set(&mut self, value: T, clock: &VectorClock) {
        // Remove entries dominated by the new clock
        self.values
            .retain(|(_, existing_clock)| !clock.dominates(existing_clock));
        self.values.push((value, clock.clone()));
    }

    /// Get all concurrent values.
    pub fn get(&self) -> Vec<&T> {
        self.values.iter().map(|(v, _)| v).collect()
    }

    /// Returns true if there are multiple concurrent values (conflict).
    pub fn is_conflicted(&self) -> bool {
        self.values.len() > 1
    }

    /// Returns true if the register has no values.
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Resolve the conflict by collapsing to a single value.
    ///
    /// This is an explicit user action — NOT "pick first". The caller
    /// provides the resolved value which replaces all concurrent entries.
    pub fn resolve(&mut self, value: T) {
        // Merge all existing clocks to create a clock that dominates everything
        let mut merged_clock = VectorClock::new();
        for (_, clock) in &self.values {
            merged_clock.merge(clock);
        }
        self.values.clear();
        self.values.push((value, merged_clock));
    }

    /// Merge with another MV-Register.
    ///
    /// Keep all non-dominated entries from both registers. An entry is
    /// dominated if any entry in the other register's clock dominates it.
    pub fn merge(&mut self, other: &Self) {
        let mut merged: Vec<(T, VectorClock)> = Vec::new();

        let all_entries: Vec<&(T, VectorClock)> =
            self.values.iter().chain(other.values.iter()).collect();

        for (i, (val, clock)) in all_entries.iter().enumerate() {
            let is_dominated = all_entries.iter().enumerate().any(|(j, (_, other_clock))| {
                i != j && other_clock.dominates(clock)
            });
            if !is_dominated {
                // Avoid duplicates: same value + same clock
                let already_present = merged
                    .iter()
                    .any(|(v, c)| v == val && c == clock);
                if !already_present {
                    merged.push(((*val).clone(), (*clock).clone()));
                }
            }
        }

        self.values = merged;
    }
}

impl<T: Clone + PartialEq> Default for MVRegister<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Clone + PartialEq> PartialEq for MVRegister<T> {
    fn eq(&self, other: &Self) -> bool {
        if self.values.len() != other.values.len() {
            return false;
        }
        // Order-independent comparison: every entry in self exists in other
        self.values.iter().all(|(v, c)| {
            other.values.iter().any(|(ov, oc)| v == ov && c == oc)
        }) && other.values.iter().all(|(v, c)| {
            self.values.iter().any(|(sv, sc)| v == sv && c == sc)
        })
    }
}

impl<T: Clone + PartialEq + Eq> Eq for MVRegister<T> {}
