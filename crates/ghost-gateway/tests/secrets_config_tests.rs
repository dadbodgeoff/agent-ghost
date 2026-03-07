//! Tests for secrets configuration parsing (Task 10.5).
//!
//! Covers: config parsing for all provider types, defaults,
//! validation errors, and JSON schema compliance.

use ghost_gateway::config::{build_secret_provider, GhostConfig, SecretsConfig};

// ─── Config parsing: env provider ────────────────────────────────────────

#[test]
fn config_parses_secrets_provider_env() {
    let yaml = r#"
secrets:
  provider: env
"#;
    let config: GhostConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.secrets.provider, "env");
    let provider = build_secret_provider(&config.secrets);
    assert!(provider.is_ok());
}

// ─── Config parsing: keychain provider ───────────────────────────────────

#[cfg(feature = "keychain")]
#[test]
fn config_parses_secrets_provider_keychain() {
    let yaml = r#"
secrets:
  provider: keychain
  keychain:
    service_name: "my-custom-service"
"#;
    let config: GhostConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.secrets.provider, "keychain");
    assert_eq!(
        config.secrets.keychain.as_ref().unwrap().service_name,
        "my-custom-service"
    );
    let provider = build_secret_provider(&config.secrets);
    assert!(provider.is_ok());
}

// ─── Config parsing: vault provider ──────────────────────────────────────

#[test]
fn config_parses_secrets_provider_vault_structure() {
    let yaml = r#"
secrets:
  provider: vault
  vault:
    endpoint: "https://vault.example.com"
    mount: "secret"
    token_env: "VAULT_TOKEN"
"#;
    let config: GhostConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.secrets.provider, "vault");
    let vault = config.secrets.vault.as_ref().unwrap();
    assert_eq!(vault.endpoint, "https://vault.example.com");
    assert_eq!(vault.mount, "secret");
    assert_eq!(vault.token_env, "VAULT_TOKEN");
}

// ─── Missing secrets section → defaults to env ───────────────────────────

#[test]
fn config_missing_secrets_section_defaults_to_env() {
    let yaml = r#"
gateway:
  port: 18789
"#;
    let config: GhostConfig = serde_yaml::from_str(yaml).unwrap();
    assert_eq!(config.secrets.provider, "env");
    let provider = build_secret_provider(&config.secrets);
    assert!(provider.is_ok());
}

// ─── Invalid provider value → validation error ──────────────────────────

#[test]
fn config_invalid_provider_returns_validation_error() {
    let config = SecretsConfig {
        provider: "invalid_backend".into(),
        keychain: None,
        vault: None,
    };
    let result = build_secret_provider(&config);
    assert!(result.is_err());
    let err = result.err().expect("should be an error");
    assert!(
        err.to_string().contains("unknown secrets provider"),
        "expected 'unknown secrets provider' error, got: {err}"
    );
}

// ─── Vault without vault section → validation error ─────────────────────

#[test]
fn config_vault_without_vault_section_returns_error() {
    let config = SecretsConfig {
        provider: "vault".into(),
        keychain: None,
        vault: None,
    };
    // This will fail because vault section is missing (or VAULT_TOKEN env var)
    let result = build_secret_provider(&config);
    assert!(result.is_err());
}

// ─── Default SecretsConfig ───────────────────────────────────────────────

#[test]
fn secrets_config_default_is_env() {
    let config = SecretsConfig::default();
    assert_eq!(config.provider, "env");
    assert!(config.keychain.is_none());
    assert!(config.vault.is_none());
}

// ─── JSON schema validation ─────────────────────────────────────────────

#[test]
fn json_schema_contains_secrets_section() {
    let schema_str = include_str!("../../../schemas/ghost-config.schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
    let secrets = schema.get("properties").and_then(|p| p.get("secrets"));
    assert!(secrets.is_some(), "schema must contain 'secrets' property");

    let secrets = secrets.unwrap();
    let provider = secrets.get("properties").and_then(|p| p.get("provider"));
    assert!(provider.is_some(), "secrets must have 'provider' property");

    // Verify enum values
    let provider_enum = provider.unwrap().get("enum").and_then(|e| e.as_array());
    assert!(provider_enum.is_some());
    let values: Vec<&str> = provider_enum
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(values.contains(&"env"));
    assert!(values.contains(&"keychain"));
    assert!(values.contains(&"vault"));
}

#[test]
fn json_schema_secrets_vault_requires_endpoint() {
    let schema_str = include_str!("../../../schemas/ghost-config.schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();
    let vault = schema
        .get("properties")
        .and_then(|p| p.get("secrets"))
        .and_then(|s| s.get("properties"))
        .and_then(|p| p.get("vault"));
    assert!(vault.is_some());

    let required = vault.unwrap().get("required").and_then(|r| r.as_array());
    assert!(required.is_some());
    let required_fields: Vec<&str> = required
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str())
        .collect();
    assert!(required_fields.contains(&"endpoint"));
}
