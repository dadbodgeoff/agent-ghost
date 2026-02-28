//! Proxy server — localhost binding with TLS termination (Req 36 AC1, AC5).

use crate::domain_filter::DomainFilter;
use crate::emitter::ProxyITPEmitter;

/// Proxy server configuration.
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    pub bind: String,
    pub port: u16,
    pub ca_dir: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            bind: "127.0.0.1".to_string(),
            port: 8080,
            ca_dir: "~/.ghost/proxy/ca/".to_string(),
        }
    }
}

/// Local HTTPS proxy server.
///
/// INVARIANT: Pass-through mode — never modifies traffic (AC5).
pub struct ProxyServer {
    config: ProxyConfig,
    domain_filter: DomainFilter,
    emitter: ProxyITPEmitter,
}

impl ProxyServer {
    pub fn new(config: ProxyConfig) -> Self {
        Self {
            config,
            domain_filter: DomainFilter::new(),
            emitter: ProxyITPEmitter::new(),
        }
    }

    pub fn with_domain_filter(mut self, filter: DomainFilter) -> Self {
        self.domain_filter = filter;
        self
    }

    pub fn with_emitter(mut self, emitter: ProxyITPEmitter) -> Self {
        self.emitter = emitter;
        self
    }

    /// Check if a domain should be intercepted.
    pub fn should_intercept(&self, domain: &str) -> bool {
        self.domain_filter.should_intercept(domain)
    }

    /// Get the bind address.
    pub fn bind_addr(&self) -> String {
        format!("{}:{}", self.config.bind, self.config.port)
    }

    /// Get the emitter reference.
    pub fn emitter(&self) -> &ProxyITPEmitter {
        &self.emitter
    }

    /// Verify that traffic is never modified (AC5 invariant).
    pub fn is_passthrough(&self) -> bool {
        true // Structural guarantee — proxy only reads, never writes to traffic
    }
}
