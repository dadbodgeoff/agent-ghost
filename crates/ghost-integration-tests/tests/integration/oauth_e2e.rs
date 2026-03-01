//! End-to-end: OAuth token storage → encrypt → decrypt → delete (Phase 15.3).
//!
//! Note: Full OAuth flow (connect → callback → token) requires a running
//! HTTP server and real provider credentials. These tests exercise the
//! storage layer which is the critical path for token security.

use ghost_secrets::SecretProvider;
use ghost_oauth::types::{OAuthRefId, TokenSet};
use ghost_oauth::storage::TokenStore;
use secrecy::SecretString;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// In-memory secret provider for testing (supports set/get).
struct MemoryProvider {
    secrets: Mutex<BTreeMap<String, String>>,
}

impl MemoryProvider {
    fn new() -> Self {
        Self {
            secrets: Mutex::new(BTreeMap::new()),
        }
    }
}

impl SecretProvider for MemoryProvider {
    fn get_secret(&self, key: &str) -> Result<SecretString, ghost_secrets::SecretsError> {
        self.secrets
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .map(SecretString::from)
            .ok_or_else(|| ghost_secrets::SecretsError::NotFound(key.to_string()))
    }

    fn set_secret(&self, key: &str, value: &str) -> Result<(), ghost_secrets::SecretsError> {
        self.secrets
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn delete_secret(&self, key: &str) -> Result<(), ghost_secrets::SecretsError> {
        self.secrets.lock().unwrap().remove(key);
        Ok(())
    }

    fn has_secret(&self, key: &str) -> bool {
        self.secrets.lock().unwrap().contains_key(key)
    }
}

fn temp_store() -> (TokenStore, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let provider: Box<dyn SecretProvider> = Box::new(MemoryProvider::new());
    let store = TokenStore::new(dir.path().to_path_buf(), provider);
    (store, dir)
}

fn sample_token_set() -> TokenSet {
    TokenSet {
        access_token: SecretString::from("test-access-token-12345".to_string()),
        refresh_token: Some(SecretString::from("test-refresh-token-67890".to_string())),
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
        scopes: vec!["read".into(), "write".into()],
    }
}

/// Store and load a token — round-trip through encryption.
#[test]
fn store_and_load_token_round_trip() {
    let (store, _dir) = temp_store();
    let ref_id = OAuthRefId::new();
    let token_set = sample_token_set();

    store.store_token(&ref_id, "google", &token_set).unwrap();
    let loaded = store.load_token(&ref_id, "google").unwrap();

    // Access token should survive round-trip.
    use ghost_secrets::ExposeSecret;
    assert_eq!(
        loaded.access_token.expose_secret(),
        "test-access-token-12345"
    );
    assert_eq!(loaded.scopes, vec!["read", "write"]);
}

/// Load non-existent token returns NotConnected error.
#[test]
fn load_nonexistent_token_returns_not_connected() {
    let (store, _dir) = temp_store();
    let ref_id = OAuthRefId::new();
    let result = store.load_token(&ref_id, "github");
    assert!(result.is_err());
}

/// Delete token removes the file.
#[test]
fn delete_token_removes_file() {
    let (store, _dir) = temp_store();
    let ref_id = OAuthRefId::new();
    let token_set = sample_token_set();

    store.store_token(&ref_id, "slack", &token_set).unwrap();
    store.delete_token(&ref_id, "slack").unwrap();

    // Should no longer be loadable.
    let result = store.load_token_raw(&ref_id, "slack");
    assert!(result.is_err());
}

/// List connections returns stored ref IDs.
#[test]
fn list_connections_returns_stored_refs() {
    let (store, _dir) = temp_store();
    let ref1 = OAuthRefId::new();
    let ref2 = OAuthRefId::new();
    let token_set = sample_token_set();

    store.store_token(&ref1, "google", &token_set).unwrap();
    store.store_token(&ref2, "google", &token_set).unwrap();

    let connections = store.list_connections("google").unwrap();
    assert_eq!(connections.len(), 2);
}

/// List all connections across providers.
#[test]
fn list_all_connections_across_providers() {
    let (store, _dir) = temp_store();
    let token_set = sample_token_set();

    store.store_token(&OAuthRefId::new(), "google", &token_set).unwrap();
    store.store_token(&OAuthRefId::new(), "github", &token_set).unwrap();

    let all = store.list_all_connections().unwrap();
    assert_eq!(all.len(), 2);
    let providers: Vec<&str> = all.iter().map(|(p, _)| p.as_str()).collect();
    assert!(providers.contains(&"google"));
    assert!(providers.contains(&"github"));
}
