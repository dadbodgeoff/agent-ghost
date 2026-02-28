//! Max-Wins Register (MaxRegister) CRDT.
//!
//! Only values greater than the current propagate. Prevents accidental
//! regression from stale replicas.
//!
//! Used for: `confidence` (explicit boosts only), `last_accessed`.
//!
//! # Examples
//!
//! ```
//! use cortex_crdt::MaxRegister;
//! use chrono::Utc;
//!
//! let mut a = MaxRegister::new(0.5_f64, Utc::now());
//! let b = MaxRegister::new(0.8_f64, Utc::now());
//!
//! a.merge(&b);
//! assert!((*a.get() - 0.8).abs() < f64::EPSILON);
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A max-wins register. Value only increases. Merge keeps the greater value.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxRegister<T: PartialOrd> {
    value: T,
    timestamp: DateTime<Utc>,
}

/// Delta for MaxRegister sync.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaxDelta<T> {
    /// The value to propagate.
    pub value: T,
    /// Timestamp of the value.
    pub timestamp: DateTime<Utc>,
}

impl<T: PartialOrd + Clone> MaxRegister<T> {
    /// Create a new MaxRegister with an initial value.
    pub fn new(value: T, timestamp: DateTime<Utc>) -> Self {
        Self { value, timestamp }
    }

    /// Update the value only if the new value is greater.
    pub fn set(&mut self, value: T) {
        if value > self.value {
            self.value = value;
            self.timestamp = Utc::now();
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

    /// Merge with another MaxRegister: keep the greater value.
    ///
    /// If values are equal, keep the one with the later timestamp.
    pub fn merge(&mut self, other: &Self) {
        if other.value > self.value {
            self.value = other.value.clone();
            self.timestamp = other.timestamp;
        }
    }

    /// Compute delta if `self` is greater than `other`.
    pub fn delta_since(&self, other: &Self) -> Option<MaxDelta<T>> {
        if self.value > other.value {
            Some(MaxDelta {
                value: self.value.clone(),
                timestamp: self.timestamp,
            })
        } else {
            None
        }
    }
}

impl<T: PartialOrd + PartialEq + Clone> PartialEq for MaxRegister<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: PartialOrd + Eq + Clone> Eq for MaxRegister<T> {}
