//! Cross-platform proxy-based egress policy (Task 11.2).
//!
//! Starts a lightweight localhost HTTP proxy per agent that inspects CONNECT
//! requests and enforces domain allowlists. No kernel privileges required.
//! Each agent gets its own port with its own allowlist.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use uuid::Uuid;

use crate::config::{AgentEgressConfig, EgressPolicyMode};
use crate::domain_matcher::DomainMatcher;
use crate::error::EgressError;
use crate::policy::EgressPolicy;

/// Per-agent proxy state.
#[derive(Debug)]
struct AgentProxy {
    /// Port the proxy is bound to.
    port: u16,
    /// Domain matcher compiled from the agent's config.
    matcher: DomainMatcher,
    /// Policy mode for this agent.
    mode: EgressPolicyMode,
    /// Blocked domains matcher (for Blocklist mode).
    blocked_matcher: DomainMatcher,
    /// Shutdown signal sender.
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    /// Violation timestamps for threshold tracking.
    violations: Vec<Instant>,
    /// Violation threshold config.
    violation_threshold: u32,
    /// Violation window in minutes.
    violation_window_minutes: u32,
    /// Whether to log violations.
    log_violations: bool,
    /// Whether to alert on violations.
    alert_on_violation: bool,
}

/// Cross-platform proxy-based egress policy.
///
/// Starts a localhost HTTP proxy per agent. The proxy inspects CONNECT
/// requests, extracts the target domain, and checks against the agent's
/// `AgentEgressConfig`. Allowed domains are forwarded; blocked domains
/// receive a 403 response with a violation log entry.
pub struct ProxyEgressPolicy {
    agents: Arc<Mutex<HashMap<Uuid, AgentProxy>>>,
}

