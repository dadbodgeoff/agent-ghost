//! End-to-end: EnvProvider → SecretProvider → credential retrieval (Phase 15.3).

use ghost_secrets::{EnvProvider, SecretProvider};

/// EnvProvider returns NotFound for missing keys.
#[test]
fn env_provider_missing_key_returns_not_found() {
    let provider = EnvProvider;
    let result = provider.get_secret("GHOST_TEST_NONEXISTENT_KEY_E2E");
    assert!(result.is_err());
}

/// EnvProvider reads a real env var when set.
#[test]
fn env_provider_reads_set_variable() {
    // PATH is always set on all platforms.
    let provider = EnvProvider;
    assert!(provider.has_secret("PATH"));
    let secret = provider.get_secret("PATH");
    assert!(secret.is_ok());
}

/// EnvProvider set_secret is read-only — returns error.
#[test]
fn env_provider_set_secret_is_read_only() {
    let provider = EnvProvider;
    let result = provider.set_secret("GHOST_TEST_KEY", "value");
    assert!(result.is_err());
}

/// EnvProvider delete_secret is read-only — returns error.
#[test]
fn env_provider_delete_secret_is_read_only() {
    let provider = EnvProvider;
    let result = provider.delete_secret("GHOST_TEST_KEY");
    assert!(result.is_err());
}
