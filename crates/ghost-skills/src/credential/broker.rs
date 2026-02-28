//! Credential broker: opaque tokens reified only at execution time (Req 23 AC4).
//!
//! Stand-in pattern: skills receive an opaque token handle, never the raw credential.
//! The broker reifies the token inside the sandbox at execution time.
//! max_uses enforcement prevents credential replay.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use uuid::Uuid;

/// An opaque credential handle given to skills.
/// The skill never sees the raw credential — only this handle.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CredentialHandle {
    pub id: Uuid,
    pub provider: String,
    pub scope: String,
}

/// Internal credential entry (never exposed to skills).
struct CredentialEntry {
    handle: CredentialHandle,
    /// The actual secret value — only reified inside the sandbox.
    secret: String,
    max_uses: u32,
    use_count: u32,
    created_at: DateTime<Utc>,
    expires_at: Option<DateTime<Utc>>,
}

/// Credential broker manages opaque tokens for skill execution.
pub struct CredentialBroker {
    credentials: BTreeMap<Uuid, CredentialEntry>,
}

impl CredentialBroker {
    pub fn new() -> Self {
        Self {
            credentials: BTreeMap::new(),
        }
    }

    /// Register a credential. Returns an opaque handle for the skill.
    pub fn register(
        &mut self,
        provider: String,
        scope: String,
        secret: String,
        max_uses: u32,
        expires_at: Option<DateTime<Utc>>,
    ) -> CredentialHandle {
        let handle = CredentialHandle {
            id: Uuid::now_v7(),
            provider: provider.clone(),
            scope: scope.clone(),
        };
        self.credentials.insert(
            handle.id,
            CredentialEntry {
                handle: handle.clone(),
                secret,
                max_uses,
                use_count: 0,
                created_at: Utc::now(),
                expires_at,
            },
        );
        handle
    }

    /// Reify a credential inside the sandbox. Consumes one use.
    /// Returns None if the credential is exhausted, expired, or not found.
    pub fn reify(&mut self, handle_id: Uuid) -> Result<String, CredentialError> {
        let entry = self
            .credentials
            .get_mut(&handle_id)
            .ok_or(CredentialError::NotFound(handle_id))?;

        // Check expiration
        if let Some(expires) = entry.expires_at {
            if Utc::now() > expires {
                return Err(CredentialError::Expired {
                    handle_id,
                    expired_at: expires,
                });
            }
        }

        // Check max_uses
        if entry.use_count >= entry.max_uses {
            return Err(CredentialError::Exhausted {
                handle_id,
                max_uses: entry.max_uses,
            });
        }

        entry.use_count += 1;
        Ok(entry.secret.clone())
    }

    /// Revoke a credential (e.g., on quarantine).
    pub fn revoke(&mut self, handle_id: Uuid) -> bool {
        self.credentials.remove(&handle_id).is_some()
    }

    /// Revoke all credentials for a given provider.
    pub fn revoke_provider(&mut self, provider: &str) {
        self.credentials
            .retain(|_, entry| entry.handle.provider != provider);
    }

    /// Get remaining uses for a credential.
    pub fn remaining_uses(&self, handle_id: Uuid) -> Option<u32> {
        self.credentials
            .get(&handle_id)
            .map(|e| e.max_uses.saturating_sub(e.use_count))
    }
}

/// Credential broker errors.
#[derive(Debug, Clone)]
pub enum CredentialError {
    NotFound(Uuid),
    Exhausted { handle_id: Uuid, max_uses: u32 },
    Expired { handle_id: Uuid, expired_at: DateTime<Utc> },
}

impl std::fmt::Display for CredentialError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "Credential {id} not found"),
            Self::Exhausted { handle_id, max_uses } => {
                write!(f, "Credential {handle_id} exhausted (max_uses={max_uses})")
            }
            Self::Expired { handle_id, expired_at } => {
                write!(f, "Credential {handle_id} expired at {expired_at}")
            }
        }
    }
}

impl Default for CredentialBroker {
    fn default() -> Self {
        Self::new()
    }
}
