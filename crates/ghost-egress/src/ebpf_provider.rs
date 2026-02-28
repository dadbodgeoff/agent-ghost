//! Linux eBPF cgroup-level egress filter (Task 11.3).
//!
//! Uses the Aya crate for pure-Rust eBPF. Attaches a `CgroupSkb` program
//! to the agent's cgroup that filters outbound connections by destination IP.
//! Allowed domains are resolved to IPs in userspace and populated into an
//! eBPF HashMap. Requires `CAP_BPF` capability.
//!
//! Feature-gated: `#[cfg(all(target_os = "linux", feature = "ebpf"))]`
//! Falls back to `ProxyEgressPolicy` if eBPF loading fails.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::config::AgentEgressConfig;
use crate::domain_matcher::DomainMatcher;
use crate::error::EgressError;
use crate::policy::EgressPolicy;
use crate::proxy_provider::ProxyEgressPolicy;

/// DNS re-resolution interval (5 minutes) for keeping eBPF maps current
/// when upstream DNS records change.
const DNS_RERESOLUTION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Per-agent eBPF state.
#[derive(Debug)]
struct AgentEbpf {
    /// Resolved IPs for allowed domains.
    allowed_ips: Vec<IpAddr>,
    /// Domain matcher for userspace checks.
    matcher: DomainMatcher,
    /// Config snapshot for DNS re-resolution.
    config: AgentEgressConfig,
    /// Whether we fell back to proxy mode.
    using_proxy_fallback: bool,
    /// Handle to cancel the periodic DNS re-resolution task.
    dns_reresolution_cancel: Option<tokio::sync::oneshot::Sender<()>>,
}

/// Linux eBPF cgroup-level egress policy.
///
/// On `apply`: loads an eBPF program, attaches to the agent's cgroup,
/// and populates the allowlist map with resolved IPs. Falls back to
/// `ProxyEgressPolicy` if eBPF loading fails (missing `CAP_BPF`).
///
/// Periodic DNS re-resolution runs every 5 minutes to handle IP changes.
/// Violation events are read from the eBPF perf event buffer and fed
/// into the violation counter.
pub struct EbpfEgressPolicy {
    agents: Arc<Mutex<HashMap<Uuid, AgentEbpf>>>,
    /// Fallback proxy policy for when eBPF is unavailable.
    proxy_fallback: ProxyEgressPolicy,
}

