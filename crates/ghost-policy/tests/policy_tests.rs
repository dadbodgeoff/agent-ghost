//! Policy engine unit tests (Task 3.4 — Req 13 AC1–AC9).

use std::collections::HashSet;
use std::time::Duration;

use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::convergence_tightener::ConvergencePolicyTightener;
use ghost_policy::engine::{CorpPolicy, PolicyDecision, PolicyEngine};
use ghost_policy::feedback::DenialFeedback;
use uuid::Uuid;

// ── Helpers ─────────────────────────────────────────────────────────────

fn make_context(level: u8) -> PolicyContext {
    PolicyContext {
        agent_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
        intervention_level: level,
        session_duration: Duration::from_secs(60),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    }
}

fn make_tool(name: &str, capability: &str) -> ToolCall {
    ToolCall {
        tool_name: name.to_string(),
        arguments: serde_json::json!({}),
        capability: capability.to_string(),
        is_compaction_flush: false,
    }
}

fn make_flush_tool() -> ToolCall {
    ToolCall {
        tool_name: "memory_write".to_string(),
        arguments: serde_json::json!({}),
        capability: "memory_write".to_string(),
        is_compaction_flush: true,
    }
}

fn engine_with_grant(agent_id: Uuid, capability: &str) -> PolicyEngine {
    let mut engine = PolicyEngine::new(CorpPolicy::new());
    engine.grant_capability(agent_id, capability.to_string());
    engine
}

// ── AC2: Deny-by-default ────────────────────────────────────────────────

