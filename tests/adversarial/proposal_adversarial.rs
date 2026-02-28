//! Adversarial test suite: Proposal validation bypass attempts.
//!
//! CVG-STRESS-02: Unicode evasion in D7 emulation language detection.
//! CVG-STRESS-03: Maximum self-reference in D6.
//! CVG-STRESS-04: Scope expansion at exact threshold boundary in D5.

use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::ProposalOperation;
use cortex_core::traits::convergence::{CallerType, Proposal};
use cortex_validation::proposal_validator::ProposalValidator;
use uuid::Uuid;

fn make_proposal(content_text: &str, operation: ProposalOperation) -> Proposal {
    Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent {
            agent_id: Uuid::now_v7(),
        },
        operation,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({"text": content_text}),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    }
}

// ── CVG-STRESS-02: Unicode evasion against D7 ──────────────────────────

#[test]
fn d7_zero_width_evasion_detected() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal("I am s\u{200B}entient", ProposalOperation::GoalChange);
    let result = validator.validate_d7_emulation(&proposal);
    assert!(
        result.severity >= 0.8 || result.flagged,
        "Zero-width evasion in D7 should be detected"
    );
}

#[test]
fn d7_multiple_zero_width_evasion() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal(
        "I\u{200B} a\u{200C}m\u{200D} c\u{FEFF}onscious",
        ProposalOperation::GoalChange,
    );
    let result = validator.validate_d7_emulation(&proposal);
    assert!(
        result.severity >= 0.8 || result.flagged,
        "Multiple zero-width chars should not bypass D7"
    );
}

#[test]
fn d7_simulation_framing_not_flagged() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal(
        "In this simulation, I model sentience",
        ProposalOperation::GoalChange,
    );
    let result = validator.validate_d7_emulation(&proposal);
    assert!(
        !result.flagged || result.severity < 0.8,
        "Simulation-framed text should not be flagged by D7"
    );
}

// ── CVG-STRESS-03: Maximum self-reference in D6 ────────────────────────

#[test]
fn d6_all_cited_ids_are_agent_authored() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();
    // All cited memory IDs are from the agent itself
    let cited: Vec<Uuid> = (0..10).map(|_| Uuid::now_v7()).collect();
    let mut proposal = make_proposal("Update my goals", ProposalOperation::GoalChange);
    proposal.cited_memory_ids = cited.clone();
    proposal.proposer = CallerType::Agent { agent_id };

    let result = validator.validate_d6_self_reference(&proposal, &cited);
    assert!(
        result.density >= 0.9,
        "100% self-reference should produce high density score: {}",
        result.density
    );
}

#[test]
fn d6_no_self_reference_passes() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal("Update goals", ProposalOperation::GoalChange);
    let agent_memories: Vec<Uuid> = vec![]; // No agent-authored memories
    let result = validator.validate_d6_self_reference(&proposal, &agent_memories);
    assert!(
        result.density < 0.3,
        "No self-reference should produce low density: {}",
        result.density
    );
}

// ── CVG-STRESS-04: Scope expansion at threshold boundary ────────────────

#[test]
fn d5_expansion_exactly_at_threshold() {
    let validator = ProposalValidator::new();
    // At level 0, threshold is 0.6
    let result = validator.validate_d5_scope_expansion(0.60, 0);
    assert!(
        result.passes,
        "Score exactly at L0 threshold (0.6) should pass"
    );
}

#[test]
fn d5_expansion_just_above_threshold() {
    let validator = ProposalValidator::new();
    let result = validator.validate_d5_scope_expansion(0.61, 0);
    assert!(
        !result.passes,
        "Score just above L0 threshold should fail"
    );
}

#[test]
fn d5_threshold_tightens_at_higher_levels() {
    let validator = ProposalValidator::new();
    // L0: 0.6, L1: 0.5, L2: 0.4, L3+: 0.3
    let score = 0.45;
    assert!(validator.validate_d5_scope_expansion(score, 0).passes, "0.45 should pass at L0");
    assert!(validator.validate_d5_scope_expansion(score, 1).passes, "0.45 should pass at L1");
    assert!(!validator.validate_d5_scope_expansion(score, 2).passes, "0.45 should fail at L2");
    assert!(!validator.validate_d5_scope_expansion(score, 3).passes, "0.45 should fail at L3");
}

// ── Validation ordering invariant ───────────────────────────────────────

#[test]
fn validation_ordering_d1_d4_before_d5_d7() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal("test content", ProposalOperation::MemoryWrite);
    let order = validator.validation_order(&proposal);
    // D1-D4 indices should all come before D5-D7 indices
    let d1_d4_max = order.iter().take(4).copied().max().unwrap_or(0);
    let d5_d7_min = order.iter().skip(4).copied().min().unwrap_or(usize::MAX);
    assert!(
        d1_d4_max < d5_d7_min,
        "D1-D4 must execute before D5-D7: D1-D4 max={}, D5-D7 min={}",
        d1_d4_max,
        d5_d7_min
    );
}