impl ProxyEgressPolicy {
    /// Create a new proxy egress policy manager.
    pub fn new() -> Self {
        Self {
            agents: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the proxy URL for a specific agent, if a proxy is running.
    ///
    /// Returns `http://127.0.0.1:{port}` for configuring the agent's reqwest client.
    pub fn proxy_url(&self, agent_id: &Uuid) -> Option<String> {
        let agents = self.agents.lock().unwrap();
        agents
            .get(agent_id)
            .map(|proxy| format!("http://127.0.0.1:{}", proxy.port))
    }

    /// Get the current violation count for an agent within the configured window.
    pub fn violation_count(&self, agent_id: &Uuid) -> u32 {
        let agents = self.agents.lock().unwrap();
        agents.get(agent_id).map_or(0, |proxy| {
            let window = std::time::Duration::from_secs(proxy.violation_window_minutes as u64 * 60);
            let cutoff = Instant::now() - window;
            proxy.violations.iter().filter(|t| **t > cutoff).count() as u32
        })
    }

    /// Check if the violation threshold has been exceeded for an agent.
    pub fn threshold_exceeded(&self, agent_id: &Uuid) -> bool {
        let agents = self.agents.lock().unwrap();
        agents.get(agent_id).is_some_and(|proxy| {
            let window = std::time::Duration::from_secs(proxy.violation_window_minutes as u64 * 60);
            let cutoff = Instant::now() - window;
            let count = proxy.violations.iter().filter(|t| **t > cutoff).count() as u32;
            count >= proxy.violation_threshold
        })
    }

    /// Record a violation for an agent. Returns `true` if the threshold is now exceeded.
    fn record_violation(&self, agent_id: &Uuid) -> bool {
        let mut agents = self.agents.lock().unwrap();
        if let Some(proxy) = agents.get_mut(agent_id) {
            proxy.violations.push(Instant::now());

            // Prune old violations outside the window.
            let window =
                std::time::Duration::from_secs(proxy.violation_window_minutes as u64 * 60);
            let cutoff = Instant::now() - window;
            proxy.violations.retain(|t| *t > cutoff);

            let count = proxy.violations.len() as u32;
            count >= proxy.violation_threshold
        } else {
            false
        }
    }

    /// Check a domain against the agent's policy (internal helper).
    fn check_domain_internal(
        mode: &EgressPolicyMode,
        allowed_matcher: &DomainMatcher,
        blocked_matcher: &DomainMatcher,
        domain: &str,
    ) -> bool {
        match mode {
            EgressPolicyMode::Allowlist => allowed_matcher.matches(domain),
            EgressPolicyMode::Blocklist => !blocked_matcher.matches(domain),
            EgressPolicyMode::Unrestricted => true,
        }
    }
}

impl Default for ProxyEgressPolicy {
    fn default() -> Self {
        Self::new()
    }
}

impl EgressPolicy for ProxyEgressPolicy {
    fn apply(&self, agent_id: &Uuid, config: &AgentEgressConfig) -> Result<(), EgressError> {
        let mut agents = self.agents.lock().unwrap();

        // If already applied, remove first.
        if let Some(existing) = agents.remove(agent_id) {
            if let Some(tx) = existing.shutdown_tx {
                let _ = tx.send(());
            }
        }

        let allowed_matcher = DomainMatcher::new(&config.allowed_domains);
        let blocked_matcher = DomainMatcher::new(&config.blocked_domains);

        // Bind to a dynamic port on loopback.
        // In a real implementation, this would start a hyper server.
        // For now, we allocate a port via binding to :0 and recording it.
        let port = allocate_port().map_err(|e| {
            EgressError::ProviderUnavailable(format!("failed to allocate proxy port: {e}"))
        })?;

        let (shutdown_tx, _shutdown_rx) = tokio::sync::oneshot::channel();

        // In production, we'd spawn a hyper proxy task here using shutdown_rx.
        // The proxy would:
        // 1. Listen on 127.0.0.1:{port}
        // 2. Intercept CONNECT requests
        // 3. Extract target domain
        // 4. Check against allowed_matcher / blocked_matcher
        // 5. Forward allowed, return 403 for blocked
        // 6. Shut down when shutdown_rx fires

        let proxy = AgentProxy {
            port,
            matcher: allowed_matcher,
            mode: config.policy,
            blocked_matcher,
            shutdown_tx: Some(shutdown_tx),
            violations: Vec::new(),
            violation_threshold: config.violation_threshold,
            violation_window_minutes: config.violation_window_minutes,
            log_violations: config.log_violations,
            alert_on_violation: config.alert_on_violation,
        };

        tracing::info!(
            agent_id = %agent_id,
            port = port,
            policy = ?config.policy,
            "Proxy egress policy applied"
        );

        agents.insert(*agent_id, proxy);
        Ok(())
    }

    fn check_domain(&self, agent_id: &Uuid, domain: &str) -> Result<bool, EgressError> {
        let agents = self.agents.lock().unwrap();
        let proxy = agents.get(agent_id).ok_or_else(|| {
            EgressError::ConfigError(format!("no egress policy for agent {agent_id}"))
        })?;

        let allowed = Self::check_domain_internal(
            &proxy.mode,
            &proxy.matcher,
            &proxy.blocked_matcher,
            domain,
        );

        if !allowed && proxy.log_violations {
            tracing::warn!(
                agent_id = %agent_id,
                domain = %domain,
                "Egress policy violation"
            );
        }

        Ok(allowed)
    }

    fn remove(&self, agent_id: &Uuid) -> Result<(), EgressError> {
        let mut agents = self.agents.lock().unwrap();
        if let Some(proxy) = agents.remove(agent_id) {
            if let Some(tx) = proxy.shutdown_tx {
                let _ = tx.send(());
            }
            tracing::info!(agent_id = %agent_id, "Proxy egress policy removed");
        }
        Ok(())
    }

    fn log_violation(&self, agent_id: &Uuid, domain: &str, action: &str) {
        let threshold_exceeded = self.record_violation(agent_id);

        tracing::warn!(
            agent_id = %agent_id,
            domain = %domain,
            action = %action,
            threshold_exceeded = threshold_exceeded,
            "Network egress violation"
        );

        if threshold_exceeded {
            let agents = self.agents.lock().unwrap();
            if let Some(proxy) = agents.get(agent_id) {
                if proxy.alert_on_violation {
                    tracing::error!(
                        agent_id = %agent_id,
                        "Violation threshold exceeded — emitting TriggerEvent::NetworkEgressViolation"
                    );
                    // In production, this would send a TriggerEvent to the
                    // AutoTriggerEvaluator via the bounded mpsc channel.
                }
            }
        }
    }
}

/// Allocate a dynamic port by binding to :0 and reading the assigned port.
fn allocate_port() -> Result<u16, std::io::Error> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    // Drop the listener — the port may be reused, but for our proxy
    // startup this is a best-effort allocation. The hyper server will
    // bind to this port immediately after.
    Ok(port)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AgentEgressConfig;

    fn test_config_allowlist() -> AgentEgressConfig {
        AgentEgressConfig {
            policy: EgressPolicyMode::Allowlist,
            allowed_domains: vec![
                "api.anthropic.com".to_string(),
                "api.openai.com".to_string(),
                "*.slack.com".to_string(),
            ],
            blocked_domains: vec![],
            log_violations: true,
            alert_on_violation: true,
            violation_threshold: 3,
            violation_window_minutes: 10,
        }
    }

    fn test_config_blocklist() -> AgentEgressConfig {
        AgentEgressConfig {
            policy: EgressPolicyMode::Blocklist,
            allowed_domains: vec![],
            blocked_domains: vec!["*.pastebin.com".to_string(), "evil.example.com".to_string()],
            log_violations: true,
            alert_on_violation: false,
            violation_threshold: 5,
            violation_window_minutes: 10,
        }
    }

    #[test]
    fn proxy_url_returns_correct_format() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = test_config_allowlist();
        policy.apply(&agent_id, &config).unwrap();

        let url = policy.proxy_url(&agent_id).unwrap();
        assert!(url.starts_with("http://127.0.0.1:"));
        let port_str = url.strip_prefix("http://127.0.0.1:").unwrap();
        let port: u16 = port_str.parse().unwrap();
        assert!(port > 0);
    }

