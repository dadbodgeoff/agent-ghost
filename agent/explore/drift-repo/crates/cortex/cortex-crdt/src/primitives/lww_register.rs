//! Last-Writer-Wins Register (LWW-Register) CRDT.
//!
//! Each update carries a timestamp and agent_id. Merge keeps the value with
//! the highest timestamp. Tie-break: lexicographically greater agent_id wins.
//!
//! Used for: `content`, `summary`, `memory_type`, `importance`, `archived`,
//! `superseded_by`, `valid_time`, `valid_until`, `namespace`.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::LWWRegister;
//! use chrono::Utc;
//!
//! let mut a = LWWRegister::new("hello".to_string(), Utc::now(), "agent-a".to_string());
//! let b = LWWRegister::new("world".to_string(), Utc::now(), "agent-b".to_string());
//!
//! a.merge(&b);
//! // The value with the higher (timestamp, agent_id) pair wins.
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A last-writer-wins register. The value with the highest `(timestamp, agent_id)`
/// pair wins on merge. Deterministic tie-breaking via lexicographic agent_id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LWWRegister<T> {
    value: T,
    timestamp: DateTime<Utc>,
    agent_id: String,
}

/// Delta for LWW-Register sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LWWDelta<T> {
    /// The value to propagate.
    pub value: T,
    /// Timestamp of the value.
    pub timestamp: DateTime<Utc>,
    /// Agent that wrote the value.
    pub agent_id: String,
}

impl<T: Clone> LWWRegister<T> {
    /// Create a new LWW-Register with an initial value.
    pub fn new(value: T, timestamp: DateTime<Utc>, agent_id: String) -> Self {
        Self {
            value,
            timestamp,
            agent_id,
        }
    }

    /// Update the value only if `(timestamp, agent_id)` is greater than current.
    pub fn set(&mut self, value: T, timestamp: DateTime<Utc>, agent_id: String) {
        if timestamp > self.timestamp
            || (timestamp == self.timestamp && agent_id > self.agent_id)
        {
            self.value = value;
            self.timestamp = timestamp;
            self.agent_id = agent_id;
        }
    }

    /// Get a reference to the current value.
    pub fn get(&self) -> &T {
        &self.value
    }

    /// Get the timestamp of the current value.
    pub fn timestamp(&self) -> DateTime<Utc> {
        self.timestamp
    }

    /// Get the agent_id of the current value.
    pub fn agent_id(&self) -> &str {
        &self.agent_id
    }

    /// Merge with another LWW-Register: keep the higher `(timestamp, agent_id)` pair.
    ///
    /// Tie-breaking rule: when timestamps are equal, the lexicographically
    /// greater agent_id wins. This ensures deterministic convergence even
    /// with synchronized clocks.
    pub fn merge(&mut self, other: &Self) {
        if other.timestamp > self.timestamp
            || (other.timestamp == self.timestamp && other.agent_id > self.agent_id)
        {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
            self.agent_id = other.agent_id.clone();
        }
    }

    /// Compute delta if `self` is newer than `other`.
    pub fn delta_since(&self, other: &Self) -> Option<LWWDelta<T>> {
        if self.timestamp > other.timestamp
            || (self.timestamp == other.timestamp && self.agent_id > other.agent_id)
        {
            Some(LWWDelta {
                value: self.value.clone(),
                timestamp: self.timestamp,
                agent_id: self.agent_id.clone(),
            })
        } else {
            None
        }
    }
}

impl<T: Clone + PartialEq> PartialEq for LWWRegister<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
            && self.timestamp == other.timestamp
            && self.agent_id == other.agent_id
    }
}

impl<T: Clone + Eq> Eq for LWWRegister<T> {}
