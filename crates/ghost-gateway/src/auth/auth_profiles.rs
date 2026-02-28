//! Auth profile management for per-provider credential rotation.

use std::collections::BTreeMap;

use uuid::Uuid;

/// An authentication profile for an LLM provider.
#[derive(Debug, Clone)]
pub struct AuthProfile {
    pub provider: String,
    pub api_key: String,
    pub is_active: bool,
}

/// Manages auth profiles with rotation on 401/429.
pub struct AuthProfileManager {
    profiles: BTreeMap<String, Vec<AuthProfile>>,
    current_index: BTreeMap<String, usize>,
    session_pins: BTreeMap<Uuid, String>,
}

impl AuthProfileManager {
    pub fn new() -> Self {
        Self {
            profiles: BTreeMap::new(),
            current_index: BTreeMap::new(),
            session_pins: BTreeMap::new(),
        }
    }

    /// Add a profile for a provider.
    pub fn add_profile(&mut self, provider: String, api_key: String) {
        self.profiles
            .entry(provider.clone())
            .or_default()
            .push(AuthProfile {
                provider,
                api_key,
                is_active: true,
            });
    }

    /// Get current profile for a provider.
    pub fn current_profile(&self, provider: &str) -> Option<&AuthProfile> {
        let profiles = self.profiles.get(provider)?;
        let idx = self.current_index.get(provider).copied().unwrap_or(0);
        profiles.get(idx)
    }

    /// Rotate to next profile on 401/429.
    pub fn rotate(&mut self, provider: &str) -> Option<&AuthProfile> {
        let profiles = self.profiles.get(provider)?;
        if profiles.is_empty() {
            return None;
        }
        let idx = self.current_index.entry(provider.to_string()).or_insert(0);
        *idx = (*idx + 1) % profiles.len();
        profiles.get(*idx)
    }

    /// Pin a profile to a session.
    pub fn pin_session(&mut self, session_id: Uuid, provider: String) {
        self.session_pins.insert(session_id, provider);
    }

    /// Check if all profiles for a provider are exhausted.
    pub fn all_exhausted(&self, provider: &str) -> bool {
        self.profiles
            .get(provider)
            .map(|p| p.is_empty())
            .unwrap_or(true)
    }
}

impl Default for AuthProfileManager {
    fn default() -> Self {
        Self::new()
    }
}
