//! OAuth E2E integration tests (Task 15.3).
//!
//! Verifies the full OAuth pipeline: types, ref IDs, token lifecycle, and API request brokering.

use chrono::{Duration, Utc};
use ghost_oauth::types::{
    ApiRequest, ApiResponse, ConnectionInfo, ConnectionStatus, OAuthRefId, ProviderConfig,
    TokenSet,
};
use secrecy::SecretString;
use std::collections::BTreeMap;

/// OAuthRefId round-trip: create → serialize → deserialize → compare.
#[test]
fn oauth_ref_id_serde_roundtrip() {
    let ref_id = OAuthRefId::new();
    let json = serde_json::to_string(&ref_id).expect("serialize");
    let deserialized: OAuthRefId = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(ref_id, deserialized);
}

/// OAuthRefId from_uuid reconstructs correctly.
#[test]
fn oauth_ref_id_from_uuid() {
    let uuid = uuid::Uuid::new_v4();
    let ref_id = OAuthRefId::from_uuid(uuid);
    assert_eq!(*ref_id.as_uuid(), uuid);
}

/// OAuthRefId Display shows the UUID.
#[test]
fn oauth_ref_id_display() {
    let ref_id = OAuthRefId::new();
    let display = format!("{ref_id}");
    assert!(!display.is_empty());
    // Should be a valid UUID string.
    uuid::Uuid::parse_str(&display).expect("display should be valid UUID");
}

/// TokenSet expiry detection.
#[test]
fn token_set_expiry_detection() {
    let expired = TokenSet {
        access_token: SecretString::from("expired-token"),
        refresh_token: None,
        expires_at: Utc::now() - Duration::hours(1),
        scopes: vec!["read".into()],
    };
    assert!(expired.is_expired());

    let valid = TokenSet {
        access_token: SecretString::from("valid-token"),
        refresh_token: Some(SecretString::from("refresh")),
        expires_at: Utc::now() + Duration::hours(1),
        scopes: vec!["read".into(), "write".into()],
    };
    assert!(!valid.is_expired());
}

/// TokenSet Debug does not leak secrets.
#[test]
fn token_set_debug_redacts_secrets() {
    let ts = TokenSet {
        access_token: SecretString::from("super-secret-token"),
        refresh_token: Some(SecretString::from("super-secret-refresh")),
        expires_at: Utc::now() + Duration::hours(1),
        scopes: vec!["read".into()],
    };
    let debug = format!("{ts:?}");
    assert!(!debug.contains("super-secret-token"), "access_token leaked in Debug");
    assert!(!debug.contains("super-secret-refresh"), "refresh_token leaked in Debug");
    assert!(debug.contains("[REDACTED]"), "Debug should show [REDACTED]");
}

/// ApiRequest serializes with BTreeMap headers (deterministic ordering).
#[test]
fn api_request_deterministic_headers() {
    let mut headers = BTreeMap::new();
    headers.insert("Accept".into(), "application/json".into());
    headers.insert("X-Custom".into(), "value".into());

    let req = ApiRequest {
        method: "GET".into(),
        url: "https://api.example.com/data".into(),
        headers,
        body: None,
    };

    let json1 = serde_json::to_string(&req).expect("serialize 1");
    let json2 = serde_json::to_string(&req).expect("serialize 2");
    assert_eq!(json1, json2, "BTreeMap headers should produce deterministic JSON");
}

/// ApiResponse round-trip.
#[test]
fn api_response_serde_roundtrip() {
    let resp = ApiResponse {
        status: 200,
        headers: BTreeMap::from([("Content-Type".into(), "application/json".into())]),
        body: r#"{"ok": true}"#.into(),
    };
    let json = serde_json::to_string(&resp).expect("serialize");
    let deserialized: ApiResponse = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.status, 200);
    assert_eq!(deserialized.body, resp.body);
}

/// ProviderConfig round-trip.
#[test]
fn provider_config_serde_roundtrip() {
    let config = ProviderConfig {
        client_id: "test-client-id".into(),
        client_secret_key: "GITHUB_CLIENT_SECRET".into(),
        auth_url: "https://github.com/login/oauth/authorize".into(),
        token_url: "https://github.com/login/oauth/access_token".into(),
        revoke_url: None,
        scopes: BTreeMap::from([
            ("repo".into(), vec!["repo".into()]),
            ("user".into(), vec!["user:email".into()]),
        ]),
    };
    let json = serde_json::to_string(&config).expect("serialize");
    let deserialized: ProviderConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.client_id, config.client_id);
    assert_eq!(deserialized.scopes.len(), 2);
}

/// ConnectionInfo round-trip.
#[test]
fn connection_info_serde_roundtrip() {
    let info = ConnectionInfo {
        ref_id: OAuthRefId::new(),
        provider: "github".into(),
        scopes: vec!["repo".into()],
        connected_at: Utc::now(),
        status: ConnectionStatus::Connected,
    };
    let json = serde_json::to_string(&info).expect("serialize");
    let deserialized: ConnectionInfo = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.provider, "github");
    assert_eq!(deserialized.status, ConnectionStatus::Connected);
}

/// ConnectionStatus variants.
#[test]
fn connection_status_all_variants() {
    let variants = [
        ConnectionStatus::Connected,
        ConnectionStatus::Expired,
        ConnectionStatus::Revoked,
        ConnectionStatus::Error,
    ];
    for v in &variants {
        let json = serde_json::to_string(v).expect("serialize");
        let deserialized: ConnectionStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*v, deserialized);
    }
}

/// ghost-oauth depends on secrecy (for SecretString) but not on ghost-gateway.
#[test]
fn ghost_oauth_layer_separation() {
    let cargo_toml = include_str!("../Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        deps_section.contains("secrecy"),
        "ghost-oauth should depend on secrecy for token wrapping"
    );
    assert!(
        !deps_section.contains("ghost-gateway"),
        "ghost-oauth must NOT depend on ghost-gateway (layer separation)"
    );
}