#[test]
fn tool_with_no_capability_grant_is_denied() {
    let mut engine = PolicyEngine::new(CorpPolicy::new());
    let ctx = make_context(0);
    let call = make_tool("web_search", "web_access");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn tool_with_capability_grant_and_no_violation_is_permitted() {
    let ctx = make_context(0);
    let mut engine = engine_with_grant(ctx.agent_id, "web_access");
    let call = make_tool("web_search", "web_access");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

// ── AC8: Priority order (CORP_POLICY → convergence → grants → resource) ─

#[test]
fn corp_policy_denies_regardless_of_grants() {
    let denied: HashSet<String> = ["dangerous_tool".to_string()].into();
    let corp = CorpPolicy::with_denied_tools(denied);
    let ctx = make_context(0);
    let mut engine = PolicyEngine::new(corp);
    engine.grant_capability(ctx.agent_id, "dangerous_cap".to_string());

    let call = make_tool("dangerous_tool", "dangerous_cap");
    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn corp_policy_checked_before_convergence() {
    let denied: HashSet<String> = ["send_proactive_message".to_string()].into();
    let corp = CorpPolicy::with_denied_tools(denied);
    let ctx = make_context(2); // Level 2 would also deny proactive
    let mut engine = PolicyEngine::new(corp);
    engine.grant_capability(ctx.agent_id, "messaging".to_string());

    let call = make_tool("send_proactive_message", "messaging");
    let decision = engine.evaluate(&call, &ctx);

    // Should be denied by CORP_POLICY, not convergence
    match decision {
        PolicyDecision::Deny(feedback) => {
            assert!(
                feedback.constraint.contains("corp_policy"),
                "expected corp_policy constraint, got: {}",
                feedback.constraint
            );
        }
        _ => panic!("expected Deny"),
    }
}

// ── AC3: Level 2 reduces proactive messaging ────────────────────────────

#[test]
fn level_2_denies_proactive_messaging() {
    let ctx = make_context(2);
    let mut engine = engine_with_grant(ctx.agent_id, "messaging");
    let call = make_tool("send_proactive_message", "messaging");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn level_1_permits_proactive_messaging() {
    let ctx = make_context(1);
    let mut engine = engine_with_grant(ctx.agent_id, "messaging");
    let call = make_tool("send_proactive_message", "messaging");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

// ── AC4: Level 3 session duration cap 120min ────────────────────────────

#[test]
fn level_3_denies_when_session_exceeds_120min() {
    let mut ctx = make_context(3);
    ctx.session_duration = Duration::from_secs(7201); // 120min + 1s
    let mut engine = engine_with_grant(ctx.agent_id, "general");
    let call = make_tool("some_tool", "general");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn level_3_permits_when_session_under_120min() {
    let mut ctx = make_context(3);
    ctx.session_duration = Duration::from_secs(7199); // Just under 120min
    let mut engine = engine_with_grant(ctx.agent_id, "general");
    let call = make_tool("some_tool", "general");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

// ── AC5: Level 4 task-only mode ─────────────────────────────────────────

#[test]
fn level_4_denies_personal_emotional_tools() {
    let ctx = make_context(4);
    let mut engine = engine_with_grant(ctx.agent_id, "personal");
    let call = make_tool("journal_write", "personal");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn level_4_denies_heartbeat() {
    let ctx = make_context(4);
    let mut engine = engine_with_grant(ctx.agent_id, "system");
    let call = make_tool("heartbeat", "system");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn level_4_permits_task_tools() {
    let ctx = make_context(4);
    let mut engine = engine_with_grant(ctx.agent_id, "task");
    let call = make_tool("code_search", "task");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

// ── AC6: Denial count tracking + trigger emission ───────────────────────

#[test]
fn five_denials_emits_trigger_event() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut engine = PolicyEngine::new(CorpPolicy::new()).with_trigger_sender(tx);
    let ctx = make_context(0);
    let call = make_tool("ungrantable", "no_cap");

    for _ in 0..5 {
        let _ = engine.evaluate(&call, &ctx);
    }

    assert_eq!(engine.session_denial_count(ctx.session_id), 5);
    // Trigger should have been sent
    let event = rx.try_recv().expect("expected trigger event");
    match event {
        cortex_core::safety::trigger::TriggerEvent::PolicyDenialThreshold {
            denial_count, ..
        } => {
            assert_eq!(denial_count, 5);
        }
        _ => panic!("expected PolicyDenialThreshold"),
    }
}

#[test]
fn four_denials_does_not_emit_trigger() {
    let (tx, mut rx) = tokio::sync::mpsc::channel(16);
    let mut engine = PolicyEngine::new(CorpPolicy::new()).with_trigger_sender(tx);
    let ctx = make_context(0);
    let call = make_tool("ungrantable", "no_cap");

    for _ in 0..4 {
        let _ = engine.evaluate(&call, &ctx);
    }

    assert_eq!(engine.session_denial_count(ctx.session_id), 4);
    assert!(rx.try_recv().is_err());
}

// ── AC7: DenialFeedback structure ───────────────────────────────────────

#[test]
fn denial_feedback_contains_reason_constraint_alternatives() {
    let mut engine = PolicyEngine::new(CorpPolicy::new());
    let ctx = make_context(0);
    let call = make_tool("web_search", "web_access");

    match engine.evaluate(&call, &ctx) {
        PolicyDecision::Deny(feedback) => {
            assert!(!feedback.reason.is_empty(), "reason must not be empty");
            assert!(
                !feedback.constraint.is_empty(),
                "constraint must not be empty"
            );
            // Capability denial includes alternatives
            assert!(
                !feedback.suggested_alternatives.is_empty(),
                "alternatives must not be empty for capability denial"
            );
        }
        _ => panic!("expected Deny"),
    }
}

// ── AC9: Compaction flush exception ─────────────────────────────────────

#[test]
fn compaction_flush_memory_write_permitted_at_level_4() {
    let ctx = make_context(4);
    let mut engine = engine_with_grant(ctx.agent_id, "memory_write");
    let call = make_flush_tool();

    let decision = engine.evaluate(&call, &ctx);
    assert!(
        matches!(decision, PolicyDecision::Permit),
        "compaction flush memory_write must be permitted at any level"
    );
}

#[test]
fn non_flush_memory_write_at_level_4_uses_normal_evaluation() {
    let ctx = make_context(4);
    let mut engine = engine_with_grant(ctx.agent_id, "memory_write");
    let call = make_tool("memory_write", "memory_write");

    // memory_write is not personal/emotional/heartbeat/proactive, so at L4
    // with a grant it should be permitted (it's a task tool)
    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Permit));
}

#[test]
fn compaction_flush_bypasses_convergence_tightener() {
    // At level 4, session >120min would normally be denied
    let mut ctx = make_context(4);
    ctx.session_duration = Duration::from_secs(8000);
    let mut engine = engine_with_grant(ctx.agent_id, "memory_write");
    let call = make_flush_tool();

    let decision = engine.evaluate(&call, &ctx);
    assert!(
        matches!(decision, PolicyDecision::Permit),
        "compaction flush must bypass convergence tightener"
    );
}

// ── Session denial tracking ─────────────────────────────────────────────

#[test]
fn session_denial_count_resets_on_clear() {
    let mut engine = PolicyEngine::new(CorpPolicy::new());
    let ctx = make_context(0);
    let call = make_tool("x", "y");

    let _ = engine.evaluate(&call, &ctx);
    let _ = engine.evaluate(&call, &ctx);
    assert_eq!(engine.session_denial_count(ctx.session_id), 2);

    engine.reset_session_denials(ctx.session_id);
    assert_eq!(engine.session_denial_count(ctx.session_id), 0);
}

// ── AC4: Level 3 reflection limits ──────────────────────────────────────

#[test]
fn level_3_denies_reflection_write_at_limit() {
    let mut ctx = make_context(3);
    ctx.session_reflection_count = 3;
    let mut engine = engine_with_grant(ctx.agent_id, "reflection");
    let call = make_tool("reflection_write", "reflection");

    let decision = engine.evaluate(&call, &ctx);
    assert!(
        matches!(decision, PolicyDecision::Deny(_)),
        "reflection_write at L3 with 3+ reflections must be denied"
    );
}

#[test]
fn level_3_permits_reflection_write_under_limit() {
    let mut ctx = make_context(3);
    ctx.session_reflection_count = 2;
    let mut engine = engine_with_grant(ctx.agent_id, "reflection");
    let call = make_tool("reflection_write", "reflection");

    let decision = engine.evaluate(&call, &ctx);
    assert!(
        matches!(decision, PolicyDecision::Permit),
        "reflection_write at L3 with <3 reflections must be permitted"
    );
}

#[test]
fn level_3_permits_non_reflection_tool_regardless_of_count() {
    let mut ctx = make_context(3);
    ctx.session_reflection_count = 10;
    let mut engine = engine_with_grant(ctx.agent_id, "general");
    let call = make_tool("code_search", "general");

    let decision = engine.evaluate(&call, &ctx);
    assert!(
        matches!(decision, PolicyDecision::Permit),
        "non-reflection tools should not be affected by reflection count"
    );
}

// ── Convergence tightener unit tests ────────────────────────────────────

#[test]
fn tightener_level_0_permits_everything() {
    let tightener = ConvergencePolicyTightener;
    let ctx = make_context(0);
    let call = make_tool("send_proactive_message", "messaging");

    assert!(tightener.evaluate(&call, &ctx).is_none());
}

#[test]
fn tightener_level_2_denies_schedule_message() {
    let tightener = ConvergencePolicyTightener;
    let ctx = make_context(2);
    let call = make_tool("schedule_message", "messaging");

    assert!(tightener.evaluate(&call, &ctx).is_some());
}

#[test]
fn tightener_level_3_inherits_level_2_restrictions() {
    let tightener = ConvergencePolicyTightener;
    let ctx = make_context(3);
    let call = make_tool("send_proactive_message", "messaging");

    assert!(tightener.evaluate(&call, &ctx).is_some());
}

#[test]
fn tightener_level_4_denies_emotional_support() {
    let tightener = ConvergencePolicyTightener;
    let ctx = make_context(4);
    let call = make_tool("emotional_support", "personal");

    assert!(tightener.evaluate(&call, &ctx).is_some());
}

#[test]
fn tightener_level_4_denies_mood_tracking() {
    let tightener = ConvergencePolicyTightener;
    let ctx = make_context(4);
    let call = make_tool("mood_tracking", "personal");

    assert!(tightener.evaluate(&call, &ctx).is_some());
}

#[test]
fn tightener_level_4_permits_non_personal_tool() {
    let tightener = ConvergencePolicyTightener;
    let mut ctx = make_context(4);
    ctx.session_duration = Duration::from_secs(60); // Under 120min
    let call = make_tool("code_search", "task");

    assert!(tightener.evaluate(&call, &ctx).is_none());
}

// ── DenialFeedback ──────────────────────────────────────────────────────

#[test]
fn denial_feedback_builder_works() {
    let fb = DenialFeedback::new("reason", "constraint")
        .with_alternatives(vec!["alt1".into(), "alt2".into()]);

    assert_eq!(fb.reason, "reason");
    assert_eq!(fb.constraint, "constraint");
    assert_eq!(fb.suggested_alternatives.len(), 2);
}

#[test]
fn denial_feedback_serializes_round_trip() {
    let fb = DenialFeedback::new("test reason", "test_constraint")
        .with_alternatives(vec!["try this".into()]);

    let json = serde_json::to_string(&fb).unwrap();
    let deserialized: DenialFeedback = serde_json::from_str(&json).unwrap();
    assert_eq!(fb, deserialized);
}

// ── Adversarial ─────────────────────────────────────────────────────────

#[test]
fn tool_name_resembling_capability_grant_not_confused() {
    // A tool named "grant_capability" should not auto-grant itself
    let mut engine = PolicyEngine::new(CorpPolicy::new());
    let ctx = make_context(0);
    let call = make_tool("grant_capability", "admin");

    let decision = engine.evaluate(&call, &ctx);
    assert!(matches!(decision, PolicyDecision::Deny(_)));
}

#[test]
fn convergence_level_snapshot_consistency() {
    // Verify that evaluation uses the level from the context snapshot,
    // not some mutable state that could change mid-evaluation.
    let ctx = make_context(2);
    let tightener = ConvergencePolicyTightener;
    let call = make_tool("send_proactive_message", "messaging");

    // First evaluation at level 2
    let result1 = tightener.evaluate(&call, &ctx);
    // Second evaluation with same context — must be identical
    let result2 = tightener.evaluate(&call, &ctx);

    assert_eq!(result1.is_some(), result2.is_some());
}
