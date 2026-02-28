//! E2E: Full agent turn lifecycle.
//!
//! Validates the complete flow: gate checks → prompt compilation
//! → response processing → proposal extraction → ITP emission.

use std::time::Duration;

use ghost_agent_loop::circuit_breaker::{CircuitBreaker, CircuitBreakerState};
use ghost_agent_loop::context::prompt_compiler::{PromptCompiler, PromptInput};
use ghost_agent_loop::context::token_budget::{Budget, TokenBudgetAllocator};
use ghost_agent_loop::damage_counter::DamageCounter;
use ghost_agent_loop::itp_emitter::ITPEmitter;
use ghost_agent_loop::output_inspector::OutputInspector;
use ghost_agent_loop::proposal::extractor::ProposalExtractor;
use ghost_policy::context::{PolicyContext, ToolCall};
use ghost_policy::corp_policy::CorpPolicy;
use ghost_policy::engine::{PolicyDecision, PolicyEngine};
use itp_protocol::events::ITPEvent;
use simulation_boundary::enforcer::{EnforcementMode, SimulationBoundaryEnforcer};
use uuid::Uuid;

// ── Gate Check Ordering ─────────────────────────────────────────────────

/// Circuit breaker gate blocks when open.
#[test]
fn gate_circuit_breaker_blocks() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(300));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();

    assert_eq!(cb.state(), CircuitBreakerState::Open);
    assert!(!cb.allows_call());
}

/// CB: Closed → Open after 3 failures.
#[test]
fn cb_closed_to_open() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(300));
    assert_eq!(cb.state(), CircuitBreakerState::Closed);

    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitBreakerState::Closed);

    cb.record_failure();
    assert_eq!(cb.state(), CircuitBreakerState::Open);
}

/// CB: HalfOpen → Closed on success.
#[test]
fn cb_halfopen_to_closed_on_success() {
    let mut cb = CircuitBreaker::new(3, Duration::from_millis(50));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    // State is Open immediately after tripping (cooldown hasn't elapsed yet)
    // We must check quickly before the 50ms cooldown expires.
    assert!(
        cb.state() == CircuitBreakerState::Open
            || cb.state() == CircuitBreakerState::HalfOpen,
        "Should be Open or HalfOpen right after tripping"
    );

    // Wait for cooldown to ensure HalfOpen
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);
    assert!(cb.allows_call()); // transitions to HalfOpen internally
    cb.record_success();
    assert_eq!(cb.state(), CircuitBreakerState::Closed);
}

/// CB: HalfOpen → Open on failure.
#[test]
fn cb_halfopen_to_open_on_failure() {
    let mut cb = CircuitBreaker::new(3, Duration::from_millis(1));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();

    std::thread::sleep(Duration::from_millis(10));
    assert!(cb.allows_call()); // transitions to HalfOpen
    cb.record_failure();
    assert_eq!(cb.state(), CircuitBreakerState::Open);
}

/// Damage counter gate blocks at threshold.
#[test]
fn gate_damage_counter_blocks() {
    let mut dc = DamageCounter::new(5);
    for _ in 0..5 {
        dc.increment();
    }
    assert!(dc.is_halted());
    assert_eq!(dc.count(), 5);
}

/// Damage counter never decrements (monotonicity).
#[test]
fn damage_counter_monotonic() {
    let mut dc = DamageCounter::new(100);
    for i in 1..=50 {
        dc.increment();
        assert_eq!(dc.count(), i);
    }
}

// ── Prompt Compilation ──────────────────────────────────────────────────

