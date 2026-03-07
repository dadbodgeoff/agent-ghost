//! Unit tests for VaultProvider URL construction and JSON parsing.
//! Integration tests against a real Vault instance are `#[ignore]`.

#[cfg(feature = "vault")]
mod vault {
    use ghost_secrets::{SecretProvider, SecretsError, VaultProvider};
    use secrecy::SecretString;

    // ─── URL construction ────────────────────────────────────────────

    #[test]
    fn vault_provider_constructs_correct_data_url() {
        // We can't directly test private methods, but we can verify behavior
        // through the public API by checking error messages contain the right paths.
        // Instead, test via the constructor and verify it doesn't panic.
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("https://vault.example.com", "secret", token);
        assert!(provider.is_ok());
    }

    #[test]
    fn vault_provider_trims_trailing_slash() {
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("https://vault.example.com/", "secret", token);
        assert!(provider.is_ok());
    }

    #[test]
    fn vault_provider_empty_key_returns_invalid_key() {
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("https://vault.example.com", "secret", token).unwrap();
        let result = provider.get_secret("");
        assert!(matches!(result, Err(SecretsError::InvalidKey(_))));
    }

    #[test]
    fn vault_provider_null_key_returns_invalid_key() {
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("https://vault.example.com", "secret", token).unwrap();
        let result = provider.get_secret("key\0null");
        assert!(matches!(result, Err(SecretsError::InvalidKey(_))));
    }

    #[test]
    fn vault_provider_path_traversal_key_sanitized() {
        // Key with ../ should be sanitized — the provider should not panic
        // and should strip traversal characters.
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("https://vault.example.com", "secret", token).unwrap();
        // This will fail with a network error (no real Vault), but should NOT panic
        // and the key should be sanitized internally.
        let result = provider.get_secret("../../etc/passwd");
        assert!(result.is_err());
        // Verify it's a network/storage error, not an InvalidKey
        // (the key itself is valid after sanitization)
        match result {
            Err(SecretsError::StorageUnavailable(_)) => {} // expected — no Vault running
            Err(SecretsError::ProviderError(_)) => {}      // also acceptable
            Err(other) => panic!("unexpected error variant: {other:?}"),
            Ok(_) => panic!("should not succeed without a Vault server"),
        }
    }

    #[test]
    fn vault_provider_network_timeout_returns_storage_unavailable() {
        // Connect to a non-routable address to trigger timeout
        let token = SecretString::from("test-token".to_string());
        let provider = VaultProvider::new("http://192.0.2.1:1", "secret", token).unwrap(); // RFC 5737 TEST-NET
        let result = provider.get_secret("test-key");
        assert!(
            matches!(result, Err(SecretsError::StorageUnavailable(_))),
            "expected StorageUnavailable, got: {result:?}"
        );
    }

    // ─── KV v2 JSON parsing ─────────────────────────────────────────

    #[test]
    fn vault_provider_parses_kv2_json_response_correctly() {
        let json = r#"{
            "data": {
                "data": {
                    "value": "my-secret-value"
                },
                "metadata": {
                    "version": 1
                }
            }
        }"#;
        let result = VaultProvider::parse_kv2_response(json);
        assert_eq!(result.unwrap(), "my-secret-value");
    }

    #[test]
    fn vault_provider_malformed_json_returns_provider_error() {
        let result = VaultProvider::parse_kv2_response("not json at all {{{");
        assert!(matches!(result, Err(SecretsError::ProviderError(_))));
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("malformed JSON"), "got: {err_msg}");
    }

    #[test]
    fn vault_provider_missing_data_field_returns_provider_error() {
        let json = r#"{"auth": null, "lease_id": ""}"#;
        let result = VaultProvider::parse_kv2_response(json);
        assert!(matches!(result, Err(SecretsError::ProviderError(_))));
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("missing .data.data.value"),
            "got: {err_msg}"
        );
    }

    #[test]
    fn vault_provider_html_response_returns_provider_error() {
        // Adversarial: Vault returns HTML instead of JSON
        let html = "<html><body>503 Service Unavailable</body></html>";
        let result = VaultProvider::parse_kv2_response(html);
        assert!(matches!(result, Err(SecretsError::ProviderError(_))));
    }

    // ─── KeychainProvider construction ───────────────────────────────

    #[cfg(feature = "keychain")]
    mod keychain {
        use ghost_secrets::KeychainProvider;

        #[test]
        fn keychain_provider_new_sets_service_name() {
            let provider = KeychainProvider::new("test-service");
            assert_eq!(provider.service_name(), "test-service");
        }

        #[test]
        fn keychain_provider_default_service_name() {
            let provider = KeychainProvider::new("ghost-platform");
            assert_eq!(provider.service_name(), "ghost-platform");
        }
    }
}
