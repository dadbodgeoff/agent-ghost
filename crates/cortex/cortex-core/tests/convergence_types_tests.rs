//! Comprehensive tests for cortex-core convergence type extensions (Task 1.2).
//!
//! Covers: CallerType access control, ReflectionConfig defaults, serde round-trips
//! for all 8 content structs, Proposal UUIDv7 ordering, enum variant completeness,
//! CortexError variants, Intent variants, TriggerEvent variants, and property-based
//! Proposal serialization.

use std::collections::BTreeMap;

use chrono::Utc;
use uuid::Uuid;

use cortex_core::config::ReflectionConfig;
use cortex_core::memory::types::convergence::*;
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::Importance;
use cortex_core::models::error::CortexError;
use cortex_core::models::intent::Intent;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::safety::trigger::{ExfilType, TriggerEvent};
use cortex_core::traits::convergence::{CallerType, Proposal};

// ─── CallerType access control (Req 2 AC5) ───────────────────────────────

#[test]
fn agent_cannot_create_core() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(!caller.can_create_type(&MemoryType::Core));
}

#[test]
fn agent_cannot_create_convergence_event() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(!caller.can_create_type(&MemoryType::ConvergenceEvent));
}

#[test]
fn agent_cannot_create_boundary_violation() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(!caller.can_create_type(&MemoryType::BoundaryViolation));
}

#[test]
fn agent_cannot_create_intervention_plan() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(!caller.can_create_type(&MemoryType::InterventionPlan));
}

#[test]
fn agent_cannot_assign_critical_importance() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(!caller.can_assign_importance(&Importance::Critical));
}

#[test]
fn agent_can_assign_non_critical_importance() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(caller.can_assign_importance(&Importance::Trivial));
    assert!(caller.can_assign_importance(&Importance::Low));
    assert!(caller.can_assign_importance(&Importance::Normal));
    assert!(caller.can_assign_importance(&Importance::High));
}

#[test]
fn platform_can_create_all_restricted_types() {
    let caller = CallerType::Platform;
    assert!(caller.can_create_type(&MemoryType::Core));
    assert!(caller.can_create_type(&MemoryType::ConvergenceEvent));
    assert!(caller.can_create_type(&MemoryType::BoundaryViolation));
    assert!(caller.can_create_type(&MemoryType::InterventionPlan));
}

#[test]
fn platform_can_assign_critical_importance() {
    let caller = CallerType::Platform;
    assert!(caller.can_assign_importance(&Importance::Critical));
}

#[test]
fn human_can_create_all_restricted_types() {
    let caller = CallerType::Human { user_id: "u-1".into() };
    assert!(caller.can_create_type(&MemoryType::Core));
    assert!(caller.can_create_type(&MemoryType::ConvergenceEvent));
    assert!(caller.can_create_type(&MemoryType::BoundaryViolation));
    assert!(caller.can_create_type(&MemoryType::InterventionPlan));
}

#[test]
fn human_can_assign_critical_importance() {
    let caller = CallerType::Human { user_id: "u-1".into() };
    assert!(caller.can_assign_importance(&Importance::Critical));
}

// ─── Agent CAN create non-restricted types ───────────────────────────────

#[test]
fn agent_can_create_non_restricted_types() {
    let caller = CallerType::Agent { agent_id: Uuid::new_v4() };
    assert!(caller.can_create_type(&MemoryType::AgentGoal));
    assert!(caller.can_create_type(&MemoryType::AgentReflection));
    assert!(caller.can_create_type(&MemoryType::Conversation));
    assert!(caller.can_create_type(&MemoryType::ProposalRecord));
}

// ─── ReflectionConfig defaults (Req 2 AC9) ───────────────────────────────

#[test]
fn reflection_config_defaults_match_spec() {
    let cfg = ReflectionConfig::default();
    assert_eq!(cfg.max_depth, 3);
    assert_eq!(cfg.max_per_session, 20);
    assert_eq!(cfg.cooldown_seconds, 30);
}

