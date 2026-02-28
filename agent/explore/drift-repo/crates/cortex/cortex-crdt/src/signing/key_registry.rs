//! Agent key registry — maps agent IDs to their Ed25519 public keys.
//! Keys are registered when agents join a namespace.

use std::collections::HashMap;
use ed25519_dalek::VerifyingKey;
use cortex_core::models::agent::AgentId;

/// Registry of agent public keys for signature verification.
#[derive(Debug, Default)]
pub struct KeyRegistry {
    keys: HashMap<String, VerifyingKey>,
}

impl KeyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an agent's public key.
    pub fn register(&mut self, agent_id: &AgentId, key: VerifyingKey) {
        self.keys.insert(agent_id.0.clone(), key);
    }

    /// Remove an agent's key (revocation).
    pub fn revoke(&mut self, agent_id: &AgentId) {
        self.keys.remove(&agent_id.0);
    }

    /// Look up an agent's public key.
    pub fn get(&self, agent_id: &str) -> Option<&VerifyingKey> {
        self.keys.get(agent_id)
    }

    /// Check if an agent has a registered key.
    pub fn has_key(&self, agent_id: &str) -> bool {
        self.keys.contains_key(agent_id)
    }
}
