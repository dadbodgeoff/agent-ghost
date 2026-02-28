use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::defaults;

/// Privacy subsystem configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacyConfig {
    /// Per-pattern confidence overrides.
    pub pattern_overrides: HashMap<String, f64>,
    /// Enable NER-based detection (requires model).
    pub ner_enabled: bool,
    /// Enable context-aware scoring.
    pub context_scoring: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            pattern_overrides: HashMap::new(),
            ner_enabled: defaults::DEFAULT_NER_ENABLED,
            context_scoring: defaults::DEFAULT_CONTEXT_SCORING,
        }
    }
}
