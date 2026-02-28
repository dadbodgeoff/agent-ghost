//! Property-based tests for ghost-secrets.

use ghost_secrets::{EnvProvider, SecretProvider};
use proptest::prelude::*;
use secrecy::ExposeSecret;

// ─── Proptest: EnvProvider round-trip for random key/value pairs ─────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn env_provider_round_trip_random_values(
        // Keys: alphanumeric + underscore, 1-64 chars (valid env var names)
        key in "[A-Z_][A-Z0-9_]{0,63}",
        value in "\\PC{0,1000}",
    ) {
        // Prefix to avoid collisions with real env vars
        let full_key = format!("GHOST_PROPTEST_{key}");
        std::env::set_var(&full_key, &value);

        let provider = EnvProvider;
        let secret = provider.get_secret(&full_key).unwrap();
        prop_assert_eq!(secret.expose_secret(), &value);

        std::env::remove_var(&full_key);
    }
}

// ─── Proptest: EnvProvider never panics on arbitrary keys ────────────────

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn env_provider_get_never_panics(key in "\\PC{0,200}") {
        let provider = EnvProvider;
        // Should never panic — either Ok or Err
        let _ = provider.get_secret(&key);
    }

    #[test]
    fn env_provider_has_secret_never_panics(key in "\\PC{0,200}") {
        let provider = EnvProvider;
        // Should never panic — returns bool
        let _ = provider.has_secret(&key);
    }
}
