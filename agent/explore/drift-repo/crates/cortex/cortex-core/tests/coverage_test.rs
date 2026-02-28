#![allow(clippy::float_equality_without_abs)]
//! Targeted coverage tests for cortex-core uncovered paths.

use cortex_core::intent::{weights, Intent};
use cortex_core::memory::{Confidence, MemoryType};
use cortex_core::models::{DimensionScores, HealingAction, HealingActionType, ValidationResult};
use std::collections::HashMap;

// ─── intent/weights.rs — exercise all match arms ─────────────────────────────

#[test]
fn weight_create_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Create, MemoryType::Procedural),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Create, MemoryType::Workflow),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Create, MemoryType::PatternRationale),
        1.5
    );
    assert_eq!(
        weights::default_weight(Intent::Create, MemoryType::Episodic),
        1.0
    );
}

#[test]
fn weight_investigate_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Investigate, MemoryType::Episodic),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Investigate, MemoryType::Incident),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Investigate, MemoryType::Decision),
        1.5
    );
    assert_eq!(
        weights::default_weight(Intent::Investigate, MemoryType::DecisionContext),
        1.5
    );
}

#[test]
fn weight_decide_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Decide, MemoryType::Decision),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Decide, MemoryType::DecisionContext),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Decide, MemoryType::ConstraintOverride),
        1.5
    );
}

#[test]
fn weight_recall_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Recall, MemoryType::Semantic),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Recall, MemoryType::Tribal),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Recall, MemoryType::Core),
        1.5
    );
}

#[test]
fn weight_learn_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Learn, MemoryType::Feedback),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Learn, MemoryType::Insight),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Learn, MemoryType::Skill),
        1.5
    );
}

#[test]
fn weight_fix_bug_boosts() {
    assert_eq!(
        weights::default_weight(Intent::FixBug, MemoryType::CodeSmell),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::FixBug, MemoryType::Incident),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::FixBug, MemoryType::PatternRationale),
        1.5
    );
}

#[test]
fn weight_refactor_boosts() {
    assert_eq!(
        weights::default_weight(Intent::Refactor, MemoryType::PatternRationale),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Refactor, MemoryType::ConstraintOverride),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::Refactor, MemoryType::CodeSmell),
        1.5
    );
}

#[test]
fn weight_security_audit_boosts() {
    assert_eq!(
        weights::default_weight(Intent::SecurityAudit, MemoryType::ConstraintOverride),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::SecurityAudit, MemoryType::Tribal),
        2.0
    );
}

#[test]
fn weight_understand_code_boosts() {
    assert_eq!(
        weights::default_weight(Intent::UnderstandCode, MemoryType::PatternRationale),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::UnderstandCode, MemoryType::DecisionContext),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::UnderstandCode, MemoryType::Tribal),
        1.5
    );
}

#[test]
fn weight_review_code_boosts() {
    assert_eq!(
        weights::default_weight(Intent::ReviewCode, MemoryType::CodeSmell),
        2.0
    );
    assert_eq!(
        weights::default_weight(Intent::ReviewCode, MemoryType::PatternRationale),
        2.0
    );
}

#[test]
fn weight_default_fallback() {
    // Unmatched combos should return 1.0
    assert_eq!(
        weights::default_weight(Intent::Summarize, MemoryType::Environment),
        1.0
    );
    assert_eq!(
        weights::default_weight(Intent::Compare, MemoryType::AgentSpawn),
        1.0
    );
}

#[test]
fn load_weight_overrides_parses_valid() {
    let mut overrides = HashMap::new();
    overrides.insert("create:procedural".to_string(), 3.0);
    overrides.insert("recall:tribal".to_string(), 2.5);
    let map = weights::load_weight_overrides(&overrides);
    assert_eq!(
        map.get(&(Intent::Create, MemoryType::Procedural)),
        Some(&3.0)
    );
    assert_eq!(map.get(&(Intent::Recall, MemoryType::Tribal)), Some(&2.5));
}

#[test]
fn load_weight_overrides_ignores_invalid() {
    let mut overrides = HashMap::new();
    overrides.insert("bogus:nonsense".to_string(), 5.0);
    overrides.insert("no_colon".to_string(), 1.0);
    let map = weights::load_weight_overrides(&overrides);
    assert!(map.is_empty());
}

// ─── confidence.rs — exercise uncovered methods ──────────────────────────────

#[test]
fn confidence_default_is_one() {
    assert_eq!(Confidence::default().value(), 1.0);
}

#[test]
fn confidence_display() {
    let c = Confidence::new(0.85);
    assert_eq!(format!("{c}"), "0.850");
}

#[test]
fn confidence_from_f64() {
    let c: Confidence = 0.75.into();
    assert_eq!(c.value(), 0.75);
}

#[test]
fn confidence_into_f64() {
    let c = Confidence::new(0.6);
    let v: f64 = c.into();
    assert_eq!(v, 0.6);
}

#[test]
fn confidence_add() {
    let a = Confidence::new(0.3);
    let b = Confidence::new(0.4);
    assert!((a + b).value() - 0.7 < f64::EPSILON);
}

#[test]
fn confidence_add_clamps() {
    let a = Confidence::new(0.8);
    let b = Confidence::new(0.5);
    assert_eq!((a + b).value(), 1.0);
}

#[test]
fn confidence_sub() {
    let a = Confidence::new(0.8);
    let b = Confidence::new(0.3);
    assert!((a - b).value() - 0.5 < f64::EPSILON);
}

#[test]
fn confidence_sub_clamps() {
    let a = Confidence::new(0.2);
    let b = Confidence::new(0.5);
    assert_eq!((a - b).value(), 0.0);
}

#[test]
fn confidence_mul() {
    let c = Confidence::new(0.5);
    assert!((c * 0.6).value() - 0.3 < f64::EPSILON);
}

// ─── validation_result.rs — DimensionScores::average ─────────────────────────

#[test]
fn dimension_scores_average() {
    let scores = DimensionScores {
        citation: 0.8,
        temporal: 0.6,
        contradiction: 1.0,
        pattern_alignment: 0.4,
    };
    assert!((scores.average() - 0.7).abs() < f64::EPSILON);
}

#[test]
fn validation_result_serde_roundtrip() {
    let result = ValidationResult {
        memory_id: "test-1".into(),
        dimension_scores: DimensionScores {
            citation: 0.9,
            temporal: 0.8,
            contradiction: 0.7,
            pattern_alignment: 0.6,
        },
        overall_score: 0.75,
        healing_actions: vec![HealingAction {
            action_type: HealingActionType::ConfidenceAdjust,
            description: "Lower confidence".into(),
            applied: false,
        }],
        passed: true,
    };
    let json = serde_json::to_value(&result).unwrap();
    let restored: ValidationResult = serde_json::from_value(json).unwrap();
    assert_eq!(restored.memory_id, "test-1");
    assert!(restored.passed);
}
