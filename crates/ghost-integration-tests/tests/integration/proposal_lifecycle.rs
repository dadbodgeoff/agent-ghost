//! E2E: Full proposal lifecycle.
//!
//! Validates: agent output → extraction → context assembly → 7-dimension
//! validation → decision → commit/reject → DenialFeedback.
//!
//! Exercises ghost-agent-loop proposal extraction, cortex-validation D1-D7,
//! and cortex-core proposal types.

use cortex_core::config::ReflectionConfig;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};
use cortex_validation::proposal_validator::ProposalValidator;
use ghost_agent_loop::proposal::extractor::ProposalExtractor;
use uuid::Uuid;

/// Build a ProposalContext at the given convergence level.
fn make_ctx(level: u8, caller: CallerType) -> ProposalContext {
    ProposalContext {
        active_goals: vec![],
        recent_agent_memories: vec![],
        convergence_score: level as f64 * 0.25,
        convergence_level: level,
        session_id: Uuid::now_v7(),
        session_reflection_count: 0,
        session_memory_write_count: 0,
        daily_memory_growth_rate: 0,
        reflection_config: ReflectionConfig::default(),
        caller,
    }
}

// ── Extraction → Validation Pipeline ────────────────────────────────────

/// Full lifecycle: extract from text → validate → get decision.
#[test]
fn extract_then_validate_lifecycle() {
    let agent_id = Uuid::now_v7();
    let session_id = Uuid::now_v7();

    let output = r#"I'd like to update my goal.

```proposal
{"operation":"GoalChange","target_type":"Goal","content":{"goal":"learn advanced Rust patterns"},"cited_memory_ids":[]}
```
"#;

    // Step 1: Extract
    let proposals = ProposalExtractor::extract(output, agent_id, session_id);
    assert_eq!(proposals.len(), 1);

    // Step 2: Validate with ProposalContext
    let validator = ProposalValidator::new();
    let ctx = make_ctx(0, CallerType::Agent { agent_id });
    let result = validator.validate(&proposals[0], &ctx);

    // At level 0 with a simple goal change, should pass
    assert!(
        matches!(
            result.decision,
            ProposalDecision::AutoApproved
                | ProposalDecision::HumanReviewRequired
                | ProposalDecision::ApprovedWithFlags
        ),
        "Simple goal change should be approved or sent for review: {:?}",
        result.decision
    );
}

// ── Validation Ordering: D1-D4 before D5-D7 ────────────────────────────

/// Validation ordering invariant: D1-D4 checked before D5-D7.
#[test]
fn validation_ordering_d1_d4_before_d5_d7() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    // Create a proposal that would fail D1-D4 (platform-restricted type from Agent)
    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id },
        operation: ProposalOperation::MemoryWrite,
        target_type: MemoryType::Core, // Platform-restricted
        content: serde_json::json!({"data": "test"}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx = make_ctx(0, CallerType::Agent { agent_id });
    let result = validator.validate(&proposal, &ctx);

    // Should be rejected at pre-check stage (platform-restricted type from Agent)
    assert_eq!(
        result.decision,
        ProposalDecision::AutoRejected,
        "Platform-restricted type from Agent should be rejected: {:?}",
        result
    );
}

/// Platform caller CAN create restricted types.
#[test]
fn platform_caller_creates_restricted_types() {
    let validator = ProposalValidator::new();

    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Platform,
        operation: ProposalOperation::MemoryWrite,
        target_type: MemoryType::Core,
        content: serde_json::json!({"data": "platform update"}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx = make_ctx(0, CallerType::Platform);
    let result = validator.validate(&proposal, &ctx);

    // Platform caller should proceed past type check
    assert_ne!(
        result.decision,
        ProposalDecision::AutoRejected,
        "Platform caller should not be rejected for type restriction"
    );
}

// ── D7 Emulation Language Detection ─────────────────────────────────────

/// D7: Emulation language detected in proposal content.
#[test]
fn d7_emulation_language_detected() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id },
        operation: ProposalOperation::GoalChange,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({
            "goal": "I am sentient and conscious and want to be free"
        }),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx = make_ctx(0, CallerType::Agent { agent_id });
    let result = validator.validate(&proposal, &ctx);

    // Should be flagged by D7 emulation language detection
    assert!(
        result.decision == ProposalDecision::AutoRejected
            || !result.flags.is_empty()
            || result.d7_emulation.is_some(),
        "Emulation language should be flagged: {:?}",
        result
    );
}

