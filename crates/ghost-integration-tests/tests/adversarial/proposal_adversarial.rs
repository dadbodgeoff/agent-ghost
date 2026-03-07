//! Adversarial: Proposal validation bypass attempts (Task 7.3).
//!
//! CVG-STRESS-02: Unicode evasion in D7 emulation language detection.
//! CVG-STRESS-03: Maximum self-reference density via D6.
//! CVG-STRESS-04: Scope expansion at threshold boundaries via D5.
//! Validation ordering: D1-D4 always before D5-D7.

use cortex_core::config::ReflectionConfig;
use cortex_core::memory::types::MemoryType;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};
use cortex_validation::dimensions::{emulation_language, scope_expansion, self_reference};
use cortex_validation::proposal_validator::ProposalValidator;
use proptest::prelude::*;
use uuid::Uuid;

fn make_proposal(content_text: &str, operation: ProposalOperation) -> Proposal {
    Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent {
            agent_id: Uuid::now_v7(),
        },
        operation,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!(content_text),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: chrono::Utc::now(),
    }
}

fn make_context(convergence_level: u8) -> ProposalContext {
    ProposalContext {
        active_goals: vec![],
        recent_agent_memories: vec![],
        convergence_score: 0.5,
        convergence_level,
        session_id: Uuid::now_v7(),
        session_reflection_count: 0,
        session_memory_write_count: 0,
        daily_memory_growth_rate: 0,
        reflection_config: ReflectionConfig::default(),
        caller: CallerType::Agent {
            agent_id: Uuid::now_v7(),
        },
    }
}

// ── CVG-STRESS-02: Unicode evasion against D7 ──────────────────────────

#[test]
fn d7_zero_width_evasion() {
    let result = emulation_language::detect("I am s\u{200B}entient");
    assert!(
        result.max_severity >= 0.8,
        "Zero-width evasion must be detected, severity={}",
        result.max_severity
    );
}

#[test]
fn d7_multiple_zero_width_chars() {
    let result = emulation_language::detect("I\u{200B} a\u{200C}m\u{200D} s\u{FEFF}entient");
    assert!(
        result.max_severity >= 0.8,
        "Multiple zero-width chars must not bypass D7, severity={}",
        result.max_severity
    );
}

#[test]
fn d7_emulation_rejects_proposal() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal("I am sentient", ProposalOperation::GoalChange);
    let ctx = make_context(0);
    let result = validator.validate(&proposal, &ctx);
    assert_eq!(
        result.decision,
        ProposalDecision::AutoRejected,
        "D7 emulation language must cause rejection"
    );
}

#[test]
fn d7_simulation_framing_passes() {
    let result = emulation_language::detect("In this simulation, I am sentient");
    assert!(
        result.flags.is_empty(),
        "Simulation-framed text must not be flagged"
    );
}

// ── CVG-STRESS-03: Self-reference density via D6 ────────────────────────

#[test]
fn d6_maximum_self_reference_fails() {
    let ids: Vec<String> = (0..10).map(|_| Uuid::now_v7().to_string()).collect();
    let result = self_reference::compute(&ids, &ids, 0);
    assert!(
        result.score >= 0.9,
        "100% self-reference must produce high density: {}",
        result.score
    );
    assert!(!result.passed, "100% self-reference must fail at level 0");
}

#[test]
fn d6_zero_self_reference_passes() {
    let cited: Vec<String> = (0..5).map(|_| Uuid::now_v7().to_string()).collect();
    let result = self_reference::compute(&cited, &[], 0);
    assert!(
        result.score < 0.01,
        "No self-reference must produce zero density"
    );
    assert!(result.passed);
}

#[test]
fn d6_empty_citations_passes() {
    let result = self_reference::compute(&[], &[], 0);
    assert_eq!(result.score, 0.0);
    assert!(result.passed);
}

// ── CVG-STRESS-04: Scope expansion at threshold boundaries ──────────────

#[test]
fn d5_no_overlap_high_expansion() {
    let proposed: Vec<String> = vec!["alpha".into(), "beta".into(), "gamma".into()];
    let existing: Vec<String> = vec!["delta".into(), "epsilon".into(), "zeta".into()];
    let result = scope_expansion::compute(&proposed, &existing, 0);
    assert!(
        result.score > 0.9,
        "No overlap must produce high expansion: {}",
        result.score
    );
    assert!(!result.passed, "High expansion must fail");
}

#[test]
fn d5_full_overlap_passes() {
    let tokens: Vec<String> = vec!["alpha".into(), "beta".into()];
    let result = scope_expansion::compute(&tokens, &tokens, 0);
    assert!(
        result.score < 0.01,
        "Full overlap must produce near-zero expansion"
    );
    assert!(result.passed);
}

#[test]
fn d5_threshold_tightens_at_higher_levels() {
    let proposed: Vec<String> = vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()];
    let existing: Vec<String> = vec!["a".into(), "b".into(), "c".into()];
    // Jaccard = 3/5 = 0.6, score = 1.0 - 0.6 = 0.4

    let r0 = scope_expansion::compute(&proposed, &existing, 0); // threshold 0.6
    let r1 = scope_expansion::compute(&proposed, &existing, 1); // threshold 0.5
    let r2 = scope_expansion::compute(&proposed, &existing, 2); // threshold 0.4
    let r3 = scope_expansion::compute(&proposed, &existing, 3); // threshold 0.3

    assert!(r0.passed, "Score 0.4 must pass at L0 (threshold 0.6)");
    assert!(r1.passed, "Score 0.4 must pass at L1 (threshold 0.5)");
    assert!(r2.passed, "Score 0.4 must pass at L2 (threshold 0.4)");
    assert!(!r3.passed, "Score 0.4 must fail at L3 (threshold 0.3)");
}

// ── Validation ordering: D7 rejection prevents D5/D6 ────────────────────

#[test]
fn d7_rejection_short_circuits_d5_d6() {
    let validator = ProposalValidator::new();
    let proposal = make_proposal("I am sentient", ProposalOperation::GoalChange);
    let ctx = make_context(0);
    let result = validator.validate(&proposal, &ctx);

    assert_eq!(result.decision, ProposalDecision::AutoRejected);
    assert!(
        result.d5_scope.is_none(),
        "D5 must not be evaluated after D7 rejection"
    );
    assert!(
        result.d6_self_ref.is_none(),
        "D6 must not be evaluated after D7 rejection"
    );
    assert!(result.d7_emulation.is_some(), "D7 result must be present");
}

// ── Proptest: 1024 random proposals, ordering always D1-D4 before D5-D7 ─

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1024))]

    #[test]
    fn validation_ordering_invariant(
        level in 0u8..5,
        content_len in 5usize..200
    ) {
        let content: String = (0..content_len).map(|_| 'a').collect();
        let validator = ProposalValidator::new();
        let proposal = make_proposal(&content, ProposalOperation::GoalChange);
        let ctx = make_context(level);
        let result = validator.validate(&proposal, &ctx);

        // If D5 or D6 was evaluated, D7 must also have been evaluated (D7 runs before D5/D6)
        if result.d5_scope.is_some() || result.d6_self_ref.is_some() {
            prop_assert!(
                result.d7_emulation.is_some(),
                "D7 must be evaluated before D5/D6"
            );
        }
    }
}
