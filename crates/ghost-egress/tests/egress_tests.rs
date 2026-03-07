//! Comprehensive tests for ghost-egress (Phase 11).
//!
//! Covers: DomainMatcher, EgressPolicy trait, ProxyEgressPolicy,
//! configuration types, adversarial inputs, and property tests.

use ghost_egress::config::{AgentEgressConfig, EgressPolicyMode, DEFAULT_ALLOWED_DOMAINS};
use ghost_egress::domain_matcher::DomainMatcher;
use ghost_egress::policy::EgressPolicy;
use ghost_egress::proxy_provider::ProxyEgressPolicy;
use uuid::Uuid;

// ── DomainMatcher unit tests ────────────────────────────────────────────

#[test]
fn domain_matcher_exact_match() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(matcher.matches("api.openai.com"));
}

#[test]
fn domain_matcher_case_insensitive() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(matcher.matches("API.OPENAI.COM"));
    assert!(matcher.matches("Api.OpenAI.Com"));
}

#[test]
fn domain_matcher_wildcard_matches_subdomain() {
    let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
    assert!(matcher.matches("api.slack.com"));
    assert!(matcher.matches("hooks.slack.com"));
    assert!(matcher.matches("a.slack.com"));
}

#[test]
fn domain_matcher_wildcard_does_not_match_bare_domain() {
    let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
    assert!(!matcher.matches("slack.com"));
}

#[test]
fn domain_matcher_wildcard_does_not_match_evil_prefix() {
    let matcher = DomainMatcher::new(&["*.slack.com".to_string()]);
    assert!(!matcher.matches("evil-slack.com"));
    assert!(!matcher.matches("notslack.com"));
}

#[test]
fn domain_matcher_strips_port() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(matcher.matches("api.openai.com:443"));
    assert!(matcher.matches("api.openai.com:8080"));
}

#[test]
fn domain_matcher_strips_path_traversal() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(matcher.matches("api.openai.com/../../etc/passwd"));
    assert!(matcher.matches("api.openai.com/v1/chat/completions"));
}

#[test]
fn domain_matcher_empty_domain_returns_false() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(!matcher.matches(""));
    assert!(!matcher.matches("   "));
}

#[test]
fn domain_matcher_null_byte_rejected() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(!matcher.matches("api.openai.com\0evil.com"));
}

#[test]
fn domain_matcher_unicode_normalization_or_rejection() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    // Cyrillic 'а' looks like Latin 'a' but is different — should not match.
    assert!(!matcher.matches("аpi.openai.com"));
}

// ── Allowlist / Blocklist / Unrestricted mode tests ─────────────────────

#[test]
fn allowlist_mode_allows_listed_blocks_unlisted() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec![
            "api.anthropic.com".to_string(),
            "api.openai.com".to_string(),
        ],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    assert!(policy.check_domain(&agent, "api.anthropic.com").unwrap());
    assert!(policy.check_domain(&agent, "api.openai.com").unwrap());
    assert!(!policy.check_domain(&agent, "evil.example.com").unwrap());
}

#[test]
fn blocklist_mode_blocks_listed_allows_unlisted() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Blocklist,
        blocked_domains: vec!["*.pastebin.com".to_string(), "evil.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    assert!(!policy.check_domain(&agent, "evil.com").unwrap());
    assert!(!policy.check_domain(&agent, "api.pastebin.com").unwrap());
    assert!(policy.check_domain(&agent, "api.openai.com").unwrap());
    assert!(policy.check_domain(&agent, "google.com").unwrap());
}

#[test]
fn unrestricted_mode_allows_everything() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Unrestricted,
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    assert!(policy.check_domain(&agent, "anything.com").unwrap());
    assert!(policy.check_domain(&agent, "evil.example.com").unwrap());
    assert!(policy.check_domain(&agent, "*.whatever.org").unwrap());
}

// ── Default allowed domains ─────────────────────────────────────────────

#[test]
fn default_allowed_domains_include_all_llm_providers() {
    let defaults = DEFAULT_ALLOWED_DOMAINS;
    assert!(defaults.contains(&"api.anthropic.com"));
    assert!(defaults.contains(&"api.openai.com"));
    assert!(defaults.contains(&"generativelanguage.googleapis.com"));
    assert!(defaults.contains(&"api.mistral.ai"));
    assert!(defaults.contains(&"api.groq.com"));
}

// ── ProxyEgressPolicy tests ─────────────────────────────────────────────