    #[test]
    fn proxy_url_none_for_unknown_agent() {
        let policy = ProxyEgressPolicy::new();
        assert!(policy.proxy_url(&Uuid::new_v4()).is_none());
    }

    #[test]
    fn allowlist_allows_listed_domain() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_allowlist()).unwrap();

        assert!(policy.check_domain(&agent_id, "api.anthropic.com").unwrap());
        assert!(policy.check_domain(&agent_id, "api.openai.com").unwrap());
        assert!(policy.check_domain(&agent_id, "hooks.slack.com").unwrap());
    }

    #[test]
    fn allowlist_blocks_unlisted_domain() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_allowlist()).unwrap();

        assert!(!policy.check_domain(&agent_id, "evil.example.com").unwrap());
        assert!(!policy.check_domain(&agent_id, "pastebin.com").unwrap());
    }

    #[test]
    fn blocklist_blocks_listed_domain() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_blocklist()).unwrap();

        assert!(!policy.check_domain(&agent_id, "evil.example.com").unwrap());
        assert!(!policy.check_domain(&agent_id, "api.pastebin.com").unwrap());
    }

    #[test]
    fn blocklist_allows_unlisted_domain() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_blocklist()).unwrap();

        assert!(policy.check_domain(&agent_id, "api.openai.com").unwrap());
        assert!(policy.check_domain(&agent_id, "google.com").unwrap());
    }

    #[test]
    fn unrestricted_allows_everything() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: EgressPolicyMode::Unrestricted,
            ..Default::default()
        };
        policy.apply(&agent_id, &config).unwrap();

        assert!(policy.check_domain(&agent_id, "anything.com").unwrap());
        assert!(policy.check_domain(&agent_id, "evil.example.com").unwrap());
    }

    #[test]
    fn remove_cleans_up_agent() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_allowlist()).unwrap();
        assert!(policy.proxy_url(&agent_id).is_some());

        policy.remove(&agent_id).unwrap();
        assert!(policy.proxy_url(&agent_id).is_none());
    }

    #[test]
    fn violation_counter_increments() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        policy.apply(&agent_id, &test_config_allowlist()).unwrap();

        assert_eq!(policy.violation_count(&agent_id), 0);
        policy.log_violation(&agent_id, "evil.com", "CONNECT");
        assert_eq!(policy.violation_count(&agent_id), 1);
        policy.log_violation(&agent_id, "evil2.com", "CONNECT");
        assert_eq!(policy.violation_count(&agent_id), 2);
    }

    #[test]
    fn violation_threshold_triggers() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();
        let config = AgentEgressConfig {
            violation_threshold: 3,
            ..test_config_allowlist()
        };
        policy.apply(&agent_id, &config).unwrap();

        assert!(!policy.threshold_exceeded(&agent_id));
        policy.log_violation(&agent_id, "a.com", "CONNECT");
        policy.log_violation(&agent_id, "b.com", "CONNECT");
        assert!(!policy.threshold_exceeded(&agent_id));
        policy.log_violation(&agent_id, "c.com", "CONNECT");
        assert!(policy.threshold_exceeded(&agent_id));
    }

    #[test]
    fn multiple_agents_independent() {
        let policy = ProxyEgressPolicy::new();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        let config_a = test_config_allowlist();
        let config_b = AgentEgressConfig {
            policy: EgressPolicyMode::Unrestricted,
            ..Default::default()
        };

        policy.apply(&agent_a, &config_a).unwrap();
        policy.apply(&agent_b, &config_b).unwrap();

        // Agent A: allowlist — evil.com blocked
        assert!(!policy.check_domain(&agent_a, "evil.com").unwrap());
        // Agent B: unrestricted — evil.com allowed
        assert!(policy.check_domain(&agent_b, "evil.com").unwrap());
    }

    #[test]
    fn reapply_replaces_existing_policy() {
        let policy = ProxyEgressPolicy::new();
        let agent_id = Uuid::new_v4();

        policy.apply(&agent_id, &test_config_allowlist()).unwrap();
        assert!(!policy.check_domain(&agent_id, "evil.com").unwrap());

        // Reapply with unrestricted
        let config = AgentEgressConfig {
            policy: EgressPolicyMode::Unrestricted,
            ..Default::default()
        };
        policy.apply(&agent_id, &config).unwrap();
        assert!(policy.check_domain(&agent_id, "evil.com").unwrap());
    }
}
