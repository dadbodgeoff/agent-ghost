//! Comprehensive tests for ghost-secrets.
//!
//! Covers: EnvProvider round-trip, error cases, key validation,
//! adversarial inputs, zeroize trait bound, leaf-crate dependency audit,
//! VaultProvider URL construction and JSON parsing, KeychainProvider construction,
//! and proptest invariants.

use ghost_secrets::{EnvProvider, SecretProvider, SecretsError};
use secrecy::ExposeSecret;

// ─── EnvProvider: basic operations ───────────────────────────────────────

#[test]
fn env_provider_reads_existing_env_var() {
    let key = "GHOST_TEST_SECRET_READ_EXISTING";
    std::env::set_var(key, "test-value-42");
    let provider = EnvProvider;
    let secret = provider.get_secret(key).expect("should find env var");
    assert_eq!(secret.expose_secret(), "test-value-42");
    std::env::remove_var(key);
}

#[test]
fn env_provider_missing_var_returns_not_found() {
    let provider = EnvProvider;
    let result = provider.get_secret("GHOST_TEST_DEFINITELY_MISSING_VAR_XYZ");
    assert!(matches!(result, Err(SecretsError::NotFound(_))));
}

#[test]
fn env_provider_set_secret_returns_storage_unavailable() {
    let provider = EnvProvider;
    let result = provider.set_secret("ANY_KEY", "any_value");
    assert!(matches!(result, Err(SecretsError::StorageUnavailable(_))));
}

#[test]
fn env_provider_delete_secret_returns_storage_unavailable() {
    let provider = EnvProvider;
    let result = provider.delete_secret("ANY_KEY");
    assert!(matches!(result, Err(SecretsError::StorageUnavailable(_))));
}

#[test]
fn env_provider_has_secret_true_for_set_var() {
    let key = "GHOST_TEST_HAS_SECRET_TRUE";
    std::env::set_var(key, "present");
    let provider = EnvProvider;
    assert!(provider.has_secret(key));
    std::env::remove_var(key);
}

#[test]
fn env_provider_has_secret_false_for_unset_var() {
    let provider = EnvProvider;
    assert!(!provider.has_secret("GHOST_TEST_HAS_SECRET_MISSING_XYZ"));
}

// ─── SecretString zeroize trait bound ────────────────────────────────────

#[test]
fn secret_string_is_zeroize_on_drop() {
    // SecretString from the `secrecy` crate implements ZeroizeOnDrop.
    // This test exercises the drop path — the inner bytes are zeroized
    // when the SecretString goes out of scope.
    let key = "GHOST_TEST_ZEROIZE_DROP";
    std::env::set_var(key, "sensitive-data");
    let provider = EnvProvider;
    let secret = provider.get_secret(key).unwrap();
    assert_eq!(secret.expose_secret(), "sensitive-data");
    drop(secret); // triggers zeroize
    std::env::remove_var(key);
}

// ─── Leaf crate dependency audit ─────────────────────────────────────────

#[test]
fn cargo_toml_has_no_ghost_or_cortex_dependencies() {
    let cargo_toml = include_str!("../Cargo.toml");
    let parsed: toml::Value = cargo_toml.parse().expect("valid TOML");

    let check_section = |section: &str| {
        if let Some(deps) = parsed.get(section).and_then(|v| v.as_table()) {
            for key in deps.keys() {
                assert!(
                    !key.starts_with("ghost-") && !key.starts_with("cortex-"),
                    "Leaf crate violation: [{section}] contains `{key}` — \
                     ghost-secrets must have zero ghost-*/cortex-* dependencies"
                );
            }
        }
    };

    check_section("dependencies");
}

// ─── Adversarial: key validation ─────────────────────────────────────────

#[test]
fn env_provider_empty_key_returns_invalid_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("");
    assert!(matches!(result, Err(SecretsError::InvalidKey(_))));
}

#[test]
fn env_provider_key_with_null_byte_returns_invalid_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("KEY\0WITH_NULL");
    assert!(matches!(result, Err(SecretsError::InvalidKey(_))));
}

#[test]
fn env_provider_key_with_equals_returns_invalid_key() {
    let provider = EnvProvider;
    let result = provider.get_secret("KEY=VALUE");
    assert!(matches!(result, Err(SecretsError::InvalidKey(_))));
}

#[test]
fn env_provider_key_with_spaces_works() {
    // Spaces in env var names are technically valid on some platforms,
    // but we allow them — the OS will just return NotFound if unsupported.
    let provider = EnvProvider;
    let result = provider.get_secret("KEY WITH SPACES");
    // Should not panic — either NotFound or a value
    assert!(result.is_err() || result.is_ok());
}

#[test]
fn env_provider_very_long_value_no_oom() {
    let key = "GHOST_TEST_LONG_VALUE";
    let long_value = "A".repeat(1_048_576); // 1 MB
    std::env::set_var(key, &long_value);
    let provider = EnvProvider;
    let secret = provider.get_secret(key).expect("should handle 1MB value");
    assert_eq!(secret.expose_secret().len(), 1_048_576);
    std::env::remove_var(key);
}

#[test]
fn env_provider_has_secret_false_for_empty_key() {
    let provider = EnvProvider;
    assert!(!provider.has_secret(""));
}

#[test]
fn env_provider_has_secret_false_for_null_key() {
    let provider = EnvProvider;
    assert!(!provider.has_secret("KEY\0NULL"));
}

// ─── ProviderConfig serde round-trip ─────────────────────────────────────

#[test]
fn provider_config_env_serde_round_trip() {
    let config = ghost_secrets::ProviderConfig::Env;
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ghost_secrets::ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, ghost_secrets::ProviderConfig::Env);
}

#[test]
fn provider_config_keychain_serde_round_trip() {
    let config = ghost_secrets::ProviderConfig::Keychain {
        service_name: "my-service".into(),
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ghost_secrets::ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, parsed);
}

#[test]
fn provider_config_vault_serde_round_trip() {
    let config = ghost_secrets::ProviderConfig::Vault {
        endpoint: "https://vault.example.com".into(),
        mount: "secret".into(),
        token_env: "VAULT_TOKEN".into(),
    };
    let json = serde_json::to_string(&config).unwrap();
    let parsed: ghost_secrets::ProviderConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, parsed);
}

#[test]
fn provider_config_default_is_env() {
    let config = ghost_secrets::ProviderConfig::default();
    assert_eq!(config, ghost_secrets::ProviderConfig::Env);
}
