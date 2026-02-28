//! Core `EgressPolicy` trait — the abstraction all egress backends implement.

use uuid::Uuid;

use crate::config::AgentEgressConfig;
use crate::error::EgressError;

/// Unified interface for network egress control backends.
///
/// Implementations:
/// - [`ProxyEgressPolicy`](crate::proxy_provider::ProxyEgressPolicy) — cross-platform localhost proxy fallback
/// - [`EbpfEgressPolicy`](crate::ebpf_provider::EbpfEgressPolicy) — Linux eBPF cgroup filter (feature `ebpf`)
/// - [`PfEgressPolicy`](crate::pf_provider::PfEgressPolicy) — macOS packet filter (feature `pf`)
pub trait EgressPolicy: Send + Sync {
    /// Apply an egress policy for the given agent.
    ///
    /// For proxy: starts a localhost proxy bound to a dynamic port.
    /// For eBPF: loads and attaches the cgroup filter program.
    /// For pf: creates a pf anchor with IP-based rules.
    fn apply(&self, agent_id: &Uuid, config: &AgentEgressConfig) -> Result<(), EgressError>;

    /// Check whether a domain is allowed for the given agent.
    ///
    /// Returns `Ok(true)` if allowed, `Ok(false)` if blocked.
    fn check_domain(&self, agent_id: &Uuid, domain: &str) -> Result<bool, EgressError>;

    /// Remove the egress policy for the given agent, releasing all resources.
    fn remove(&self, agent_id: &Uuid) -> Result<(), EgressError>;

    /// Log a policy violation for audit trail.
    fn log_violation(&self, agent_id: &Uuid, domain: &str, action: &str);
}
