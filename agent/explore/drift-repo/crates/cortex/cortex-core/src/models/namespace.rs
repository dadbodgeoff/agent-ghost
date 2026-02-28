//! Namespace types for multi-agent memory isolation and sharing.
//!
//! # Examples
//!
//! ```
//! use cortex_core::models::namespace::{NamespaceId, NamespaceScope};
//! use cortex_core::models::agent::AgentId;
//!
//! let ns = NamespaceId::default_namespace();
//! assert_eq!(ns.to_uri(), "agent://default/");
//!
//! let team_ns = NamespaceId {
//!     scope: NamespaceScope::Team("backend".to_string()),
//!     name: "shared".to_string(),
//! };
//! assert_eq!(team_ns.to_uri(), "team://shared/");
//! ```

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::agent::AgentId;
use crate::memory::importance::Importance;
use crate::memory::types::MemoryType;

/// A namespace identifier composed of a scope and a name.
///
/// URI format: `{scope}://{name}/`
/// - `agent://default/` — the default single-agent namespace
/// - `team://backend/` — a team-scoped namespace
/// - `project://cortex/` — a project-scoped namespace
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NamespaceId {
    /// The scope determines visibility and default permissions.
    pub scope: NamespaceScope,
    /// Human-readable namespace name.
    pub name: String,
}

impl NamespaceId {
    /// The default namespace for backward compatibility with single-agent mode.
    pub fn default_namespace() -> Self {
        Self {
            scope: NamespaceScope::Agent(AgentId::default_agent()),
            name: "default".to_string(),
        }
    }

    /// Format as a URI string: `{scope}://{name}/`.
    pub fn to_uri(&self) -> String {
        let scope_str = match &self.scope {
            NamespaceScope::Agent(_) => "agent",
            NamespaceScope::Team(_) => "team",
            NamespaceScope::Project(_) => "project",
        };
        format!("{scope_str}://{}/", self.name)
    }

    /// Parse a namespace URI string into a `NamespaceId`.
    ///
    /// Accepted formats: `agent://name/`, `team://name/`, `project://name/`.
    pub fn parse(uri: &str) -> Result<Self, String> {
        let uri = uri.trim_end_matches('/');
        let parts: Vec<&str> = uri.splitn(2, "://").collect();
        if parts.len() != 2 {
            return Err(format!("invalid namespace URI: {uri}"));
        }
        let (scope_str, name) = (parts[0], parts[1]);
        let name = name.to_string();
        if name.is_empty() {
            return Err(format!("namespace name cannot be empty: {uri}"));
        }
        let scope = match scope_str {
            "agent" => NamespaceScope::Agent(AgentId::from(name.as_str())),
            "team" => NamespaceScope::Team(name.clone()),
            "project" => NamespaceScope::Project(name.clone()),
            other => return Err(format!("unknown namespace scope: {other}")),
        };
        Ok(Self { scope, name })
    }

    /// Whether this is an agent-scoped namespace.
    pub fn is_agent(&self) -> bool {
        matches!(self.scope, NamespaceScope::Agent(_))
    }

    /// Whether this is a team-scoped namespace.
    pub fn is_team(&self) -> bool {
        matches!(self.scope, NamespaceScope::Team(_))
    }

    /// Whether this is a project-scoped namespace.
    pub fn is_project(&self) -> bool {
        matches!(self.scope, NamespaceScope::Project(_))
    }

    /// Whether this is a shared namespace (team or project).
    pub fn is_shared(&self) -> bool {
        self.is_team() || self.is_project()
    }
}

impl Default for NamespaceId {
    fn default() -> Self {
        Self::default_namespace()
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_uri())
    }
}

/// The scope of a namespace, determining visibility and default permissions.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum NamespaceScope {
    /// Private to a single agent.
    Agent(AgentId),
    /// Shared among a team.
    Team(String),
    /// Shared across an entire project.
    Project(String),
}

/// Permission levels for namespace access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum NamespacePermission {
    /// Can read memories in this namespace.
    Read,
    /// Can write (create/update) memories in this namespace.
    Write,
    /// Can share memories from this namespace to others.
    Share,
    /// Full administrative access including permission management.
    Admin,
}

/// Access control list for a namespace.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct NamespaceACL {
    /// The namespace this ACL applies to.
    pub namespace: NamespaceId,
    /// Permission grants: (agent_id, permissions).
    pub grants: Vec<(AgentId, Vec<NamespacePermission>)>,
}

/// A projection from one namespace to another with optional filtering.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct MemoryProjection {
    /// Unique projection identifier.
    pub id: String,
    /// Source namespace to project from.
    pub source: NamespaceId,
    /// Target namespace to project into.
    pub target: NamespaceId,
    /// Filter criteria for which memories to include.
    pub filter: ProjectionFilter,
    /// Compression level for projected memories (0–3).
    pub compression_level: u8,
    /// Whether this projection is live (auto-syncs on changes).
    pub live: bool,
    /// When this projection was created.
    pub created_at: DateTime<Utc>,
    /// Who created this projection.
    pub created_by: AgentId,
}

/// Filter criteria for memory projections.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct ProjectionFilter {
    /// Only include these memory types (empty = all).
    #[serde(default)]
    pub memory_types: Vec<MemoryType>,
    /// Minimum confidence threshold.
    #[serde(default)]
    pub min_confidence: Option<f64>,
    /// Minimum importance level.
    #[serde(default)]
    pub min_importance: Option<Importance>,
    /// Only include memories linked to these files.
    #[serde(default)]
    pub linked_files: Vec<String>,
    /// Only include memories with these tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Maximum age in days.
    #[serde(default)]
    pub max_age_days: Option<u64>,
    /// Custom predicate expression (future use).
    #[serde(default)]
    pub predicate: Option<String>,
}