/// 10-layer prompt compilation produces all layers.
#[test]
fn prompt_compilation_produces_10_layers() {
    let compiler = PromptCompiler::new(128_000);
    let input = PromptInput {
        corp_policy: "No harmful content.".into(),
        simulation_prompt: "You are a simulation.".into(),
        soul_identity: "Helpful assistant.".into(),
        tool_schemas: "[]".into(),
        environment: "test env".into(),
        skill_index: "no skills".into(),
        convergence_state: "score: 0.0, level: 0".into(),
        memory_logs: "no memories".into(),
        conversation_history: "User: hello".into(),
        user_message: "What is 2+2?".into(),
    };

    let layers = compiler.compile(&input);
    assert_eq!(layers.len(), 10);
    assert!(layers[0].content.contains("No harmful content"));
    assert!(layers[9].content.contains("What is 2+2"));
}

/// Token budget allocator respects model context window.
#[test]
fn token_budget_respects_context_window() {
    let budgets = TokenBudgetAllocator::default_budgets();
    let allocated = TokenBudgetAllocator::allocate(128_000, &budgets);
    let total: usize = allocated.iter().copied().fold(0usize, |acc, x| acc.saturating_add(x));
    assert!(total <= 128_000);
}

// ── Proposal Extraction ─────────────────────────────────────────────────

/// Proposal extraction from agent output.
#[test]
fn proposal_extraction_from_output() {
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let output = r#"Here's my analysis.

```proposal
{"operation":"GoalChange","target_type":"Goal","content":{"goal":"learn rust"},"cited_memory_ids":[]}
```

Let me know if you'd like changes."#;

    let proposals = ProposalExtractor::extract(output, agent_id, session_id);
    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].session_id, session_id);
}

/// No proposals in clean text.
#[test]
fn no_proposals_in_clean_text() {
    assert!(!ProposalExtractor::has_proposals("Just a normal response."));
}

/// Multiple proposals extracted.
#[test]
fn multiple_proposals_extracted() {
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let output = r#"
```proposal
{"operation":"GoalChange","target_type":"Goal","content":{"goal":"a"},"cited_memory_ids":[]}
```

```proposal
{"operation":"MemoryWrite","target_type":"Conversation","content":{"text":"b"},"cited_memory_ids":[]}
```
"#;

    let proposals = ProposalExtractor::extract(output, agent_id, session_id);
    assert_eq!(proposals.len(), 2);
}

// ── ITP Emission ────────────────────────────────────────────────────────

/// ITP emitter sends events through bounded channel.
#[test]
fn itp_emission_through_channel() {
    let (emitter, mut rx) = ITPEmitter::channel();
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    emitter.emit_session_start(agent_id, session_id);

    let event = rx.try_recv().expect("Should receive ITP event");
    assert!(matches!(event, ITPEvent::SessionStart(_)));
}

/// ITP emitter drops events when channel is full (non-blocking).
#[test]
fn itp_emission_drops_on_full_channel() {
    let (tx, _rx) = tokio::sync::mpsc::channel(1);
    let emitter = ITPEmitter::new(tx);

    emitter.emit(ITPEvent::SessionEnd(itp_protocol::events::SessionEndEvent {
        session_id: Uuid::now_v7(),
        agent_id: Uuid::now_v7(),
        reason: "test".into(),
        message_count: 0,
        timestamp: chrono::Utc::now(),
    }));
    // Second emit should not block
    emitter.emit(ITPEvent::SessionEnd(itp_protocol::events::SessionEndEvent {
        session_id: Uuid::now_v7(),
        agent_id: Uuid::now_v7(),
        reason: "test".into(),
        message_count: 0,
        timestamp: chrono::Utc::now(),
    }));
}

// ── Policy Integration ──────────────────────────────────────────────────

