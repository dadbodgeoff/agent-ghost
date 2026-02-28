//! Observed-Remove Set (OR-Set) CRDT with add-wins semantics.
//!
//! Concurrent add + remove of the same element → element is PRESENT.
//! Each add creates a unique tag. Remove only tombstones tags that existed
//! at the time of removal. A concurrent add creates a new tag that isn't
//! tombstoned.
//!
//! Used for: `tags`, `linked_patterns`, `linked_constraints`, `linked_files`,
//! `linked_functions`, `supersedes`.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::ORSet;
//!
//! let mut set = ORSet::new();
//! set.add("hello".to_string(), "agent-1", 1);
//! assert!(set.contains(&"hello".to_string()));
//!
//! set.remove(&"hello".to_string());
//! assert!(!set.contains(&"hello".to_string()));
//! ```

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

/// A unique tag identifying a specific add operation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UniqueTag {
    /// The agent that performed the add.
    pub agent_id: String,
    /// Monotonically increasing sequence number per agent.
    pub seq: u64,
}

/// Delta for OR-Set sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ORSetDelta<T: Eq + Hash> {
    /// New adds since the other's state.
    pub new_adds: HashMap<T, HashSet<UniqueTag>>,
    /// New tombstones since the other's state.
    pub new_tombstones: HashSet<UniqueTag>,
}

/// An observed-remove set with add-wins semantics.
///
/// Each element is associated with a set of unique tags (one per add operation).
/// Remove tombstones specific tags, not the element itself. A concurrent add
/// creates a new tag that survives the remove.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ORSet<T: Eq + Hash> {
    /// Element → set of unique tags from add operations.
    adds: HashMap<T, HashSet<UniqueTag>>,
    /// Tombstoned tags (from remove operations).
    tombstones: HashSet<UniqueTag>,
}

impl<T: Eq + Hash + Clone> ORSet<T> {
    /// Create a new empty OR-Set.
    pub fn new() -> Self {
        Self {
            adds: HashMap::new(),
            tombstones: HashSet::new(),
        }
    }

    /// Add an element with a unique tag.
    ///
    /// Returns the unique tag created for this add operation.
    pub fn add(&mut self, value: T, agent_id: &str, seq: u64) -> UniqueTag {
        let tag = UniqueTag {
            agent_id: agent_id.to_string(),
            seq,
        };
        self.adds
            .entry(value)
            .or_default()
            .insert(tag.clone());
        tag
    }

    /// Remove an element by tombstoning all its current tags.
    ///
    /// Only tombstones tags that exist at the time of removal. Concurrent
    /// adds create new tags that won't be tombstoned (add-wins semantics).
    pub fn remove(&mut self, value: &T) {
        if let Some(tags) = self.adds.get(value) {
            for tag in tags.iter() {
                self.tombstones.insert(tag.clone());
            }
        }
    }

    /// Check if an element is present (has at least one non-tombstoned tag).
    pub fn contains(&self, value: &T) -> bool {
        if let Some(tags) = self.adds.get(value) {
            tags.iter().any(|tag| !self.tombstones.contains(tag))
        } else {
            false
        }
    }

    /// Get all present elements (those with at least one non-tombstoned tag).
    pub fn elements(&self) -> Vec<&T> {
        self.adds
            .iter()
            .filter(|(_, tags)| tags.iter().any(|tag| !self.tombstones.contains(tag)))
            .map(|(value, _)| value)
            .collect()
    }

    /// Number of present elements.
    pub fn len(&self) -> usize {
        self.elements().len()
    }

    /// Returns true if no elements are present.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Merge with another OR-Set: union of add-sets, union of tombstone-sets.
    ///
    /// Add-wins semantics: if element has a tag in adds that is NOT in
    /// tombstones, the element is present after merge.
    pub fn merge(&mut self, other: &Self) {
        // Union of adds
        for (value, other_tags) in &other.adds {
            let entry = self.adds.entry(value.clone()).or_default();
            for tag in other_tags {
                entry.insert(tag.clone());
            }
        }
        // Union of tombstones
        for tag in &other.tombstones {
            self.tombstones.insert(tag.clone());
        }
    }

    /// Compute delta since another OR-Set's state.
    ///
    /// Returns new adds and new tombstones that the other doesn't have.
    pub fn delta_since(&self, other: &Self) -> ORSetDelta<T> {
        let mut new_adds: HashMap<T, HashSet<UniqueTag>> = HashMap::new();
        for (value, tags) in &self.adds {
            let other_tags = other.adds.get(value);
            for tag in tags {
                let is_new = match other_tags {
                    Some(ot) => !ot.contains(tag),
                    None => true,
                };
                if is_new {
                    new_adds
                        .entry(value.clone())
                        .or_default()
                        .insert(tag.clone());
                }
            }
        }

        let new_tombstones: HashSet<UniqueTag> = self
            .tombstones
            .difference(&other.tombstones)
            .cloned()
            .collect();

        ORSetDelta {
            new_adds,
            new_tombstones,
        }
    }
}

impl<T: Eq + Hash + Clone> Default for ORSet<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Eq + Hash + Clone> PartialEq for ORSet<T> {
    fn eq(&self, other: &Self) -> bool {
        self.adds == other.adds && self.tombstones == other.tombstones
    }
}

impl<T: Eq + Hash + Clone> Eq for ORSet<T> {}
