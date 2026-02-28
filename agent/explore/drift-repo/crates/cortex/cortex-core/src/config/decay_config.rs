use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::defaults;

/// Decay subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DecayConfig {
    /// Per-type half-life overrides (days). Key is memory type name.
    pub half_life_overrides: HashMap<String, u64>,
    /// Confidence threshold below which memories are archived.
    pub archival_threshold: f64,
    /// Interval between decay processing runs (seconds).
    pub processing_interval_secs: u64,
}

impl Default for DecayConfig {
    fn default() -> Self {
        Self {
            half_life_overrides: HashMap::new(),
            archival_threshold: defaults::DEFAULT_ARCHIVAL_THRESHOLD,
            processing_interval_secs: defaults::DEFAULT_DECAY_PROCESSING_INTERVAL_SECS,
        }
    }
}
