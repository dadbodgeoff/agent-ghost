//! Proxy configuration for LLM provider HTTP clients (Phase 11).
//!
//! When `ProxyEgressPolicy` is active, the agent's reqwest client must
//! route all requests through the localhost proxy. This module provides
//! the configuration bridge between `ghost-egress` and `ghost-llm`.
//!
//! Usage:
//! ```ignore
//! let proxy_config = ProxyConfig::from_url("http://127.0.0.1:12345");
//! let client = proxy_config.build_client()?;
//! // Use `client` for all LLM API calls — requests go through the proxy.
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

/// Per-agent proxy configuration for LLM HTTP clients.
///
/// When a `ProxyEgressPolicy` is active, each agent's LLM requests
/// must be routed through its assigned localhost proxy. This struct
/// holds the proxy URL and provides a method to build a configured
/// reqwest client.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Proxy URL, e.g. `http://127.0.0.1:12345`.
    pub proxy_url: String,
}

impl ProxyConfig {
    /// Create a proxy config from a URL.
    pub fn from_url(url: &str) -> Self {
        Self {
            proxy_url: url.to_string(),
        }
    }

    /// Build a reqwest client configured to use this proxy.
    ///
    /// All HTTP/HTTPS requests made through this client will be routed
    /// through the proxy, which enforces the agent's egress policy.
    pub fn build_client(&self) -> Result<reqwest::Client, reqwest::Error> {
        let proxy = reqwest::Proxy::all(&self.proxy_url)?;
        reqwest::Client::builder()
            .proxy(proxy)
            .build()
    }
}

/// Registry of per-agent proxy configurations.
///
/// Used by the gateway bootstrap to register proxy URLs after
/// `ProxyEgressPolicy::apply()`, and by the agent loop to retrieve
/// the configured client for LLM calls.
#[derive(Debug, Clone, Default)]
pub struct ProxyRegistry {
    configs: Arc<Mutex<HashMap<Uuid, ProxyConfig>>>,
}

impl ProxyRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            configs: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a proxy URL for an agent.
    pub fn register(&self, agent_id: Uuid, proxy_url: &str) {
        let config = ProxyConfig::from_url(proxy_url);
        self.configs.lock().unwrap().insert(agent_id, config);
        tracing::debug!(
            agent_id = %agent_id,
            proxy_url = %proxy_url,
            "Registered proxy config for LLM client"
        );
    }

    /// Remove the proxy config for an agent.
    pub fn unregister(&self, agent_id: &Uuid) {
        self.configs.lock().unwrap().remove(agent_id);
    }

    /// Get the proxy config for an agent, if one is registered.
    pub fn get(&self, agent_id: &Uuid) -> Option<ProxyConfig> {
        self.configs.lock().unwrap().get(agent_id).cloned()
    }

    /// Build a reqwest client for an agent.
    ///
    /// If a proxy is registered, returns a proxy-configured client.
    /// Otherwise, returns a default client (no proxy).
    pub fn build_client_for_agent(&self, agent_id: &Uuid) -> Result<reqwest::Client, reqwest::Error> {
        match self.get(agent_id) {
            Some(config) => config.build_client(),
            None => reqwest::Client::builder().build(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_config_from_url() {
        let config = ProxyConfig::from_url("http://127.0.0.1:12345");
        assert_eq!(config.proxy_url, "http://127.0.0.1:12345");
    }

    #[test]
    fn proxy_registry_register_and_get() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        assert!(registry.get(&agent).is_none());

        registry.register(agent, "http://127.0.0.1:9999");
        let config = registry.get(&agent).unwrap();
        assert_eq!(config.proxy_url, "http://127.0.0.1:9999");
    }

    #[test]
    fn proxy_registry_unregister() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        registry.register(agent, "http://127.0.0.1:9999");
        assert!(registry.get(&agent).is_some());

        registry.unregister(&agent);
        assert!(registry.get(&agent).is_none());
    }

    #[test]
    fn build_client_without_proxy() {
        let registry = ProxyRegistry::new();
        let agent = Uuid::new_v4();

        // No proxy registered — should build a default client.
        let client = registry.build_client_for_agent(&agent);
        assert!(client.is_ok());
    }
}