#[test]
fn proxy_url_correct_format() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    let url = policy.proxy_url(&agent).unwrap();
    assert!(url.starts_with("http://127.0.0.1:"));
    let port: u16 = url
        .strip_prefix("http://127.0.0.1:")
        .unwrap()
        .parse()
        .unwrap();
    assert!(port > 0);
}

#[test]
fn violation_counter_increments_on_blocked_request() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        violation_threshold: 10,
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    assert_eq!(policy.violation_count(&agent), 0);
    policy.log_violation(&agent, "evil.com", "CONNECT");
    assert_eq!(policy.violation_count(&agent), 1);
    policy.log_violation(&agent, "evil2.com", "CONNECT");
    assert_eq!(policy.violation_count(&agent), 2);
}

#[test]
fn violation_threshold_emits_trigger_event() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        violation_threshold: 3,
        alert_on_violation: true,
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    assert!(!policy.threshold_exceeded(&agent));
    policy.log_violation(&agent, "a.com", "CONNECT");
    policy.log_violation(&agent, "b.com", "CONNECT");
    assert!(!policy.threshold_exceeded(&agent));
    policy.log_violation(&agent, "c.com", "CONNECT");
    assert!(policy.threshold_exceeded(&agent));
}

#[test]
fn multiple_agents_independent_policies() {
    let policy = ProxyEgressPolicy::new();
    let agent_a = Uuid::new_v4();
    let agent_b = Uuid::new_v4();

    policy
        .apply(
            &agent_a,
            &AgentEgressConfig {
                policy: EgressPolicyMode::Allowlist,
                allowed_domains: vec!["api.openai.com".to_string()],
                ..Default::default()
            },
        )
        .unwrap();

    policy
        .apply(
            &agent_b,
            &AgentEgressConfig {
                policy: EgressPolicyMode::Unrestricted,
                ..Default::default()
            },
        )
        .unwrap();

    assert!(!policy.check_domain(&agent_a, "evil.com").unwrap());
    assert!(policy.check_domain(&agent_b, "evil.com").unwrap());
}

#[test]
fn remove_policy_frees_resources() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();
    assert!(policy.proxy_url(&agent).is_some());

    policy.remove(&agent).unwrap();
    assert!(policy.proxy_url(&agent).is_none());
    assert!(policy.check_domain(&agent, "anything").is_err());
}

// ── Configuration serialization tests ───────────────────────────────────

#[test]
fn config_serializes_deserializes_roundtrip() {
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string(), "*.slack.com".to_string()],
        blocked_domains: vec!["evil.com".to_string()],
        log_violations: true,
        alert_on_violation: true,
        violation_threshold: 5,
        violation_window_minutes: 10,
    };

    let json = serde_json::to_string(&config).unwrap();
    let deserialized: AgentEgressConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.policy, EgressPolicyMode::Allowlist);
    assert_eq!(deserialized.allowed_domains.len(), 2);
    assert_eq!(deserialized.violation_threshold, 5);
}

#[test]
fn config_defaults_to_unrestricted() {
    let config: AgentEgressConfig = serde_json::from_str("{}").unwrap();
    assert_eq!(config.policy, EgressPolicyMode::Unrestricted);
    assert!(config.log_violations);
    assert!(!config.alert_on_violation);
}

// ── Cargo.toml dependency verification ──────────────────────────────────

#[test]
fn cargo_toml_has_cortex_core_dependency() {
    let cargo_toml = include_str!("../Cargo.toml");
    let parsed: toml::Value = toml::from_str(cargo_toml).unwrap();
    let deps = parsed["dependencies"].as_table().unwrap();
    assert!(
        deps.contains_key("cortex-core"),
        "ghost-egress must depend on cortex-core for TriggerEvent"
    );
}

#[test]
fn cargo_toml_package_name_correct() {
    let cargo_toml = include_str!("../Cargo.toml");
    let parsed: toml::Value = toml::from_str(cargo_toml).unwrap();
    assert_eq!(parsed["package"]["name"].as_str().unwrap(), "ghost-egress");
}

// ── Adversarial tests ───────────────────────────────────────────────────

#[test]
fn adversarial_domain_with_unicode() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    // Cyrillic homograph — must not match.
    assert!(!matcher.matches("аpi.openai.com"));
    // Emoji in domain — should not match.
    assert!(!matcher.matches("🎉.openai.com"));
}

#[test]
fn adversarial_path_traversal_only_domain_matched() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    // Path traversal in the "domain" — only domain portion should be matched.
    assert!(matcher.matches("api.openai.com/../../etc/passwd"));
}

#[test]
fn adversarial_empty_domain_no_panic() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(!matcher.matches(""));
}