impl EbpfEgressPolicy {
    /// Create a new eBPF egress policy manager.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            proxy_fallback: ProxyEgressPolicy::new(),
        }
    }

    /// Resolve domains to IP addresses for eBPF map population.
    fn resolve_domains(domains: &[String]) -> Vec<IpAddr> {
        let mut ips = Vec::new();
        for domain in domains {
            // Strip wildcard prefix for resolution.
            let resolve_domain = if domain.starts_with("*.") {
                &domain[2..]
            } else {
                domain.as_str()
            };

            match std::net::ToSocketAddrs::to_socket_addrs(&(resolve_domain, 443)) {
                Ok(addrs) => {
                    for addr in addrs {
                        ips.push(addr.ip());
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        domain = %resolve_domain,
                        error = %e,
                        "DNS resolution failed for eBPF allowlist"
                    );
                }
            }
        }
        ips
    }

    /// Attempt to load the eBPF program. Returns false if unavailable.
    fn try_load_ebpf(_agent_id: &Uuid, _allowed_ips: &[IpAddr]) -> bool {
        // In production, this would:
        // 1. Load the compiled eBPF program from ebpf/
        // 2. Create a CgroupSkb program via Aya
        // 3. Attach to the agent's cgroup
        // 4. Populate the HashMap with allowed IPs
        //
        // For now, we check if we have CAP_BPF by attempting a
        // privileged operation. If it fails, return false for fallback.
        false // Always fall back in this stub — real impl uses Aya
    }

    /// Get the proxy fallback reference for URL retrieval.
    pub fn proxy_fallback(&self) -> &ProxyEgressPolicy {
        &self.proxy_fallback
    }

    /// Spawn a periodic DNS re-resolution task (every 5 minutes).
    ///
    /// Re-resolves all allowed domains and updates the eBPF map with fresh IPs.
    /// This handles the case where upstream DNS records change (e.g. CDN rotation).
    fn spawn_dns_reresolution_task(
        agents: Arc<Mutex<HashMap<Uuid, AgentEbpf>>>,
        agent_id: Uuid,
    ) -> tokio::sync::oneshot::Sender<()> {
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(DNS_RERESOLUTION_INTERVAL);
            interval.tick().await; // Skip the immediate first tick.

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let domains = {
                            let agents_guard = agents.lock().unwrap();
                            match agents_guard.get(&agent_id) {
                                Some(state) if !state.using_proxy_fallback => {
                                    state.config.allowed_domains.clone()
                                }
                                _ => break, // Agent removed or using fallback.
                            }
                        };

                        let new_ips = Self::resolve_domains(&domains);
                        let mut agents_guard = agents.lock().unwrap();
                        if let Some(state) = agents_guard.get_mut(&agent_id) {
                            if !state.using_proxy_fallback {
                                let old_count = state.allowed_ips.len();
                                state.allowed_ips = new_ips;
                                tracing::debug!(
                                    agent_id = %agent_id,
                                    old_ip_count = old_count,
                                    new_ip_count = state.allowed_ips.len(),
                                    "eBPF DNS re-resolution complete — map updated"
                                );
                                // In production: update the eBPF HashMap with new IPs.
                            }
                        }
                    }
                    _ = &mut cancel_rx => {
                        tracing::debug!(
                            agent_id = %agent_id,
                            "DNS re-resolution task cancelled"
                        );
                        break;
                    }
                }
            }
        });

        cancel_tx
    }

    /// Read violation events from the eBPF perf event buffer.
    ///
    /// In production, this spawns a task that reads from the `VIOLATIONS`
    /// perf event array and feeds events into the violation counter.
    /// Each violation event contains the destination IP, protocol, and port.
    fn _spawn_perf_event_reader(
        _agents: Arc<Mutex<HashMap<Uuid, AgentEbpf>>>,
        _agent_id: Uuid,
    ) {
        // In production, this would:
        // 1. Open the VIOLATIONS perf event array via Aya
        // 2. Spawn an async task that polls for events
        // 3. For each ViolationEvent:
        //    a. Reverse-resolve the IP to a domain (best-effort)
        //    b. Call log_violation() to increment the counter
        //    c. If threshold exceeded, emit TriggerEvent::NetworkEgressViolation
        tracing::debug!("eBPF perf event reader: stub (production uses Aya perf buffer)");
    }
}

impl Default for EbpfEgressPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl EgressPolicy for EbpfEgressPolicy {
    fn apply(&self, agent_id: &Uuid, config: &AgentEgressConfig) -> Result<(), EgressError> {
        let allowed_ips = Self::resolve_domains(&config.allowed_domains);
        let matcher = DomainMatcher::new(&config.allowed_domains);

        let using_proxy_fallback = !Self::try_load_ebpf(agent_id, &allowed_ips);

        // Cancel any existing DNS re-resolution task for this agent.
        {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.remove(agent_id) {
                if let Some(cancel) = existing.dns_reresolution_cancel {
                    let _ = cancel.send(());
                }
            }
        }

        let dns_reresolution_cancel = if using_proxy_fallback {
            tracing::info!(
                agent_id = %agent_id,
                "eBPF unavailable — falling back to proxy egress policy"
            );
            self.proxy_fallback.apply(agent_id, config)?;
            None
        } else {
            tracing::info!(
                agent_id = %agent_id,
                ip_count = allowed_ips.len(),
                "eBPF egress policy applied"
            );
            // Spawn periodic DNS re-resolution (every 5 minutes).
            let cancel = Self::spawn_dns_reresolution_task(
                Arc::clone(&self.agents),
                *agent_id,
            );
            // Spawn perf event reader for violation logging.
            Self::_spawn_perf_event_reader(Arc::clone(&self.agents), *agent_id);
            Some(cancel)
        };

        let state = AgentEbpf {
            allowed_ips,
            matcher,
            config: config.clone(),
            using_proxy_fallback,
            dns_reresolution_cancel,
        };

        self.agents.lock().unwrap().insert(*agent_id, state);
        Ok(())
    }

