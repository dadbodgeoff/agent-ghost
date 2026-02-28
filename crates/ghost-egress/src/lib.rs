//! # ghost-egress
//!
//! Per-agent network egress allowlisting with three backends:
//! - **ProxyEgressPolicy** — cross-platform localhost proxy fallback (always available)
//! - **EbpfEgressPolicy** — Linux eBPF cgroup filter (feature `ebpf`, requires `CAP_BPF`)
//! - **PfEgressPolicy** — macOS packet filter (feature `pf`, requires root)
//!
//! Violation events feed into the `AutoTriggerEvaluator` via `TriggerEvent::NetworkEgressViolation`.
//! Domain matching is case-insensitive and supports wildcard patterns (`*.slack.com`).

pub mod config;
pub mod domain_matcher;
pub mod error;
pub mod policy;
pub mod proxy_provider;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub mod ebpf_provider;

#[cfg(all(target_os = "macos", feature = "pf"))]
pub mod pf_provider;

// Re-exports for convenience.
pub use config::{AgentEgressConfig, EgressPolicyMode};
pub use domain_matcher::DomainMatcher;
pub use error::EgressError;
pub use policy::EgressPolicy;
pub use proxy_provider::ProxyEgressPolicy;

#[cfg(all(target_os = "linux", feature = "ebpf"))]
pub use ebpf_provider::EbpfEgressPolicy;

#[cfg(all(target_os = "macos", feature = "pf"))]
pub use pf_provider::PfEgressPolicy;