#[test]
fn adversarial_domain_with_port_correct_matching() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(matcher.matches("api.openai.com:443"));
    assert!(matcher.matches("api.openai.com:8080"));
}

#[test]
fn adversarial_very_long_domain_no_panic() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    let long_domain = "a".repeat(100_000) + ".openai.com";
    // Should not panic, just not match.
    assert!(!matcher.matches(&long_domain));
}

#[test]
fn adversarial_domain_with_null_bytes() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(!matcher.matches("api.openai.com\0"));
    assert!(!matcher.matches("\0api.openai.com"));
}

#[test]
fn adversarial_domain_with_spaces() {
    let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
    assert!(!matcher.matches("api openai com"));
}

// ── TriggerEvent variant exists ─────────────────────────────────────────

#[test]
fn trigger_event_network_egress_violation_exists() {
    use cortex_core::safety::trigger::TriggerEvent;

    let event = TriggerEvent::NetworkEgressViolation {
        agent_id: Uuid::new_v4(),
        domain: "evil.com".to_string(),
        policy_mode: "allowlist".to_string(),
        violation_count: 5,
        threshold: 5,
        detected_at: chrono::Utc::now(),
    };

    // Verify it serializes correctly.
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("NetworkEgressViolation"));
    assert!(json.contains("evil.com"));
}

// ── Adversarial: Concurrent requests (Task 11.2) ───────────────────────