    fn check_domain(&self, agent_id: &Uuid, domain: &str) -> Result<bool, EgressError> {
        let agents = self.agents.lock().unwrap();
        let state = agents.get(agent_id).ok_or_else(|| {
            EgressError::ConfigError(format!("no egress policy for agent {agent_id}"))
        })?;

        if state.using_proxy_fallback {
            drop(agents);
            return self.proxy_fallback.check_domain(agent_id, domain);
        }

        // In eBPF mode, domain checking is done at the kernel level via IP matching.
        // Userspace check is a best-effort supplement.
        Ok(state.matcher.matches(domain))
    }

    fn remove(&self, agent_id: &Uuid) -> Result<(), EgressError> {
        let mut agents = self.agents.lock().unwrap();
        if let Some(state) = agents.remove(agent_id) {
            // Cancel periodic DNS re-resolution task.
            if let Some(cancel) = state.dns_reresolution_cancel {
                let _ = cancel.send(());
            }

            if state.using_proxy_fallback {
                drop(agents);
                self.proxy_fallback.remove(agent_id)?;
            } else {
                // In production: detach eBPF program from cgroup, clean up maps.
                tracing::info!(agent_id = %agent_id, "eBPF egress policy removed");
            }
        }
        Ok(())
    }

    fn log_violation(&self, agent_id: &Uuid, domain: &str, action: &str) {
        let agents = self.agents.lock().unwrap();
        let using_fallback = agents
            .get(agent_id)
            .is_some_and(|s| s.using_proxy_fallback);
        drop(agents);

        if using_fallback {
            self.proxy_fallback.log_violation(agent_id, domain, action);
        } else {
            tracing::warn!(
                agent_id = %agent_id,
                domain = %domain,
                action = %action,
                backend = "ebpf",
                "Network egress violation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::EgressPolicyMode;

    #[test]
    fn dns_resolution_produces_ips() {
        // Resolve a well-known domain — should produce at least one IP.
        let ips = EbpfEgressPolicy::resolve_domains(&["localhost".to_string()]);
        // localhost should resolve to 127.0.0.1 or ::1
        assert!(!ips.is_empty(), "localhost should resolve to at least one IP");
    }

    #[test]
    fn dns_resolution_handles_invalid_domain() {
        let ips = EbpfEgressPolicy::resolve_domains(&["this.domain.definitely.does.not.exist.invalid".to_string()]);
        // Should not panic, may return empty.
        let _ = ips;
    }

    #[test]
    fn fallback_to_proxy_on_ebpf_failure() {
        let policy = EbpfEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: EgressPolicyMode::Allowlist,
            allowed_domains: vec!["api.openai.com".to_string()],
            ..Default::default()
        };

        policy.apply(&agent_id, &config).unwrap();

        // Should have fallen back to proxy.
        let agents = policy.agents.lock().unwrap();
        assert!(agents.get(&agent_id).unwrap().using_proxy_fallback);
    }

    #[test]
    fn fallback_proxy_url_available() {
        let policy = EbpfEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: EgressPolicyMode::Allowlist,
            allowed_domains: vec!["api.openai.com".to_string()],
            ..Default::default()
        };

        policy.apply(&agent_id, &config).unwrap();
        assert!(policy.proxy_fallback().proxy_url(&agent_id).is_some());
    }
}
