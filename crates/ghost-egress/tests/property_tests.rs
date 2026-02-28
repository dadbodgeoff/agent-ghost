//! Property tests for ghost-egress (Phase 11).
//!
//! Proptest: For 500 random domain strings, DomainMatcher never panics.

use ghost_egress::domain_matcher::DomainMatcher;
use ghost_egress::config::{AgentEgressConfig, EgressPolicyMode};
use ghost_egress::policy::EgressPolicy;
use ghost_egress::proxy_provider::ProxyEgressPolicy;
use proptest::prelude::*;
use uuid::Uuid;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// For 500 random domain strings, DomainMatcher never panics.
    #[test]
    fn domain_matcher_never_panics(domain in ".*") {
        let matcher = DomainMatcher::new(&[
            "api.openai.com".to_string(),
            "*.slack.com".to_string(),
            "api.anthropic.com".to_string(),
        ]);
        // Must not panic — result doesn't matter.
        let _ = matcher.matches(&domain);
    }

    /// For 500 random pattern strings, DomainMatcher::new never panics.
    #[test]
    fn domain_matcher_construction_never_panics(pattern in ".*") {
        let _ = DomainMatcher::new(&[pattern]);
    }

    /// For 500 random configs, ProxyEgressPolicy apply/check/remove never panics.
    #[test]
    fn proxy_policy_lifecycle_never_panics(
        mode in prop_oneof![
            Just(EgressPolicyMode::Allowlist),
            Just(EgressPolicyMode::Blocklist),
            Just(EgressPolicyMode::Unrestricted),
        ],
        domain in "[a-z]{1,20}\\.[a-z]{2,5}",
    ) {
        let policy = ProxyEgressPolicy::new();
        let agent = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: mode,
            allowed_domains: vec!["api.openai.com".to_string()],
            blocked_domains: vec!["evil.com".to_string()],
            ..Default::default()
        };

        // Apply should succeed.
        policy.apply(&agent, &config).unwrap();

        // Check should not panic.
        let _ = policy.check_domain(&agent, &domain);

        // Remove should succeed.
        policy.remove(&agent).unwrap();
    }

    /// For 500 random domain strings, allowlist mode returns consistent results.
    #[test]
    fn allowlist_consistency(domain in "[a-z0-9\\.\\-]{1,50}") {
        let matcher = DomainMatcher::new(&["api.openai.com".to_string()]);
        let result1 = matcher.matches(&domain);
        let result2 = matcher.matches(&domain);
        // Same input → same output (deterministic).
        prop_assert_eq!(result1, result2);
    }

    /// For 500 random violation sequences, threshold tracking is consistent.
    #[test]
    fn violation_tracking_consistent(count in 1u32..20) {
        let policy = ProxyEgressPolicy::new();
        let agent = Uuid::new_v4();
        let config = AgentEgressConfig {
            policy: EgressPolicyMode::Allowlist,
            allowed_domains: vec!["api.openai.com".to_string()],
            violation_threshold: 10,
            violation_window_minutes: 60,
            ..Default::default()
        };
        policy.apply(&agent, &config).unwrap();

        for i in 0..count {
            policy.log_violation(&agent, &format!("evil{i}.com"), "CONNECT");
        }

        let recorded = policy.violation_count(&agent);
        prop_assert_eq!(recorded, count);
    }
}