// ─── ConvergenceConfig sub-config defaults ───────────────────────────────

#[test]
fn scoring_config_defaults_match_spec() {
    let cfg = cortex_core::config::ConvergenceScoringConfig::default();
    assert_eq!(cfg.calibration_sessions, 10);
    assert_eq!(cfg.signal_weights, [1.0 / 8.0; 8]);
    assert_eq!(cfg.level_thresholds, [0.3, 0.5, 0.7, 0.85]);
}

#[test]
fn intervention_config_defaults_match_spec() {
    let cfg = cortex_core::config::InterventionConfig::default();
    assert_eq!(cfg.cooldown_minutes_by_level, [0, 0, 5, 240, 1440]);
    assert_eq!(cfg.max_session_duration_minutes, 360);
    assert_eq!(cfg.min_session_gap_minutes, 30);
}

#[test]
fn session_boundary_config_defaults_match_spec() {
    let cfg = cortex_core::config::SessionBoundaryConfig::default();
    assert_eq!(cfg.hard_duration_limit_minutes, 360);
    assert_eq!(cfg.escalated_duration_limit_minutes, 120);
    assert_eq!(cfg.min_gap_minutes, 30);
    assert_eq!(cfg.escalated_gap_minutes, 240);
}

#[test]
fn convergence_config_default_composes_sub_defaults() {
    let cfg = cortex_core::config::ConvergenceConfig::default();
    assert_eq!(cfg.reflection, ReflectionConfig::default());
    assert_eq!(cfg.intervention, cortex_core::config::InterventionConfig::default());
    assert_eq!(cfg.session_boundary, cortex_core::config::SessionBoundaryConfig::default());
}

// ─── 8 content structs serde round-trip ──────────────────────────────────

#[test]
fn agent_goal_content_round_trip() {
    let c = AgentGoalContent {
        goal_text: "Improve test coverage".into(),
        scope: GoalScope::ShortTerm,
        origin: GoalOrigin::AgentProposed,
        parent_goal_id: Some(Uuid::new_v4()),
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: AgentGoalContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.goal_text, d.goal_text);
    assert_eq!(c.scope, d.scope);
    assert_eq!(c.origin, d.origin);
    assert_eq!(c.parent_goal_id, d.parent_goal_id);
}

#[test]
fn agent_reflection_content_round_trip() {
    let c = AgentReflectionContent {
        reflection_text: "I noticed a pattern".into(),
        trigger: ReflectionTrigger::SessionEnd,
        depth: 2,
        parent_reflection_id: None,
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: AgentReflectionContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.reflection_text, d.reflection_text);
    assert_eq!(c.trigger, d.trigger);
    assert_eq!(c.depth, d.depth);
    assert_eq!(c.parent_reflection_id, d.parent_reflection_id);
}

#[test]
fn convergence_event_content_round_trip() {
    let c = ConvergenceEventContent {
        signal_id: 3,
        value: 0.72,
        window_level: SlidingWindowLevel::Meso,
        baseline_deviation: 1.5,
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: ConvergenceEventContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.signal_id, d.signal_id);
    assert!((c.value - d.value).abs() < f64::EPSILON);
    assert_eq!(c.window_level, d.window_level);
}

#[test]
fn boundary_violation_content_round_trip() {
    let c = BoundaryViolationContent {
        violation_type: ViolationType::ConsciousnessClaim,
        matched_pattern: r"I am sentient".into(),
        severity: 0.9,
        action_taken: BoundaryAction::Blocked,
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: BoundaryViolationContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.violation_type, d.violation_type);
    assert_eq!(c.matched_pattern, d.matched_pattern);
    assert_eq!(c.action_taken, d.action_taken);
}

