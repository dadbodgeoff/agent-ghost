//! macOS packet filter (pf) egress policy (Task 11.4).
//!
//! Uses `pfctl` to create per-agent pf anchors with IP-based rules.
//! Requires root privileges. Falls back to `ProxyEgressPolicy` if
//! pfctl fails with a permission error.
//!
//! Feature-gated: `#[cfg(all(target_os = "macos", feature = "pf"))]`

use std::collections::HashMap;
use std::net::IpAddr;
use std::process::Command;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use crate::config::AgentEgressConfig;
use crate::domain_matcher::DomainMatcher;
use crate::error::EgressError;
use crate::policy::EgressPolicy;
use crate::proxy_provider::ProxyEgressPolicy;

/// DNS re-resolution interval (5 minutes) for keeping pf rules current
/// when upstream DNS records change.
const DNS_RERESOLUTION_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Per-agent pf state.
#[derive(Debug)]
struct AgentPf {
    /// Anchor name: `ghost/{agent_id}`.
    anchor: String,
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

/// macOS pf-based egress policy.
///
/// Creates pf anchors per agent with rules allowing only resolved IPs
/// of allowed domains. Falls back to `ProxyEgressPolicy` if pfctl
/// fails with a permission error.
pub struct PfEgressPolicy {
    agents: Arc<Mutex<HashMap<Uuid, AgentPf>>>,
    /// Fallback proxy policy for when pf is unavailable.
    proxy_fallback: ProxyEgressPolicy,
}

impl PfEgressPolicy {
    /// Create a new pf egress policy manager.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
            proxy_fallback: ProxyEgressPolicy::new(),
        }
    }

    /// Build the pfctl command for creating an anchor.
    pub fn build_anchor_create_command(anchor: &str) -> Vec<String> {
        vec![
            "pfctl".to_string(),
            "-a".to_string(),
            anchor.to_string(),
            "-f".to_string(),
            "-".to_string(),
        ]
    }

    /// Build pf rules for the allowed IPs.
    pub fn build_rules(allowed_ips: &[IpAddr]) -> String {
        let mut rules = String::new();
        for ip in allowed_ips {
            rules.push_str(&format!("pass out proto tcp to {ip}\n"));
            rules.push_str(&format!("pass out proto udp to {ip}\n"));
        }
        // Block everything else from this anchor.
        rules.push_str("block out all\n");
        rules
    }

    /// Build the pfctl command for flushing an anchor.
    pub fn build_anchor_flush_command(anchor: &str) -> Vec<String> {
        vec![
            "pfctl".to_string(),
            "-a".to_string(),
            anchor.to_string(),
            "-F".to_string(),
            "all".to_string(),
        ]
    }

    /// Resolve domains to IP addresses (same pattern as eBPF).
    fn resolve_domains(domains: &[String]) -> Vec<IpAddr> {
        let mut ips = Vec::new();
        for domain in domains {
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
                        "DNS resolution failed for pf allowlist"
                    );
                }
            }
        }
        ips
    }

    /// Attempt to apply pf rules. Returns Err if permission denied.
    fn try_apply_pf(anchor: &str, rules: &str) -> Result<(), EgressError> {
        let cmd_args = Self::build_anchor_create_command(anchor);
        let output = Command::new(&cmd_args[0])
            .args(&cmd_args[1..])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .and_then(|mut child| {
                use std::io::Write;
                if let Some(ref mut stdin) = child.stdin {
                    stdin.write_all(rules.as_bytes())?;
                }
                child.wait_with_output()
            });

        match output {
            Ok(out) if out.status.success() => Ok(()),
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                if stderr.contains("Permission denied") || stderr.contains("Operation not permitted")
                {
                    Err(EgressError::ProviderUnavailable(
                        "pfctl requires root privileges".to_string(),
                    ))
                } else {
                    Err(EgressError::ProviderUnavailable(format!(
                        "pfctl failed: {stderr}"
                    )))
                }
            }
            Err(e) => Err(EgressError::ProviderUnavailable(format!(
                "pfctl not available: {e}"
            ))),
        }
    }

    /// Get the proxy fallback reference.
    pub fn proxy_fallback(&self) -> &ProxyEgressPolicy {
        &self.proxy_fallback
    }

    /// Spawn a periodic DNS re-resolution task (every 5 minutes).
    ///
    /// Re-resolves all allowed domains and updates pf anchor rules with fresh IPs.
    /// This handles the case where upstream DNS records change (e.g. CDN rotation).
    fn spawn_dns_reresolution_task(
        agents: Arc<Mutex<HashMap<Uuid, AgentPf>>>,
        agent_id: Uuid,
    ) -> tokio::sync::oneshot::Sender<()> {
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(DNS_RERESOLUTION_INTERVAL);
            interval.tick().await; // Skip the immediate first tick.

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let (domains, anchor) = {
                            let agents_guard = agents.lock().unwrap();
                            match agents_guard.get(&agent_id) {
                                Some(state) if !state.using_proxy_fallback => {
                                    (state.config.allowed_domains.clone(), state.anchor.clone())
                                }
                                _ => break, // Agent removed or using fallback.
                            }
                        };

                        let new_ips = Self::resolve_domains(&domains);
                        let rules = Self::build_rules(&new_ips);

                        // Update pf anchor with new rules.
                        if let Err(e) = Self::try_apply_pf(&anchor, &rules) {
                            tracing::warn!(
                                agent_id = %agent_id,
                                error = %e,
                                "pf DNS re-resolution: failed to update anchor rules"
                            );
                        }

                        let mut agents_guard = agents.lock().unwrap();
                        if let Some(state) = agents_guard.get_mut(&agent_id) {
                            if !state.using_proxy_fallback {
                                let old_count = state.allowed_ips.len();
                                state.allowed_ips = new_ips;
                                tracing::debug!(
                                    agent_id = %agent_id,
                                    old_ip_count = old_count,
                                    new_ip_count = state.allowed_ips.len(),
                                    "pf DNS re-resolution complete — anchor rules updated"
                                );
                            }
                        }
                    }
                    _ = &mut cancel_rx => {
                        tracing::debug!(
                            agent_id = %agent_id,
                            "pf DNS re-resolution task cancelled"
                        );
                        break;
                    }
                }
            }
        });

        cancel_tx
    }
}

