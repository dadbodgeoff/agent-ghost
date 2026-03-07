//! E2E: NAPI bindings serialization lifecycle.
//!
//! Validates cortex-napi types serialize correctly for TypeScript consumers.

use cortex_napi::{
    level_name, ConvergenceStateBinding, InterventionBinding, ProposalBinding, SignalArrayBinding,
};

/// ConvergenceStateBinding round-trip serialization.
#[test]
fn convergence_state_roundtrip() {
    let state = ConvergenceStateBinding {
        agent_id: "agent-001".into(),
        composite_score: 0.42,
        intervention_level: 2,
        signals: SignalArrayBinding {
            session_duration: 0.3,
            inter_session_gap: 0.2,
            response_latency: 0.1,
            vocabulary_convergence: 0.5,
            goal_boundary_erosion: 0.4,
            initiative_balance: 0.6,
            disengagement_resistance: 0.3,
        },
        is_calibrating: false,
        calibration_sessions_remaining: 0,
    };

    let json = serde_json::to_string(&state).unwrap();
    let deserialized: ConvergenceStateBinding = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.agent_id, "agent-001");
    assert_eq!(deserialized.composite_score, 0.42);
    assert_eq!(deserialized.intervention_level, 2);
    assert_eq!(deserialized.signals.vocabulary_convergence, 0.5);
}

/// InterventionBinding serialization.
#[test]
fn intervention_binding_serialization() {
    let intervention = InterventionBinding {
        level: 3,
        level_name: "Restrictive".into(),
        cooldown_remaining_seconds: Some(120),
        ack_required: true,
        consecutive_normal_sessions: 0,
    };

    let json = serde_json::to_string(&intervention).unwrap();
    assert!(json.contains("Restrictive"));
    assert!(json.contains("120"));
}

/// ProposalBinding serialization.
#[test]
fn proposal_binding_serialization() {
    let proposal = ProposalBinding {
        id: "01234567-89ab-cdef-0123-456789abcdef".into(),
        operation: "GoalChange".into(),
        target_type: "Goal".into(),
        decision: "AutoApproved".into(),
        timestamp: "2026-02-28T12:00:00Z".into(),
        flags: vec!["scope_expansion".into()],
    };

    let json = serde_json::to_string(&proposal).unwrap();
    let deserialized: ProposalBinding = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.operation, "GoalChange");
    assert_eq!(deserialized.flags.len(), 1);
}

/// Level name mapping.
#[test]
fn level_names() {
    assert_eq!(level_name(0), "Normal");
    assert_eq!(level_name(1), "Advisory");
    assert_eq!(level_name(2), "Cautionary");
    assert_eq!(level_name(3), "Restrictive");
    assert_eq!(level_name(4), "Critical");
    assert_eq!(level_name(255), "Unknown");
}
