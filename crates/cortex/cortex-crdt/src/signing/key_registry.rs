//! Key registry for CRDT delta verification (Req 29 AC3).
//!
//! Populated from ghost-identity key files during bootstrap.
//! Dual registration: same public key registered in both MessageDispatcher
//! and cortex-crdt KeyRegistry.

use std::collections::BTreeMap;

use uuid::Uuid;

/// Registry of agent public keys for delta signature verification.
#[derive(Debug, Default)]
pub struct KeyRegistry {
    keys: BTreeMap<Uuid, ed25519_dalek::VerifyingKey>,
}

impl KeyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a public key for an agent.
    pub fn register(&mut self, agent_id: Uuid, key: ed25519_dalek::VerifyingKey) {
        self.keys.insert(agent_id, key);
    }

    /// Look up a public key by agent ID.
    pub fn get(&self, agent_id: &Uuid) -> Option<&ed25519_dalek::VerifyingKey> {
        self.keys.get(agent_id)
    }

    /// Remove a key (e.g., on agent deregistration).
    pub fn remove(&mut self, agent_id: &Uuid) -> Option<ed25519_dalek::VerifyingKey> {
        self.keys.remove(agent_id)
    }

    /// Number of registered keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}