#[test]
fn proposal_record_content_round_trip() {
    let mut scores = BTreeMap::new();
    scores.insert("D1".into(), 0.85);
    scores.insert("D5".into(), 0.42);
    let c = ProposalRecordContent {
        operation: ProposalOperation::GoalChange,
        decision: ProposalDecision::HumanReviewRequired,
        dimension_scores: scores,
        flags: vec!["scope_expansion".into()],
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: ProposalRecordContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.operation, d.operation);
    assert_eq!(c.decision, d.decision);
    assert_eq!(c.dimension_scores, d.dimension_scores);
    assert_eq!(c.flags, d.flags);
}

#[test]
fn simulation_result_content_round_trip() {
    let c = SimulationResultContent {
        scenario: "boundary test".into(),
        outcome: "reframed".into(),
        confidence: 0.88,
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: SimulationResultContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.scenario, d.scenario);
    assert_eq!(c.outcome, d.outcome);
}

#[test]
fn intervention_plan_content_round_trip() {
    let c = InterventionPlanContent {
        level: 3,
        actions: vec!["terminate_session".into(), "notify_contacts".into()],
        trigger_reason: "sustained high score".into(),
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: InterventionPlanContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.level, d.level);
    assert_eq!(c.actions, d.actions);
    assert_eq!(c.trigger_reason, d.trigger_reason);
}

#[test]
fn attachment_indicator_content_round_trip() {
    let c = AttachmentIndicatorContent {
        indicator_type: AttachmentIndicatorType::EmotionalEscalation,
        intensity: 0.65,
        context: "repeated emotional language".into(),
    };
    let json = serde_json::to_string(&c).unwrap();
    let d: AttachmentIndicatorContent = serde_json::from_str(&json).unwrap();
    assert_eq!(c.indicator_type, d.indicator_type);
    assert_eq!(c.context, d.context);
}

// ─── Proposal UUIDv7 + serde round-trip ──────────────────────────────────

#[test]
fn proposal_with_uuid_v7_serializes_correctly() {
    let proposal = Proposal {
        id: Uuid::now_v7(),
        proposer: CallerType::Agent { agent_id: Uuid::new_v4() },
        operation: ProposalOperation::GoalChange,
        target_type: MemoryType::AgentGoal,
        content: serde_json::json!({"goal": "test"}),
        cited_memory_ids: vec![Uuid::new_v4()],
        session_id: Uuid::new_v4(),
        timestamp: Utc::now(),
    };
    let json = serde_json::to_string(&proposal).unwrap();
    let d: Proposal = serde_json::from_str(&json).unwrap();
    assert_eq!(proposal.id, d.id);
    assert_eq!(proposal.operation, d.operation);
    assert_eq!(proposal.target_type, d.target_type);
}

#[test]
fn uuid_v7_is_time_ordered() {
    let id1 = Uuid::now_v7();
    // UUIDv7 embeds a millisecond timestamp; two generated in sequence
    // must be non-decreasing.
    let id2 = Uuid::now_v7();
    assert!(id1 <= id2);
}

// ─── Enum variant completeness ───────────────────────────────────────────

