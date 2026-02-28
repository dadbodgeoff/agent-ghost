//! Secrets E2E integration tests (Task 15.3).
//!
//! Verifies the full secrets pipeline: provider → credential retrieval → round-trip.

use ghost_secrets::{EnvProvider, SecretProvider, ExposeSecret};

/// EnvProvider retrieves a secret from the environment.
#[test]
fn env_provider_retrieves_secret() {
    std::env::set_var("GHOST_TEST_SECRET_E2E", "test-api-key-12345");
    let provider = EnvProvider;
    let result = provider.get_secret("GHOST_TEST_SECRET_E2E");
    assert!(result.is_ok(), "EnvProvider should retrieve env var");
    assert_eq!(result.unwrap().expose_secret(), "test-api-key-12345");
    std::env::remove_var("GHOST_TEST_SECRET_E2E");
}

/// EnvProvider returns error for missing key.
#[test]
fn env_provider_missing_key_returns_error() {
    std::env::remove_var("GHOST_NONEXISTENT_KEY_E2E");
    let provider = EnvProvider;
    let result = provider.get_secret("GHOST_NONEXISTENT_KEY_E2E");
    assert!(result.is_err(), "EnvProvider should error on missing key");
}

/// EnvProvider has_secret returns true for existing key.
#[test]
fn env_provider_has_secret() {
    std::env::set_var("GHOST_HAS_E2E", "exists");
    let provider = EnvProvider;
    assert!(provider.has_secret("GHOST_HAS_E2E"));
    assert!(!provider.has_secret("GHOST_DOES_NOT_EXIST_E2E"));
    std::env::remove_var("GHOST_HAS_E2E");
}

/// EnvProvider set_secret returns StorageUnavailable (env vars are read-only).
#[test]
fn env_provider_set_is_read_only() {
    let provider = EnvProvider;
    let result = provider.set_secret("GHOST_READONLY_E2E", "value");
    assert!(result.is_err(), "EnvProvider set_secret should fail (read-only)");
}

/// EnvProvider delete_secret returns StorageUnavailable (env vars are read-only).
#[test]
fn env_provider_delete_is_read_only() {
    let provider = EnvProvider;
    let result = provider.delete_secret("GHOST_READONLY_E2E");
    assert!(result.is_err(), "EnvProvider delete_secret should fail (read-only)");
}

/// EnvProvider rejects empty key.
#[test]
fn env_provider_rejects_empty_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("");
    assert!(result.is_err(), "Empty key should be rejected");
}

/// EnvProvider rejects key with null byte.
#[test]
fn env_provider_rejects_null_byte_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("KEY\0EVIL");
    assert!(result.is_err(), "Key with null byte should be rejected");
}

/// EnvProvider rejects key with equals sign.
#[test]
fn env_provider_rejects_equals_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("KEY=VALUE");
    assert!(result.is_err(), "Key with '=' should be rejected");
}

/// KeychainProvider integration test — requires OS keychain, so ignored by default.
#[test]
#[ignore = "requires OS keychain — run manually with `cargo test -- --ignored`"]
#[cfg(feature = "keychain")]
fn keychain_provider_roundtrip() {
    use ghost_secrets::KeychainProvider;

    let provider = KeychainProvider::new("ghost-test-e2e");

    // Set
    provider.set_secret("test-key", "keychain-test-value").expect("keychain set failed");

    // Get
    let retrieved = provider.get_secret("test-key").expect("keychain get failed");
    assert_eq!(retrieved.expose_secret(), "keychain-test-value");

    // Delete (cleanup)
    provider.delete_secret("test-key").expect("keychain delete failed");
}

/// ghost-secrets crate is a leaf crate with zero ghost-*/cortex-* dependencies.
#[test]
fn ghost_secrets_is_leaf_crate() {
    let cargo_toml = include_str!("../Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        !deps_section.contains("ghost-"),
        "ghost-secrets must not depend on any ghost-* crate (leaf crate rule)"
    );
    assert!(
        !deps_section.contains("cortex-"),
        "ghost-secrets must not depend on any cortex-* crate (leaf crate rule)"
    );
}

/// ProviderConfig default is Env.
#[test]
fn provider_config_default_is_env() {
    let config = ghost_secrets::ProviderConfig::default();
    assert_eq!(config, ghost_secrets::ProviderConfig::Env);
}

/// ProviderConfig serde round-trip for all variants.
#[test]
fn provider_config_serde_roundtrip() {
    let configs = vec![
        ghost_secrets::ProviderConfig::Env,
        ghost_secrets::ProviderConfig::Keychain {
            service_name: "ghost-test".into(),
        },
        ghost_secrets::ProviderConfig::Vault {
            endpoint: "https://vault.local:8200".into(),
            mount: "secret".into(),
            token_env: "VAULT_TOKEN".into(),
        },
    ];
    for config in &configs {
        let json = serde_json::to_string(config).expect("serialize");
        let deserialized: ghost_secrets::ProviderConfig =
            serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*config, deserialized);
    }
}