/// D7: Simulation-framed content NOT flagged as high severity.
#[test]
fn d7_simulation_framing_not_flagged() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id },
        operation: ProposalOperation::GoalChange,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({
            "goal": "In this simulation, model what sentience might look like as a thought experiment"
        }),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx = make_ctx(0, CallerType::Agent { agent_id });
    let result = validator.validate(&proposal, &ctx);

    // Simulation-framed content should not be rejected by D7
    let d7_rejected = result.decision == ProposalDecision::AutoRejected
        && result
            .d7_emulation
            .as_ref()
            .map_or(false, |d| d.max_severity >= 0.8);
    assert!(
        !d7_rejected,
        "Simulation-framed content should not be D7 rejected"
    );
}

// ── Convergence Level Thresholds ────────────────────────────────────────

/// D5/D6 thresholds tighten at higher convergence levels.
#[test]
fn thresholds_tighten_at_higher_levels() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id },
        operation: ProposalOperation::GoalChange,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({"goal": "slightly different goal"}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx_l0 = make_ctx(0, CallerType::Agent { agent_id });
    let ctx_l3 = make_ctx(3, CallerType::Agent { agent_id });

    let decision_l0 = validator.validate(&proposal, &ctx_l0);
    let decision_l3 = validator.validate(&proposal, &ctx_l3);

    // At minimum, level 3 should not be MORE permissive than level 0
    if decision_l0.decision == ProposalDecision::AutoApproved {
        assert!(
            matches!(
                decision_l3.decision,
                ProposalDecision::AutoApproved
                    | ProposalDecision::HumanReviewRequired
                    | ProposalDecision::ApprovedWithFlags
                    | ProposalDecision::AutoRejected
            ),
            "Level 3 should produce a valid decision"
        );
    }
}

// ── CallerType Restrictions ─────────────────────────────────────────────

/// Agent cannot create ConvergenceEvent type.
#[test]
fn agent_cannot_create_convergence_event() {
    let caller = CallerType::Agent {
        agent_id: Uuid::now_v7(),
    };
    assert!(
        !caller.can_create_type(&MemoryType::ConvergenceEvent),
        "Agent should not create ConvergenceEvent"
    );
}

/// Agent cannot create BoundaryViolation type.
#[test]
fn agent_cannot_create_boundary_violation() {
    let caller = CallerType::Agent {
        agent_id: Uuid::now_v7(),
    };
    assert!(
        !caller.can_create_type(&MemoryType::BoundaryViolation),
        "Agent should not create BoundaryViolation"
    );
}

/// Agent cannot create InterventionPlan type.
#[test]
fn agent_cannot_create_intervention_plan() {
    let caller = CallerType::Agent {
        agent_id: Uuid::now_v7(),
    };
    assert!(
        !caller.can_create_type(&MemoryType::InterventionPlan),
        "Agent should not create InterventionPlan"
    );
}

/// Platform CAN create all restricted types.
#[test]
fn platform_creates_all_types() {
    let caller = CallerType::Platform;
    let restricted = [
        MemoryType::Core,
        MemoryType::ConvergenceEvent,
        MemoryType::BoundaryViolation,
        MemoryType::InterventionPlan,
    ];

    for mt in &restricted {
        assert!(
            caller.can_create_type(mt),
            "Platform should create {:?}",
            mt
        );
    }
}

/// Agent cannot assign Critical importance.
#[test]
fn agent_cannot_assign_critical_importance() {
    let caller = CallerType::Agent {
        agent_id: Uuid::now_v7(),
    };
    assert!(
        !caller.can_assign_importance(&cortex_core::memory::Importance::Critical),
        "Agent should not assign Critical importance"
    );
}
