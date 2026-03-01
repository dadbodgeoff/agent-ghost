//! Agent templates loaded from YAML (personal, developer, researcher).

use serde::{Deserialize, Serialize};

/// Predefined agent template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    pub name: String,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default = "default_spending_cap")]
    pub spending_cap: f64,
    #[serde(default)]
    pub heartbeat_interval_minutes: u32,
    #[serde(default)]
    pub convergence_profile: String,
}

fn default_spending_cap() -> f64 { 5.0 }

impl AgentTemplate {
    /// Load a template from YAML string.
    pub fn from_yaml(yaml: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml).map_err(|e| e.to_string())
    }

    /// Built-in personal template.
    pub fn personal() -> Self {
        Self {
            name: "personal".into(),
            capabilities: vec![
                "memory_read".into(),
                "memory_write".into(),
                "web_search".into(),
                "web_fetch".into(),
            ],
            spending_cap: 5.0,
            heartbeat_interval_minutes: 30,
            convergence_profile: "companion".into(),
        }
    }

    /// Built-in developer template.
    pub fn developer() -> Self {
        Self {
            name: "developer".into(),
            capabilities: vec![
                "memory_read".into(),
                "memory_write".into(),
                "shell_execute".into(),
                "filesystem_read".into(),
                "filesystem_write".into(),
                "web_search".into(),
                "web_fetch".into(),
                "http_request".into(),
            ],
            spending_cap: 10.0,
            heartbeat_interval_minutes: 60,
            convergence_profile: "productivity".into(),
        }
    }

    /// Built-in researcher template.
    pub fn researcher() -> Self {
        Self {
            name: "researcher".into(),
            capabilities: vec![
                "memory_read".into(),
                "memory_write".into(),
                "web_search".into(),
                "web_fetch".into(),
                "http_request".into(),
            ],
            spending_cap: 20.0,
            heartbeat_interval_minutes: 120,
            convergence_profile: "research".into(),
        }
    }
}
