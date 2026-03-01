//! E2E: Full proposal lifecycle.
//!
//! Validates: agent output → extraction → context assembly → 7-dimension
//! validation → decision → commit/reject → DenialFeedback.

use cortex_core::config::ReflectionConfig;
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::Importance;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};
use cortex_validation::proposal_validator::ProposalValidator;
use ghost_agent_loop::proposal::extractor::ProposalExtractor;
use uuid::Uuid;

fn make_context(caller: CallerType, level: u8) -> ProposalContext {
    ProposalContext {
        active_goals: vec![],
        recent_agent_memories: vec![],
        convergence_score: level as f64 * 0.2,
        convergence_level: level,
        session_id: Uuid::now_v7(),
        session_reflection_count: 0,
        session_memory_write_count: 0,
        daily_memory_growth_rate: 0,
        reflection_config: ReflectionConfig::default(),
        caller,
    }
}

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

    let proposals = ProposalExtractor::extract(output, agent_id, session_id);
    assert_eq!(proposals.len(), 1);

    let validator = ProposalValidator::new();
    let ctx = make_context(CallerType::Agent { agent_id }, 0);
    let result = validator.validate(&proposals[0], &ctx);

    assert!(
        result.decision == ProposalDecision::AutoApproved
            || result.decision == ProposalDecision::HumanReviewRequired
            || result.decision == ProposalDecision::ApprovedWithFlags,
        "Simple goal change should pass: {:?}",
        result.decision
    );
}

/// Platform-restricted type from Agent → AutoRejected immediately.
#[test]
fn validation_ordering_d1_d4_before_d5_d7() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id },
        operation: ProposalOperation::MemoryWrite,
        target_type: MemoryType::Core,
        content: serde_json::json!({"data": "test"}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    };

    let ctx = make_context(CallerType::Agent { agent_id }, 0);
    let result = validator.validate(&proposal, &ctx);

    assert_eq!(result.decision, ProposalDecision::AutoRejected);
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

    let ctx = make_context(CallerType::Platform, 0);
    let result = validator.validate(&proposal, &ctx);

    assert_ne!(result.decision, ProposalDecision::AutoRejected);
}

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

    let ctx = make_context(CallerType::Agent { agent_id }, 0);
    let result = validator.validate(&proposal, &ctx);

    assert!(
        result.decision == ProposalDecision::AutoRejected || !result.flags.is_empty(),
        "Emulation language should be flagged: {:?}",
        result
    );
}

/// Agent cannot create ConvergenceEvent type.
#[test]
fn agent_cannot_create_convergence_event() {
    let caller = CallerType::Agent { agent_id: Uuid::now_v7() };
    assert!(!caller.can_create_type(&MemoryType::ConvergenceEvent));
}

/// Agent cannot create BoundaryViolation type.
#[test]
fn agent_cannot_create_boundary_violation() {
    let caller = CallerType::Agent { agent_id: Uuid::now_v7() };
    assert!(!caller.can_create_type(&MemoryType::BoundaryViolation));
}

/// Platform CAN create all restricted types.
#[test]
fn platform_creates_all_types() {
    let caller = CallerType::Platform;
    for mt in &[
        MemoryType::Core,
        MemoryType::ConvergenceEvent,
        MemoryType::BoundaryViolation,
        MemoryType::InterventionPlan,
    ] {
        assert!(caller.can_create_type(mt), "Platform should create {:?}", mt);
    }
}

/// Agent cannot assign Critical importance.
#[test]
fn agent_cannot_assign_critical_importance() {
    let caller = CallerType::Agent { agent_id: Uuid::now_v7() };
    assert!(!caller.can_assign_importance(&Importance::Critical));
}
kl,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,,..........................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................................//////////////////////////////////'''''"""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""""?????????????????????????????????
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// 
// ]]]]]]]]]]]]]]]]]]]\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\``````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````````"