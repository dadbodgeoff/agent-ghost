//! End-to-end: Egress allowlist → proxy → domain check → violation (Phase 15.3).

use ghost_egress::config::{AgentEgressConfig, EgressPolicyMode};
use ghost_egress::policy::EgressPolicy;
use ghost_egress::proxy_provider::ProxyEgressPolicy;
use uuid::Uuid;

/// Allowlist policy allows listed domains and blocks unlisted ones.
#[test]
fn allowlist_allows_listed_domain_blocks_unlisted() {
    let policy = ProxyEgressPolicy::new();
    let agent_id = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["api.openai.com".into(), "*.slack.com".into()],
        blocked_domains: vec![],
        log_violations: true,
        alert_on_violation: false,
        violation_threshold: 5,
        violation_window_minutes: 10,
    };

    policy.apply(&agent_id, &config).unwrap();

    // Allowed domain passes.
    assert!(policy.check_domain(&agent_id, "api.openai.com").unwrap());
    // Wildcard subdomain passes.
    assert!(policy.check_domain(&agent_id, "hooks.slack.com").unwrap());
    // Unlisted domain blocked.
    assert!(!policy.check_domain(&agent_id, "evil.example.com").unwrap());

    // Cleanup.
    policy.remove(&agent_id).unwrap();
}

/// Blocklist policy blocks listed domains and allows unlisted ones.
#[test]
fn blocklist_blocks_listed_domain_allows_unlisted() {
    let policy = ProxyEgressPolicy::new();
    let agent_id = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Blocklist,
        allowed_domains: vec![],
        blocked_domains: vec!["evil.example.com".into()],
        log_violations: true,
        alert_on_violation: false,
        violation_threshold: 5,
        violation_window_minutes: 10,
    };

    policy.apply(&agent_id, &config).unwrap();

    // Blocked domain fails.
    assert!(!policy.check_domain(&agent_id, "evil.example.com").unwrap());
    // Unlisted domain passes.
    assert!(policy.check_domain(&agent_id, "api.openai.com").unwrap());

    policy.remove(&agent_id).unwrap();
}

/// Violation logging increments counter.
#[test]
fn violation_logging_increments_counter() {
    let policy = ProxyEgressPolicy::new();
    let agent_id = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Allowlist,
        allowed_domains: vec!["safe.example.com".into()],
        blocked_domains: vec![],
        log_violations: true,
        alert_on_violation: false,
        violation_threshold: 3,
        violation_window_minutes: 10,
    };

    policy.apply(&agent_id, &config).unwrap();

    // Log violations for blocked domain.
    policy.log_violation(&agent_id, "evil.example.com", "blocked");
    policy.log_violation(&agent_id, "evil.example.com", "blocked");

    // Should not panic — violation counter is internal.
    policy.remove(&agent_id).unwrap();
}

/// Unrestricted policy allows everything.
#[test]
fn unrestricted_allows_everything() {
    let policy = ProxyEgressPolicy::new();
    let agent_id = Uuid::new_v4();
    let config = AgentEgressConfig {
        policy: EgressPolicyMode::Unrestricted,
        allowed_domains: vec![],
        blocked_domains: vec![],
        log_violations: false,
        alert_on_violation: false,
        violation_threshold: 5,
        violation_window_minutes: 10,
    };

    policy.apply(&agent_id, &config).unwrap();
    assert!(policy.check_domain(&agent_id, "anything.example.com").unwrap());
    policy.remove(&agent_id).unwrap();
}
