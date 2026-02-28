//! Agent discovery: local registry + remote discovery with cached AgentCards.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::error::MeshError;
use crate::transport::a2a_client::A2AClient;
use crate::types::AgentCard;

/// Cached agent card with TTL.
#[derive(Debug, Clone)]
struct CachedCard {
    card: AgentCard,
    fetched_at: Instant,
}

/// Agent discovery: local registry of known agents + remote discovery
/// with signature verification and TTL-based caching.
pub struct AgentDiscovery {
    /// Known agents from ghost.yml mesh config: name → (endpoint, public_key).
    known_agents: BTreeMap<String, KnownAgentConfig>,
    /// Cached agent cards: endpoint → cached card.
    cache: BTreeMap<String, CachedCard>,
    /// Cache TTL (default 1 hour).
    cache_ttl: Duration,
    /// A2A client for remote discovery.
    client: A2AClient,
}

/// Configuration for a known agent from ghost.yml.
#[derive(Debug, Clone)]
pub struct KnownAgentConfig {
    pub name: String,
    pub endpoint: String,
    pub public_key: Vec<u8>,
}

impl AgentDiscovery {
    pub fn new(known_agents: Vec<KnownAgentConfig>, cache_ttl: Duration) -> Self {
        let known_map: BTreeMap<String, KnownAgentConfig> = known_agents
            .into_iter()
            .map(|a| (a.name.clone(), a))
            .collect();
        Self {
            known_agents: known_map,
            cache: BTreeMap::new(),
            cache_ttl,
            client: A2AClient::default(),
        }
    }

    /// Discover an agent by endpoint. Uses cache if available and not expired.
    pub async fn discover(&mut self, endpoint: &str) -> Result<AgentCard, MeshError> {
        // Check cache first.
        if let Some(cached) = self.cache.get(endpoint) {
            if cached.fetched_at.elapsed() < self.cache_ttl {
                return Ok(cached.card.clone());
            }
            // Expired — remove and re-fetch.
        }

        // Fetch from remote.
        let card = self.client.discover_agent(endpoint).await?;

        // Verify signature.
        if !card.verify_signature() {
            return Err(MeshError::AuthenticationFailed {
                reason: "agent card signature verification failed".to_string(),
            });
        }

        // Cache the card.
        self.cache.insert(
            endpoint.to_string(),
            CachedCard {
                card: card.clone(),
                fetched_at: Instant::now(),
            },
        );

        Ok(card)
    }

    /// Get a known agent by name from the local registry.
    pub fn get_known_agent(&self, name: &str) -> Option<&KnownAgentConfig> {
        self.known_agents.get(name)
    }

    /// List all known agent names.
    pub fn known_agent_names(&self) -> Vec<&str> {
        self.known_agents.keys().map(|s| s.as_str()).collect()
    }

    /// Get a cached agent card by endpoint (without fetching).
    pub fn get_cached(&self, endpoint: &str) -> Option<&AgentCard> {
        self.cache
            .get(endpoint)
            .filter(|c| c.fetched_at.elapsed() < self.cache_ttl)
            .map(|c| &c.card)
    }

    /// Check if a cached card has expired.
    pub fn is_cache_expired(&self, endpoint: &str) -> bool {
        self.cache
            .get(endpoint)
            .map_or(true, |c| c.fetched_at.elapsed() >= self.cache_ttl)
    }

    /// Invalidate the cache for a specific endpoint.
    pub fn invalidate_cache(&mut self, endpoint: &str) {
        self.cache.remove(endpoint);
    }

    /// Invalidate all cached cards.
    pub fn invalidate_all_cache(&mut self) {
        self.cache.clear();
    }
}

impl Default for AgentDiscovery {
    fn default() -> Self {
        Self::new(vec![], Duration::from_secs(3600))
    }
}
