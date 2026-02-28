use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Add, Mul, Sub};
use ts_rs::TS;

/// Confidence score clamped to [0.0, 1.0].
/// Represents how confident the system is in a memory's accuracy.
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct Confidence(f64);

impl Confidence {
    /// High confidence threshold — memories above this are considered reliable.
    pub const HIGH: f64 = 0.8;
    /// Medium confidence threshold.
    pub const MEDIUM: f64 = 0.5;
    /// Low confidence threshold — memories below this may need validation.
    pub const LOW: f64 = 0.3;
    /// Archival threshold — memories below this are candidates for archival.
    pub const ARCHIVAL: f64 = 0.15;

    /// Create a new Confidence, clamping to [0.0, 1.0].
    pub fn new(value: f64) -> Self {
        Self(value.clamp(0.0, 1.0))
    }

    /// Get the raw f64 value.
    pub fn value(self) -> f64 {
        self.0
    }

    /// Check if confidence is above the high threshold.
    pub fn is_high(self) -> bool {
        self.0 >= Self::HIGH
    }

    /// Check if confidence is below the archival threshold.
    pub fn is_archival(self) -> bool {
        self.0 < Self::ARCHIVAL
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self(1.0)
    }
}

impl fmt::Display for Confidence {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.3}", self.0)
    }
}

impl From<f64> for Confidence {
    fn from(value: f64) -> Self {
        Self::new(value)
    }
}

impl From<Confidence> for f64 {
    fn from(c: Confidence) -> Self {
        c.0
    }
}

impl Add for Confidence {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.0 + rhs.0)
    }
}

impl Sub for Confidence {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.0 - rhs.0)
    }
}

impl Mul<f64> for Confidence {
    type Output = Self;
    fn mul(self, rhs: f64) -> Self {
        Self::new(self.0 * rhs)
    }
}
