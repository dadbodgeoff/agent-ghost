//! Mesh error types.

use thiserror::Error;
use uuid::Uuid;

/// Unified error type for all mesh operations.
#[derive(Debug, Error)]
pub enum MeshError {
    #[error("agent not found: {agent_id}")]
    AgentNotFound { agent_id: Uuid },

    #[error("task failed: {reason}")]
    TaskFailed { task_id: Uuid, reason: String },

    #[error("authentication failed: {reason}")]
    AuthenticationFailed { reason: String },

    #[error("trust insufficient: agent {agent_id} has trust {trust:.3}, required {required:.3}")]
    TrustInsufficient {
        agent_id: Uuid,
        trust: f64,
        required: f64,
    },

    #[error("rate limited: {reason}")]
    RateLimited { reason: String },

    #[error("protocol error: {0}")]
    ProtocolError(String),

    #[error("timeout after {duration_secs}s")]
    Timeout { duration_secs: u64 },

    #[error("invalid state transition: {from} → {to}")]
    InvalidTransition { from: String, to: String },

    #[error("delegation depth exceeded: {depth} > {max}")]
    DelegationDepthExceeded { depth: u32, max: u32 },

    #[error("circuit breaker open for {from} → {to}")]
    CircuitBreakerOpen { from: Uuid, to: Uuid },

    #[error("memory poisoning detected: {reason}")]
    MemoryPoisoningDetected { reason: String },
}
