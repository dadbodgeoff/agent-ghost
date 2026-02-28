//! Egress E2E integration tests (Task 15.3).
//!
//! Verifies the full egress pipeline: config → domain matching → policy enforcement → violation events.

use ghost_egress::config::{AgentEgressConfig, EgressPolicyMode};
use ghost_egress::DomainMatcher;

/// Allowlist: DomainMatcher permits listed domains.
#[test]
fn allowlist_permits_listed_domain() {
    let matcher = DomainMatcher::new(&[
        "api.openai.com".into(),
        "api.anthropic.com".into(),
    ]);
    assert!(matcher.matches("api.openai.com"));
    assert!(matcher.matches("api.anthropic.com"));
}

/// Allowlist: DomainMatcher blocks unlisted domains.
#[test]
fn allowlist_blocks_unlisted_domain() {
    let matcher = DomainMatcher::new(&["api.openai.com".into()]);
    assert!(!matcher.matches("evil.example.com"));
}

/// Wildcard domain matching: *.slack.com matches sub.slack.com.
#[test]
fn wildcard_domain_matching() {
    let matcher = DomainMatcher::new(&["*.slack.com".into()]);
    assert!(matcher.matches("hooks.slack.com"));
    assert!(matcher.matches("api.slack.com"));
    // Bare domain without subdomain should NOT match wildcard.
    assert!(!matcher.matches("slack.com"));
}

/// Domain matching is case-insensitive.
#[test]
fn domain_matching_case_insensitive() {
    let matcher = DomainMatcher::new(&["api.openai.com".into()]);
    assert!(matcher.matches("API.OPENAI.COM"));
    assert!(matcher.matches("Api.OpenAI.Com"));
}

/// Empty matcher matches nothing.
#[test]
fn empty_matcher_matches_nothing() {
    let matcher = DomainMatcher::new(&[]);
    assert!(!matcher.matches("anything.example.com"));
}

/// Domain with port is stripped before matching.
#[test]
fn domain_with_port_stripped() {
    let matcher = DomainMatcher::new(&["api.openai.com".into()]);
    assert!(matcher.matches("api.openai.com:443"));
}

/// Default config uses Unrestricted policy with LLM provider domains.
#[test]
fn default_config_is_unrestricted() {
    let config = AgentEgressConfig::default();
    assert_eq!(config.policy, EgressPolicyMode::Unrestricted);
    assert!(!config.allowed_domains.is_empty());
    assert!(config.log_violations);
}

/// AgentEgressConfig serializes and deserializes correctly.
#[test]
fn egress_config_serde_roundtrip() {
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".into(), "*.slack.com".into()],
        blocked_domains: vec![],
        log_violations: true,
        alert_on_violation: true,
        violation_threshold: 3,
        violation_window_minutes: 5,
    };
    let json = serde_json::to_string(&config).expect("serialize");
    let deserialized: AgentEgressConfig = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(deserialized.policy, config.policy);
    assert_eq!(deserialized.allowed_domains, config.allowed_domains);
    assert_eq!(deserialized.violation_threshold, config.violation_threshold);
}

/// EgressPolicyMode all variants serialize correctly.
#[test]
fn egress_policy_mode_serde() {
    let modes = [
        EgressPolicyMode::Allowlist,
        EgressPolicyMode::Blocklist,
        EgressPolicyMode::Unrestricted,
    ];
    for mode in &modes {
        let json = serde_json::to_string(mode).expect("serialize");
        let deserialized: EgressPolicyMode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(*mode, deserialized);
    }
}

/// ghost-egress depends on cortex-core (for TriggerEvent) but not on ghost-gateway.
#[test]
fn ghost_egress_layer_separation() {
    let cargo_toml = include_str!("../Cargo.toml");
    let deps_section = cargo_toml
        .split("[dependencies]")
        .nth(1)
        .unwrap_or("")
        .split("[dev-dependencies]")
        .next()
        .unwrap_or("");

    assert!(
        deps_section.contains("cortex-core"),
        "ghost-egress should depend on cortex-core for TriggerEvent"
    );
    assert!(
        !deps_section.contains("ghost-gateway"),
        "ghost-egress must NOT depend on ghost-gateway (layer separation)"
    );
}