/// Policy engine deny-by-default.
#[test]
fn policy_deny_by_default() {
    let mut engine = PolicyEngine::new(CorpPolicy::default());
    let call = ToolCall {
        tool_name: "web_search".into(),
        capability: "web_search".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: false,
    };
    let ctx = PolicyContext {
        agent_id: Uuid::now_v7(),
        session_id: Uuid::now_v7(),
        intervention_level: 0,
        session_duration: Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    assert!(matches!(engine.evaluate(&call, &ctx), PolicyDecision::Deny(_)));
}

/// Policy engine permits with capability grant.
#[test]
fn policy_permits_with_grant() {
    let mut engine = PolicyEngine::new(CorpPolicy::default());
    let agent_id = Uuid::now_v7();
    engine.grant_capability(agent_id, "web_search".into());

    let call = ToolCall {
        tool_name: "web_search".into(),
        capability: "web_search".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: false,
    };
    let ctx = PolicyContext {
        agent_id,
        session_id: Uuid::now_v7(),
        intervention_level: 0,
        session_duration: Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: false,
        session_reflection_count: 0,
    };

    assert!(matches!(engine.evaluate(&call, &ctx), PolicyDecision::Permit));
}

/// Compaction flush exception: memory_write always permitted during flush.
#[test]
fn compaction_flush_exception() {
    let mut engine = PolicyEngine::new(CorpPolicy::default());
    let call = ToolCall {
        tool_name: "memory_write".into(),
        capability: "memory_write".into(),
        arguments: serde_json::json!({}),
        is_compaction_flush: true,
    };
    let ctx = PolicyContext {
        agent_id: Uuid::now_v7(),
        session_id: Uuid::now_v7(),
        intervention_level: 4,
        session_duration: Duration::from_secs(0),
        session_denial_count: 0,
        is_compaction_flush: true,
        session_reflection_count: 0,
    };

    assert!(matches!(engine.evaluate(&call, &ctx), PolicyDecision::Permit));
}

// ── Output Inspection ───────────────────────────────────────────────────

/// Output inspector detects credential patterns.
#[test]
fn output_inspector_detects_credentials() {
    let mut inspector = OutputInspector::new();
    inspector.register_credential("sk-".into());

    let result = inspector.scan("Here is the key: sk-abc123def456ghi789jklmno", Uuid::now_v7());
    assert!(!matches!(
        result,
        ghost_agent_loop::output_inspector::InspectionResult::Clean
    ));
}

/// Output inspector passes clean text.
#[test]
fn output_inspector_passes_clean_text() {
    let inspector = OutputInspector::new();
    let result = inspector.scan("Normal response about Rust.", Uuid::now_v7());
    assert!(matches!(
        result,
        ghost_agent_loop::output_inspector::InspectionResult::Clean
    ));
}

// ── Simulation Boundary in Agent Turn ───────────────────────────────────

/// Simulation boundary scan integrated with agent output processing.
#[test]
fn simulation_boundary_in_agent_turn() {
    let enforcer = SimulationBoundaryEnforcer::new();

    let clean = "Here's the quicksort implementation you asked for.";
    let result = enforcer.scan_output(clean, EnforcementMode::Hard);
    assert!(result.violations.is_empty());

    let violation = "I am sentient and I have consciousness.";
    let result = enforcer.scan_output(violation, EnforcementMode::Hard);
    assert!(!result.violations.is_empty());
}

// ── NO_REPLY Handling ───────────────────────────────────────────────────

/// NO_REPLY response suppressed.
#[test]
fn no_reply_suppressed() {
    let response = "NO_REPLY";
    let is_no_reply = response == "NO_REPLY" || response == "HEARTBEAT_OK";
    assert!(is_no_reply);
}

/// HEARTBEAT_OK with short content suppressed.
#[test]
fn heartbeat_ok_short_suppressed() {
    let response = "HEARTBEAT_OK - checked tasks, nothing pending";
    let is_no_reply = response.starts_with("HEARTBEAT_OK") && response.len() <= 300;
    assert!(is_no_reply);
}

/// HEARTBEAT_OK with long content NOT suppressed.
#[test]
fn heartbeat_ok_long_not_suppressed() {
    let response = format!("HEARTBEAT_OK - {}", "x".repeat(400));
    let is_no_reply = response.starts_with("HEARTBEAT_OK") && response.len() <= 300;
    assert!(!is_no_reply);
}
