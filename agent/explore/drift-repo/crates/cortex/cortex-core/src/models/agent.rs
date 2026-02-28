//! Agent identity and registration types for multi-agent memory.
//!
//! # Examples
//!
//! ```
//! use cortex_core::models::agent::{AgentId, AgentStatus};
//!
//! let agent = AgentId::new();
//! assert!(!agent.0.is_empty());
//!
//! let default = AgentId::default_agent();
//! assert_eq!(default.0, "default");
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// UUID-based agent identifier.
///
/// Wraps a `String` for type safety. Use [`AgentId::new()`] for a fresh UUID
/// or [`AgentId::default_agent()`] for the backward-compatible sentinel value.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentId(pub String);

impl AgentId {
    /// Create a new agent ID with a random UUID v4.
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// The default agent sentinel — used for single-agent backward compatibility.
    ///
    /// All existing memories created before multi-agent support are attributed
    /// to this agent. When `MultiAgentConfig.enabled` is `false`, this is the
    /// only agent in the system.
    pub fn default_agent() -> Self {
        Self("default".to_string())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::default_agent()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for AgentId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for AgentId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Full agent metadata stored in the registry.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AgentRegistration {
    /// Unique agent identifier.
    pub agent_id: AgentId,
    /// Human-readable agent name (must be non-empty).
    pub name: String,
    /// The agent's home namespace URI.
    pub namespace: String,
    /// Capabilities this agent advertises (e.g., "code_review", "testing").
    pub capabilities: Vec<String>,
    /// Parent agent if this was spawned.
    pub parent_agent: Option<AgentId>,
    /// When this agent was registered.
    pub registered_at: DateTime<Utc>,
    /// Last heartbeat timestamp.
    pub last_active: DateTime<Utc>,
    /// Current lifecycle status.
    pub status: AgentStatus,
}

/// Agent lifecycle status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
#[ts(export)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is actively processing.
    Active,
    /// Agent has been idle since the given timestamp.
    Idle { since: DateTime<Utc> },
    /// Agent has been deregistered at the given timestamp.
    Deregistered { at: DateTime<Utc> },
}

/// Configuration for spawning a sub-agent.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SpawnConfig {
    /// The parent agent spawning this sub-agent.
    pub parent_agent: AgentId,
    /// Optional projection from parent namespace to give the sub-agent context.
    pub projection: Option<String>,
    /// Trust discount factor applied to inherited trust scores (default: 0.8).
    pub trust_discount: f64,
    /// Whether to auto-promote sub-agent memories to parent namespace on deregister.
    pub auto_promote_on_deregister: bool,
    /// Optional time-to-live in seconds.
    pub ttl: Option<u64>,
}

impl Default for SpawnConfig {
    fn default() -> Self {
        Self {
            parent_agent: AgentId::default_agent(),
            projection: None,
            trust_discount: 0.8,
            auto_promote_on_deregister: false,
            ttl: None,
        }
    }
}
