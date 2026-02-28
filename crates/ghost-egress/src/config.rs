//! Configuration types for per-agent network egress control.

use serde::{Deserialize, Serialize};

/// Default allowed domains — LLM provider APIs that agents need to reach.
pub const DEFAULT_ALLOWED_DOMAINS: &[&str] = &[
    "api.anthropic.com",
    "api.openai.com",
    "generativelanguage.googleapis.com",
    "api.mistral.ai",
    "api.groq.com",
];

/// Per-agent egress configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEgressConfig {
    /// Policy mode: allowlist, blocklist, or unrestricted.
    #[serde(default)]
    pub policy: EgressPolicyMode,

    /// Domains allowed when policy is `Allowlist`. Supports wildcards: `*.slack.com`.
    #[serde(default = "default_allowed_domains")]
    pub allowed_domains: Vec<String>,

    /// Domains blocked when policy is `Blocklist`.
    #[serde(default)]
    pub blocked_domains: Vec<String>,

    /// Whether to log violation events.
    #[serde(default = "default_true")]
    pub log_violations: bool,

    /// Whether to emit a TriggerEvent on violation.
    #[serde(default)]
    pub alert_on_violation: bool,

    /// Number of violations in `violation_window_minutes` before QUARANTINE.
    #[serde(default = "default_violation_threshold")]
    pub violation_threshold: u32,

    /// Time window (minutes) for violation counting.
    #[serde(default = "default_violation_window")]
    pub violation_window_minutes: u32,
}

impl Default for AgentEgressConfig {
    fn default() -> Self {
        Self {
            policy: EgressPolicyMode::default(),
            allowed_domains: default_allowed_domains(),
            blocked_domains: Vec::new(),
            log_violations: true,
            alert_on_violation: false,
            violation_threshold: default_violation_threshold(),
            violation_window_minutes: default_violation_window(),
        }
    }
}

/// Egress policy mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum EgressPolicyMode {
    /// Only explicitly allowed domains are reachable.
    Allowlist,
    /// All domains reachable except explicitly blocked ones.
    Blocklist,
    /// No restrictions (backward compatible default).
    #[default]
    Unrestricted,
}

fn default_allowed_domains() -> Vec<String> {
    DEFAULT_ALLOWED_DOMAINS.iter().map(|s| s.to_string()).collect()
}

fn default_true() -> bool {
    true
}

fn default_violation_threshold() -> u32 {
    5
}

fn default_violation_window() -> u32 {
    10
}