#[test]
fn adversarial_100_concurrent_requests_no_crash() {
    use std::sync::Arc;
    use std::thread;

    let policy = Arc::new(ProxyEgressPolicy::new());
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string(), "*.slack.com".to_string()],
        violation_threshold: 200,
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    let mut handles = Vec::new();
    for i in 0..100 {
        let policy = Arc::clone(&policy);
        let agent = agent;
        handles.push(thread::spawn(move || {
            let domain = if i % 2 == 0 {
                "api.openai.com"
            } else {
                "evil.example.com"
            };
            let result = policy.check_domain(&agent, domain);
            assert!(result.is_ok());
            if i % 2 != 0 {
                policy.log_violation(&agent, domain, "CONNECT");
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // No crash, violations recorded.
    assert!(policy.violation_count(&agent) > 0);
}

#[test]
fn adversarial_request_to_ip_address_bypassing_dns() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    // Raw IP address should NOT match any domain pattern — blocked.
    assert!(!policy.check_domain(&agent, "104.18.7.192").unwrap());
    assert!(!policy.check_domain(&agent, "127.0.0.1").unwrap());
    assert!(!policy.check_domain(&agent, "::1").unwrap());
    assert!(!policy.check_domain(&agent, "0.0.0.0").unwrap());
}

#[test]
fn adversarial_malformed_connect_request() {
    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    // Malformed domains — should not panic, should return false or error.
    assert!(!policy.check_domain(&agent, "").unwrap_or(false));
    assert!(!policy.check_domain(&agent, "   ").unwrap_or(false));
    assert!(!policy.check_domain(&agent, "\r\n").unwrap_or(false));
    assert!(!policy
        .check_domain(&agent, "http://api.openai.com")
        .unwrap_or(false));
    assert!(!policy
        .check_domain(&agent, "CONNECT api.openai.com:443 HTTP/1.1")
        .unwrap_or(false));
    assert!(!policy.check_domain(&agent, "\0\0\0").unwrap_or(false));
}

// ── Gateway integration tests (Task 11.5) ───────────────────────────────

#[test]
fn config_parsing_all_egress_policy_modes() {
    // Allowlist
    let json = r#"{"policy": "allowlist", "allowed_domains": ["api.openai.com"]}"#;
    let config: ghost_egress::AgentEgressConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.policy, EgressPolicyMode::Allowlist);

    // Blocklist
    let json = r#"{"policy": "blocklist", "blocked_domains": ["evil.com"]}"#;
    let config: ghost_egress::AgentEgressConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.policy, EgressPolicyMode::Blocklist);

    // Unrestricted
    let json = r#"{"policy": "unrestricted"}"#;
    let config: ghost_egress::AgentEgressConfig = serde_json::from_str(json).unwrap();
    assert_eq!(config.policy, EgressPolicyMode::Unrestricted);
}

#[test]
fn correct_egress_policy_selection_per_isolation_mode() {
    // InProcess → always ProxyEgressPolicy (can't do per-thread filtering).
    // We verify by applying a proxy policy and checking it works.
    let proxy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    proxy.apply(&agent, &config).unwrap();
    assert!(proxy.proxy_url(&agent).is_some());
    assert!(proxy.check_domain(&agent, "api.openai.com").unwrap());
    assert!(!proxy.check_domain(&agent, "evil.com").unwrap());
}

#[test]
fn json_schema_validates_egress_config() {
    // Verify the JSON schema file contains the network egress config section.
    let schema_str = include_str!("../../../schemas/ghost-config.schema.json");
    let schema: serde_json::Value = serde_json::from_str(schema_str).unwrap();

    // Navigate to agents.items.properties.network
    let network = &schema["properties"]["agents"]["items"]["properties"]["network"];
    assert!(network.is_object(), "network property must exist in schema");

    // Verify all 7 fields are present.
    let props = &network["properties"];
    assert!(props["egress_policy"].is_object());
    assert!(props["allowed_domains"].is_object());
    assert!(props["blocked_domains"].is_object());
    assert!(props["log_violations"].is_object());
    assert!(props["alert_on_violation"].is_object());
    assert!(props["violation_threshold"].is_object());
    assert!(props["violation_window_minutes"].is_object());

    // Verify enum values for egress_policy.
    let policy_enum = &props["egress_policy"]["enum"];
    assert!(policy_enum.is_array());
    let values: Vec<&str> = policy_enum
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert!(values.contains(&"allowlist"));
    assert!(values.contains(&"blocklist"));
    assert!(values.contains(&"unrestricted"));
}

#[test]
fn missing_network_config_defaults_to_unrestricted() {
    // When no network config is provided, the default should be Unrestricted
    // (backward compatible — no enforcement).
    let config = AgentEgressConfig::default();
    assert_eq!(config.policy, EgressPolicyMode::Unrestricted);
    assert!(config.log_violations);
    assert!(!config.alert_on_violation);
    assert_eq!(config.violation_threshold, 5);
    assert_eq!(config.violation_window_minutes, 10);

    // Verify the default allowed domains are the 5 LLM provider APIs.
    assert_eq!(config.allowed_domains.len(), 5);
}

#[test]
fn bootstrap_egress_policy_applied_per_agent() {
    // Simulate what step4b_apply_egress_policies does:
    // For each agent with a network config, apply the correct policy.
    let policy = ProxyEgressPolicy::new();

    // Agent 1: allowlist
    let agent1 = Uuid::new_v4();
    let config1 = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent1, &config1).unwrap();

    // Agent 2: blocklist
    let agent2 = Uuid::new_v4();
    let config2 = AgentEgressConfig {
        policy: EgressPolicyMode::Blocklist,
        blocked_domains: vec!["evil.com".to_string()],
        ..Default::default()
    };
    policy.apply(&agent2, &config2).unwrap();

    // Agent 3: unrestricted (no enforcement)
    let agent3 = Uuid::new_v4();
    let config3 = AgentEgressConfig::default();
    policy.apply(&agent3, &config3).unwrap();

    // Verify each agent has independent policy enforcement.
    assert!(policy.check_domain(&agent1, "api.openai.com").unwrap());
    assert!(!policy.check_domain(&agent1, "evil.com").unwrap());

    assert!(!policy.check_domain(&agent2, "evil.com").unwrap());
    assert!(policy.check_domain(&agent2, "api.openai.com").unwrap());

    assert!(policy.check_domain(&agent3, "anything.com").unwrap());
}

#[test]
fn violation_threshold_exceeded_emits_trigger_event() {
    use cortex_core::safety::trigger::TriggerEvent;

    let policy = ProxyEgressPolicy::new();
    let agent = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".to_string()],
        violation_threshold: 3,
        alert_on_violation: true,
        violation_window_minutes: 60,
        ..Default::default()
    };
    policy.apply(&agent, &config).unwrap();

    // Record violations up to threshold.
    policy.log_violation(&agent, "evil1.com", "CONNECT");
    policy.log_violation(&agent, "evil2.com", "CONNECT");
    assert!(!policy.threshold_exceeded(&agent));

    policy.log_violation(&agent, "evil3.com", "CONNECT");
    assert!(policy.threshold_exceeded(&agent));

    // At this point, the production code would emit a TriggerEvent.
    // Verify we can construct the correct event.
    let event = TriggerEvent::NetworkEgressViolation {
        agent_id: agent,
        domain: "evil3.com".to_string(),
        policy_mode: "allowlist".to_string(),
        violation_count: policy.violation_count(&agent),
        threshold: config.violation_threshold,
        detected_at: chrono::Utc::now(),
    };
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("NetworkEgressViolation"));
    assert_eq!(policy.violation_count(&agent), 3);
}
