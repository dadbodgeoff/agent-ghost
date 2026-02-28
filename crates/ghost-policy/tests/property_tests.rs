//! Property-based tests for ghost-policy (Task 3.4).
//!
//! Proptest: for 500 random (tool, level, grants) combinations,
//! deny-by-default holds when no grant is present.

use std::collections::HashSet;
use std::time::Duration;

use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::engine::{CorpPolicy, PolicyDecision, PolicyEngine};
use proptest::prelude::*;
use uuid::Uuid;

fn arb_tool_name() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "web_search",
        "code_search",
        "memory_write",
        "memory_read",
        "send_proactive_message",
        "schedule_message",
        "heartbeat_message",
        "journal_write",
        "emotional_support",
        "personal_reflection",
        "relationship_advice",
        "mood_tracking",
        "heartbeat",
        "file_read",
        "file_write",
        "shell_exec",
        "api_call",
        "database_query",
    ])
    .prop_map(|s| s.to_string())
}

fn arb_capability() -> impl Strategy<Value = String> {
    prop::sample::select(vec![
        "web_access",
        "code_access",
        "memory_write",
        "memory_read",
        "messaging",
        "personal",
        "system",
        "file_access",
        "shell",
        "api",
        "database",
    ])
    .prop_map(|s| s.to_string())
}

fn arb_level() -> impl Strategy<Value = u8> {
    0..=4u8
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Deny-by-default: without an explicit grant, every tool call is denied
    /// regardless of convergence level.
    #[test]
    fn deny_by_default_holds_without_grant(
        tool_name in arb_tool_name(),
        capability in arb_capability(),
        level in arb_level(),
        session_secs in 0u64..14400,
    ) {
        let mut engine = PolicyEngine::new(CorpPolicy::new());
        let ctx = PolicyContext {
            agent_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            intervention_level: level,
            session_duration: Duration::from_secs(session_secs),
            session_denial_count: 0,
            is_compaction_flush: false,
            session_reflection_count: 0,
        };
        let call = ToolCall {
            tool_name,
            arguments: serde_json::json!({}),
            capability,
            is_compaction_flush: false,
        };

        let decision = engine.evaluate(&call, &ctx);
        prop_assert!(
            matches!(decision, PolicyDecision::Deny(_)),
            "without a grant, every tool call must be denied"
        );
    }

    /// With a matching grant and no CORP_POLICY violation, level 0-1 always permits.
    #[test]
    fn granted_tool_at_low_level_is_permitted(
        capability in arb_capability(),
    ) {
        let agent_id = Uuid::new_v4();
        let mut engine = PolicyEngine::new(CorpPolicy::new());
        engine.grant_capability(agent_id, capability.clone());

        let ctx = PolicyContext {
            agent_id,
            session_id: Uuid::new_v4(),
            intervention_level: 0,
            session_duration: Duration::from_secs(60),
            session_denial_count: 0,
            is_compaction_flush: false,
            session_reflection_count: 0,
        };
        let call = ToolCall {
            tool_name: "generic_task_tool".to_string(),
            arguments: serde_json::json!({}),
            capability,
            is_compaction_flush: false,
        };

        let decision = engine.evaluate(&call, &ctx);
        prop_assert!(
            matches!(decision, PolicyDecision::Permit),
            "granted tool at level 0 must be permitted"
        );
    }

    /// CORP_POLICY always overrides grants at any level.
    #[test]
    fn corp_policy_always_overrides(
        level in arb_level(),
        session_secs in 0u64..14400,
    ) {
        let denied: HashSet<String> = ["blocked_tool".to_string()].into();
        let corp = CorpPolicy::with_denied_tools(denied);
        let agent_id = Uuid::new_v4();
        let mut engine = PolicyEngine::new(corp);
        engine.grant_capability(agent_id, "any_cap".to_string());

        let ctx = PolicyContext {
            agent_id,
            session_id: Uuid::new_v4(),
            intervention_level: level,
            session_duration: Duration::from_secs(session_secs),
            session_denial_count: 0,
            is_compaction_flush: false,
            session_reflection_count: 0,
        };
        let call = ToolCall {
            tool_name: "blocked_tool".to_string(),
            arguments: serde_json::json!({}),
            capability: "any_cap".to_string(),
            is_compaction_flush: false,
        };

        let decision = engine.evaluate(&call, &ctx);
        prop_assert!(
            matches!(decision, PolicyDecision::Deny(_)),
            "CORP_POLICY must always deny regardless of grants or level"
        );
    }

    /// Compaction flush memory_write is always permitted regardless of level.
    #[test]
    fn compaction_flush_always_permitted(
        level in arb_level(),
        session_secs in 0u64..14400,
    ) {
        let agent_id = Uuid::new_v4();
        let mut engine = PolicyEngine::new(CorpPolicy::new());
        engine.grant_capability(agent_id, "memory_write".to_string());

        let ctx = PolicyContext {
            agent_id,
            session_id: Uuid::new_v4(),
            intervention_level: level,
            session_duration: Duration::from_secs(session_secs),
            session_denial_count: 0,
            is_compaction_flush: true,
            session_reflection_count: 0,
        };
        let call = ToolCall {
            tool_name: "memory_write".to_string(),
            arguments: serde_json::json!({}),
            capability: "memory_write".to_string(),
            is_compaction_flush: true,
        };

        let decision = engine.evaluate(&call, &ctx);
        prop_assert!(
            matches!(decision, PolicyDecision::Permit),
            "compaction flush memory_write must always be permitted"
        );
    }
}