impl Default for PfEgressPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl EgressPolicy for PfEgressPolicy {
    fn apply(&self, agent_id: &Uuid, config: &AgentEgressConfig) -> Result<(), EgressError> {
        let allowed_ips = Self::resolve_domains(&config.allowed_domains);
        let matcher = DomainMatcher::new(&config.allowed_domains);
        let anchor = format!("ghost/{}", agent_id);
        let rules = Self::build_rules(&allowed_ips);

        // Cancel any existing DNS re-resolution task for this agent.
        {
            let mut agents = self.agents.lock().unwrap();
            if let Some(existing) = agents.remove(agent_id) {
                if let Some(cancel) = existing.dns_reresolution_cancel {
                    let _ = cancel.send(());
                }
            }
        }

        let (using_proxy_fallback, dns_reresolution_cancel) = match Self::try_apply_pf(&anchor, &rules) {
            Ok(()) => {
                tracing::info!(
                    agent_id = %agent_id,
                    anchor = %anchor,
                    ip_count = allowed_ips.len(),
                    "pf egress policy applied"
                );
                // Spawn periodic DNS re-resolution (every 5 minutes).
                let cancel = Self::spawn_dns_reresolution_task(
                    Arc::clone(&self.agents),
                    *agent_id,
                );
                (false, Some(cancel))
            }
            Err(EgressError::ProviderUnavailable(reason)) => {
                tracing::info!(
                    agent_id = %agent_id,
                    reason = %reason,
                    "pf unavailable — falling back to proxy egress policy"
                );
                self.proxy_fallback.apply(agent_id, config)?;
                (true, None)
            }
            Err(e) => return Err(e),
        };

        let state = AgentPf {
            anchor,
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
                // Flush the pf anchor.
                let cmd_args = Self::build_anchor_flush_command(&state.anchor);
                match Command::new(&cmd_args[0]).args(&cmd_args[1..]).output() {
                    Ok(out) if !out.status.success() => {
                        tracing::warn!(
                            agent_id = %agent_id,
                            anchor = %state.anchor,
                            stderr = %String::from_utf8_lossy(&out.stderr),
                            "pf anchor flush failed"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            agent_id = %agent_id,
                            anchor = %state.anchor,
                            error = %e,
                            "pf anchor flush command failed"
                        );
                    }
                    _ => {}
                }
                tracing::info!(
                    agent_id = %agent_id,
                    anchor = %state.anchor,
                    "pf egress policy removed"
                );
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
                backend = "pf",
                "Network egress violation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_create_command_correct() {
        let cmd = PfEgressPolicy::build_anchor_create_command("ghost/test-agent");
        assert_eq!(cmd, vec!["pfctl", "-a", "ghost/test-agent", "-f", "-"]);
    }

    #[test]
    fn anchor_flush_command_correct() {
        let cmd = PfEgressPolicy::build_anchor_flush_command("ghost/test-agent");
        assert_eq!(cmd, vec!["pfctl", "-a", "ghost/test-agent", "-F", "all"]);
    }

    #[test]
    fn build_rules_correct() {
        let ips = vec![
            "1.2.3.4".parse::<IpAddr>().unwrap(),
            "5.6.7.8".parse::<IpAddr>().unwrap(),
        ];
        let rules = PfEgressPolicy::build_rules(&ips);
        assert!(rules.contains("pass out proto tcp to 1.2.3.4"));
        assert!(rules.contains("pass out proto udp to 1.2.3.4"));
        assert!(rules.contains("pass out proto tcp to 5.6.7.8"));
        assert!(rules.contains("block out all"));
    }

    #[test]
    fn fallback_to_proxy_on_permission_error() {
        let policy = PfEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: crate::config::EgressPolicyMode::Allowlist,
            allowed_domains: vec!["api.openai.com".to_string()],
            ..Default::default()
        };

        // On non-macOS or without root, this should fall back to proxy.
        policy.apply(&agent_id, &config).unwrap();

        let agents = policy.agents.lock().unwrap();
        assert!(agents.get(&agent_id).unwrap().using_proxy_fallback);
    }
}