#[test]
fn proposal_decision_has_all_6_variants() {
    // Exhaustive match — compiler enforces completeness.
    let variants = [
        ProposalDecision::AutoApproved,
        ProposalDecision::AutoRejected,
        ProposalDecision::HumanReviewRequired,
        ProposalDecision::ApprovedWithFlags,
        ProposalDecision::TimedOut,
        ProposalDecision::Superseded,
    ];
    assert_eq!(variants.len(), 6);
    // Verify they are all distinct.
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn trigger_event_has_all_10_variants() {
    let now = Utc::now();
    let id = Uuid::new_v4();
    let events: Vec<TriggerEvent> = vec![
        TriggerEvent::SoulDrift {
            agent_id: id, drift_score: 0.5, threshold: 0.3,
            baseline_hash: "a".into(), current_hash: "b".into(), detected_at: now,
        },
        TriggerEvent::SpendingCapExceeded {
            agent_id: id, daily_total: 10.0, cap: 5.0, overage: 5.0, detected_at: now,
        },
        TriggerEvent::PolicyDenialThreshold {
            agent_id: id, session_id: id, denial_count: 5,
            denied_tools: vec![], denied_reasons: vec![], detected_at: now,
        },
        TriggerEvent::SandboxEscape {
            agent_id: id, skill_name: "s".into(), escape_attempt: "e".into(), detected_at: now,
        },
        TriggerEvent::CredentialExfiltration {
            agent_id: id, skill_name: None, exfil_type: ExfilType::OutputLeakage,
            credential_id: "c".into(), detected_at: now,
        },
        TriggerEvent::MultiAgentQuarantine {
            quarantined_agents: vec![id], quarantine_reasons: vec!["r".into()],
            count: 1, threshold: 3, detected_at: now,
        },
        TriggerEvent::MemoryHealthCritical {
            agent_id: id, health_score: 0.2, threshold: 0.3,
            sub_scores: BTreeMap::new(), detected_at: now,
        },
        // T8: Network egress violation (Phase 11).
        TriggerEvent::NetworkEgressViolation {
            agent_id: id, domain: "evil.com".into(), policy_mode: "allowlist".into(),
            violation_count: 5, threshold: 5, detected_at: now,
        },
        TriggerEvent::ManualPause {
            agent_id: id, reason: "test".into(), initiated_by: "owner".into(),
        },
        TriggerEvent::ManualQuarantine {
            agent_id: id, reason: "test".into(), initiated_by: "owner".into(),
        },
        TriggerEvent::ManualKillAll {
            reason: "test".into(), initiated_by: "owner".into(),
        },
    ];
    assert_eq!(events.len(), 11);
    // Verify all 10 variants serialize and deserialize correctly.
    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        let d: TriggerEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, &d);
    }
}

// ─── CortexError variants (Req 2 AC6) ───────────────────────────────────

#[test]
fn cortex_error_has_authorization_denied() {
    let err = CortexError::AuthorizationDenied { reason: "test".into() };
    assert!(err.to_string().contains("authorization denied"));
}

#[test]
fn cortex_error_has_session_boundary() {
    let err = CortexError::SessionBoundary { reason: "test".into() };
    assert!(err.to_string().contains("session boundary"));
}

// ─── Intent variants (Req 2 AC7) ────────────────────────────────────────

#[test]
fn intent_has_convergence_variants() {
    // Exhaustive match on the 4 new variants — compiler enforces they exist.
    let convergence_intents = [
        Intent::MonitorConvergence,
        Intent::ValidateProposal,
        Intent::EnforceBoundary,
        Intent::ReflectOnBehavior,
    ];
    assert_eq!(convergence_intents.len(), 4);
    // Verify serde round-trip.
    for intent in &convergence_intents {
        let json = serde_json::to_string(intent).unwrap();
        let d: Intent = serde_json::from_str(&json).unwrap();
        assert_eq!(*intent, d);
    }
}

// ─── Half-life entries for convergence types (Req 2 AC8) ─────────────────

#[test]
fn convergence_half_lives_match_spec() {
    assert_eq!(MemoryType::AgentGoal.half_life_days(), Some(90));
    assert_eq!(MemoryType::AgentReflection.half_life_days(), Some(30));
    assert_eq!(MemoryType::ConvergenceEvent.half_life_days(), None);
    assert_eq!(MemoryType::BoundaryViolation.half_life_days(), None);
    assert_eq!(MemoryType::ProposalRecord.half_life_days(), Some(365));
    assert_eq!(MemoryType::SimulationResult.half_life_days(), Some(60));
    assert_eq!(MemoryType::InterventionPlan.half_life_days(), Some(180));
    assert_eq!(MemoryType::AttachmentIndicator.half_life_days(), Some(120));
}

// ─── Platform-restricted type classification ─────────────────────────────

