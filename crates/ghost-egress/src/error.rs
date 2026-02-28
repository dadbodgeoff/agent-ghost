//! Error types for the network egress control infrastructure.

use thiserror::Error;

/// Errors from egress policy operations.
#[derive(Debug, Error)]
pub enum EgressError {
    /// A network request violated the configured egress policy.
    #[error("policy violation: agent {agent_id} attempted to reach blocked domain '{domain}'")]
    PolicyViolation {
        agent_id: uuid::Uuid,
        domain: String,
    },

    /// Configuration error in egress policy setup.
    #[error("config error: {0}")]
    ConfigError(String),

    /// The egress policy provider is unavailable (e.g. eBPF not supported).
    #[error("provider unavailable: {0}")]
    ProviderUnavailable(String),

    /// DNS resolution failed for an allowed domain.
    #[error("domain resolution failed: {0}")]
    DomainResolutionFailed(String),
}
