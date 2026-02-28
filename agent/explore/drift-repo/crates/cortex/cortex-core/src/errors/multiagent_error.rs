//! Error types for multi-agent memory operations.

/// Errors specific to multi-agent memory operations.
#[derive(Debug, thiserror::Error)]
pub enum MultiAgentError {
    /// The specified agent was not found in the registry.
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    /// An agent with this ID is already registered.
    #[error("agent already registered: {0}")]
    AgentAlreadyRegistered(String),

    /// The specified namespace does not exist.
    #[error("namespace not found: {0}")]
    NamespaceNotFound(String),

    /// The agent does not have the required permission on the namespace.
    #[error("permission denied: agent {agent} lacks {permission:?} on namespace {namespace}")]
    PermissionDenied {
        /// The agent that was denied.
        agent: String,
        /// The namespace being accessed.
        namespace: String,
        /// The permission that was required.
        permission: String,
    },

    /// The specified projection was not found.
    #[error("projection not found: {0}")]
    ProjectionNotFound(String),

    /// The namespace URI is malformed.
    #[error("invalid namespace URI: {0}")]
    InvalidNamespaceUri(String),

    /// A causal ordering violation was detected during delta application.
    #[error("causal order violation: expected clock {expected}, found {found}")]
    CausalOrderViolation {
        /// Expected vector clock state.
        expected: String,
        /// Actual vector clock state found.
        found: String,
    },

    /// A cyclic dependency was detected in the causal graph.
    #[error("cyclic dependency detected: {0}")]
    CyclicDependency(String),

    /// Delta synchronization between agents failed.
    #[error("sync failed: {0}")]
    SyncFailed(String),

    /// Trust score computation failed.
    #[error("trust computation failed: {0}")]
    TrustComputationFailed(String),
}