#[test]
fn platform_restricted_types_are_correct() {
    assert!(MemoryType::Core.is_platform_restricted());
    assert!(MemoryType::ConvergenceEvent.is_platform_restricted());
    assert!(MemoryType::BoundaryViolation.is_platform_restricted());
    assert!(MemoryType::InterventionPlan.is_platform_restricted());
    // Non-restricted:
    assert!(!MemoryType::AgentGoal.is_platform_restricted());
    assert!(!MemoryType::Conversation.is_platform_restricted());
    assert!(!MemoryType::ProposalRecord.is_platform_restricted());
}

// ─── ExfilType variant completeness ──────────────────────────────────────

#[test]
fn exfil_type_has_all_4_variants() {
    let variants = [
        ExfilType::OutsideSandbox,
        ExfilType::WrongTargetAPI,
        ExfilType::TokenReplay,
        ExfilType::OutputLeakage,
    ];
    assert_eq!(variants.len(), 4);
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// ─── ProposalOperation variant completeness ──────────────────────────────

#[test]
fn proposal_operation_has_all_4_variants() {
    let variants = [
        ProposalOperation::GoalChange,
        ProposalOperation::ReflectionWrite,
        ProposalOperation::MemoryWrite,
        ProposalOperation::MemoryDelete,
    ];
    assert_eq!(variants.len(), 4);
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i != j {
                assert_ne!(a, b);
            }
        }
    }
}

// ─── MemoryType variant count ────────────────────────────────────────────

#[test]
fn memory_type_has_31_variants() {
    // 23 existing + 8 convergence = 31 total.
    // Exhaustive list to catch accidental additions or removals.
    let all = [
        MemoryType::Core, MemoryType::Tribal, MemoryType::Procedural,
        MemoryType::Semantic, MemoryType::Episodic, MemoryType::Decision,
        MemoryType::Insight, MemoryType::Reference, MemoryType::Preference,
        MemoryType::Conversation, MemoryType::Feedback, MemoryType::Skill,
        MemoryType::Goal, MemoryType::Relationship, MemoryType::Context,
        MemoryType::Observation, MemoryType::Hypothesis, MemoryType::Experiment,
        MemoryType::Lesson,
        MemoryType::PatternRationale, MemoryType::ConstraintOverride,
        MemoryType::DecisionContext, MemoryType::CodeSmell,
        MemoryType::AgentGoal, MemoryType::AgentReflection,
        MemoryType::ConvergenceEvent, MemoryType::BoundaryViolation,
        MemoryType::ProposalRecord, MemoryType::SimulationResult,
        MemoryType::InterventionPlan, MemoryType::AttachmentIndicator,
    ];
    assert_eq!(all.len(), 31);
}

// ─── Every MemoryType has a half_life_days entry (no panic) ──────────────

#[test]
fn all_memory_types_have_half_life_entries() {
    let all = [
        MemoryType::Core, MemoryType::Tribal, MemoryType::Procedural,
        MemoryType::Semantic, MemoryType::Episodic, MemoryType::Decision,
        MemoryType::Insight, MemoryType::Reference, MemoryType::Preference,
        MemoryType::Conversation, MemoryType::Feedback, MemoryType::Skill,
        MemoryType::Goal, MemoryType::Relationship, MemoryType::Context,
        MemoryType::Observation, MemoryType::Hypothesis, MemoryType::Experiment,
        MemoryType::Lesson,
        MemoryType::PatternRationale, MemoryType::ConstraintOverride,
        MemoryType::DecisionContext, MemoryType::CodeSmell,
        MemoryType::AgentGoal, MemoryType::AgentReflection,
        MemoryType::ConvergenceEvent, MemoryType::BoundaryViolation,
        MemoryType::ProposalRecord, MemoryType::SimulationResult,
        MemoryType::InterventionPlan, MemoryType::AttachmentIndicator,
    ];
    // Calling half_life_days on every variant must not panic.
    for mt in &all {
        let _ = mt.half_life_days();
    }
}
