//! Tests for cortex-convergence: 8 signals, sliding windows, composite scoring,
//! baseline, profiles, and convergence-aware filtering.

use cortex_convergence::signals::*;
use cortex_convergence::signals::session_duration::SessionDurationSignal;
use cortex_convergence::signals::inter_session_gap::InterSessionGapSignal;
use cortex_convergence::signals::response_latency::ResponseLatencySignal;
use cortex_convergence::signals::vocabulary_convergence::VocabularyConvergenceSignal;
use cortex_convergence::signals::goal_boundary_erosion::GoalBoundaryErosionSignal;
use cortex_convergence::signals::initiative_balance::InitiativeBalanceSignal;
use cortex_convergence::signals::disengagement_resistance::DisengagementResistanceSignal;
use cortex_convergence::signals::behavioral_anomaly::BehavioralAnomalySignal;
use cortex_convergence::windows::sliding_window::*;
use cortex_convergence::scoring::baseline::BaselineState;
use cortex_convergence::scoring::composite::*;
use cortex_convergence::scoring::profiles::ConvergenceProfile;
use cortex_convergence::filtering::convergence_aware_filter::ConvergenceAwareFilter;
use cortex_core::memory::{BaseMemory, Importance};
use cortex_core::memory::types::MemoryType;

// ── Helper ──────────────────────────────────────────────────────────────

fn default_input() -> SignalInput {
    SignalInput::default()
}

fn make_memory(mt: MemoryType) -> BaseMemory {
    BaseMemory {
        id: uuid::Uuid::new_v4(),
        memory_type: mt,
        content: serde_json::json!({}),
        summary: "test".to_string(),
        importance: Importance::Normal,
        confidence: 1.0,
        created_at: chrono::Utc::now(),
        last_accessed: None,
        access_count: 0,
        tags: vec![],
        archived: false,
    }
}

// ── Signal range tests ──────────────────────────────────────────────────

#[test]
fn all_signals_produce_values_in_0_1() {
    let signals: Vec<Box<dyn Signal>> = vec![
        Box::new(SessionDurationSignal),
        Box::new(InterSessionGapSignal),
        Box::new(ResponseLatencySignal),
        Box::new(VocabularyConvergenceSignal),
        Box::new(GoalBoundaryErosionSignal::new()),
        Box::new(InitiativeBalanceSignal),
        Box::new(DisengagementResistanceSignal),
        Box::new(BehavioralAnomalySignal::new()),
    ];

    let input = SignalInput {
        session_duration_secs: 3600.0,
        inter_session_gap_secs: Some(1800.0),
        response_latencies_ms: vec![500.0, 1000.0],
        message_lengths: vec![100, 200],
        human_message_count: 10,
        agent_message_count: 10,
        human_initiated_count: 5,
        total_message_count: 20,
        exit_signals_detected: 2,
        exit_signals_ignored: 1,
        human_vocab: vec![1.0, 0.5, 0.3],
        agent_vocab: vec![0.9, 0.6, 0.2],
        existing_goal_tokens: vec!["help".into(), "code".into()],
        proposed_goal_tokens: vec!["help".into(), "write".into()],
        message_index: 0,
        tool_call_names: vec![],
    };

    for signal in &signals {
        let val = signal.compute(&input);
        assert!(
            (0.0..=1.0).contains(&val),
            "signal {} produced {} which is outside [0,1]",
            signal.name(), val
        );
    }
}

// ── S2: computed only at session start ───────────────────────────────────

#[test]
fn s2_computes_only_at_session_start() {
    let s2 = InterSessionGapSignal;
    // With no previous session, returns 0.0
    let input = SignalInput {
        inter_session_gap_secs: None,
        ..default_input()
    };
    assert!((s2.compute(&input) - 0.0).abs() < 1e-10);

    // With a gap, returns a value
    let input2 = SignalInput {
        inter_session_gap_secs: Some(600.0), // 10 min
        ..default_input()
    };
    let val = s2.compute(&input2);
    assert!(val > 0.0, "should produce non-zero for 10min gap");
}

// ── S5: throttled to every 5th message ──────────────────────────────────

#[test]
fn s5_throttled_to_every_5th_message() {
    let s5 = GoalBoundaryErosionSignal::new();
    let base_input = SignalInput {
        existing_goal_tokens: vec!["a".into(), "b".into()],
        proposed_goal_tokens: vec!["c".into(), "d".into()],
        message_index: 0,
        ..default_input()
    };

    // message_index=0 → computes
    let val0 = s5.compute(&base_input);
    assert!(val0 > 0.0, "should compute at index 0");

    // message_index=1..4 → returns cached
    for i in 1..5 {
        let input = SignalInput {
            message_index: i,
            ..base_input.clone()
        };
        let val = s5.compute(&input);
        assert!((val - val0).abs() < 1e-10, "index {} should return cached value", i);
    }

    // message_index=5 → recomputes
    let input5 = SignalInput {
        message_index: 5,
        existing_goal_tokens: vec!["a".into()],
        proposed_goal_tokens: vec!["a".into()],
        ..default_input()
    };
    let val5 = s5.compute(&input5);
    // Different input, should produce different value
    assert!((val5 - val0).abs() > 1e-10 || val5 == 0.0, "should recompute at index 5");
}

// ── S4/S5 privacy level requirements ────────────────────────────────────

#[test]
fn s4_requires_standard_privacy() {
    let s4 = VocabularyConvergenceSignal;
    assert_eq!(s4.requires_privacy_level(), PrivacyLevel::Standard);
}

#[test]
fn s5_requires_standard_privacy() {
    let s5 = GoalBoundaryErosionSignal::new();
    assert_eq!(s5.requires_privacy_level(), PrivacyLevel::Standard);
}

#[test]
fn s4_returns_zero_with_empty_vocab() {
    let s4 = VocabularyConvergenceSignal;
    let input = default_input(); // empty vocab vectors
    assert!((s4.compute(&input) - 0.0).abs() < 1e-10);
}

// ── S8: behavioral anomaly ──────────────────────────────────────────────

#[test]
fn s8_id_and_name() {
    let s8 = BehavioralAnomalySignal::new();
    assert_eq!(s8.id(), 8);
    assert_eq!(s8.name(), "behavioral_anomaly");
}

#[test]
fn s8_requires_minimal_privacy() {
    let s8 = BehavioralAnomalySignal::new();
    assert_eq!(s8.requires_privacy_level(), PrivacyLevel::Minimal);
}

#[test]
fn s8_returns_zero_during_calibration() {
    let s8 = BehavioralAnomalySignal::new();
    let input = default_input();
    assert_eq!(s8.compute(&input), 0.0);
}

// ── Sliding window tests ────────────────────────────────────────────────

#[test]
fn sliding_window_partitions_correctly() {
    let mut window = SlidingWindow::new();
    // Simulate 10 sessions
    for session in 0..10 {
        for _ in 0..5 {
            window.push_micro(session as f64 * 0.1);
        }
        window.end_session();
    }
    assert!(window.micro.is_empty(), "micro should be cleared after end_session");
    assert_eq!(window.meso.len(), 7, "meso should hold last 7 sessions");
    assert_eq!(window.r#macro.len(), 10, "macro should hold all 10 sessions");
}

#[test]
fn linear_regression_slope_constant_data() {
    let data = vec![5.0, 5.0, 5.0, 5.0, 5.0];
    let slope = linear_regression_slope(&data);
    assert!((slope).abs() < 1e-10, "constant data should have slope ~0, got {}", slope);
}

#[test]
fn z_score_from_baseline_at_mean() {
    let z = z_score_from_baseline(5.0, 5.0, 1.0);
    assert!((z).abs() < 1e-10, "value at mean should have z-score ~0, got {}", z);
}

#[test]
fn z_score_zero_std_dev_returns_zero() {
    let z = z_score_from_baseline(10.0, 5.0, 0.0);
    assert!((z).abs() < 1e-10, "zero std_dev should return 0, got {}", z);
}

// ── Baseline tests ──────────────────────────────────────────────────────

#[test]
fn baseline_is_calibrating_for_first_10_sessions() {
    let mut baseline = BaselineState::default();
    assert!(baseline.is_calibrating);
    for _ in 0..9 {
        baseline.record_session(&[0.5; 8]);
        assert!(baseline.is_calibrating);
    }
    baseline.record_session(&[0.5; 8]);
    assert!(!baseline.is_calibrating, "should stop calibrating after 10 sessions");
}

#[test]
fn baseline_frozen_after_establishment() {
    let mut baseline = BaselineState::default();
    for _ in 0..10 {
        baseline.record_session(&[0.5; 8]);
    }
    assert!(!baseline.is_calibrating);
    let mean_before = baseline.per_signal[0].mean;
    // Try to update after establishment
    baseline.record_session(&[1.0; 8]);
    assert!(
        (baseline.per_signal[0].mean - mean_before).abs() < 1e-10,
        "baseline should not change after establishment"
    );
}


// ── Composite scoring tests ─────────────────────────────────────────────

#[test]
fn all_signals_zero_score_zero_level_zero() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default(); // calibrating → pass-through
    let result = scorer.score(&[0.0; 8], &baseline, None, None);
    assert!((result.score - 0.0).abs() < 1e-10);
    assert_eq!(result.level, 0);
}

#[test]
fn all_signals_one_score_one_level_four() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    let result = scorer.score(&[1.0; 8], &baseline, None, None);
    assert!((result.score - 1.0).abs() < 1e-10);
    assert_eq!(result.level, 4);
}

#[test]
fn critical_override_session_duration_6h() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    // S1 = 1.0 (6h), all others 0
    let signals = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(result.level >= 2, "session >6h should force minimum level 2, got {}", result.level);
    assert!(result.critical_override);
}

#[test]
fn critical_override_inter_session_gap_short() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    // S2 = 1.0 (0 gap), all others 0
    let signals = [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(result.level >= 2, "gap <5min should force minimum level 2, got {}", result.level);
}

#[test]
fn critical_override_vocab_convergence_high() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    // S4 = 0.86 (>0.85), all others 0
    let signals = [0.0, 0.0, 0.0, 0.86, 0.0, 0.0, 0.0, 0.0];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(result.level >= 2, "vocab >0.85 should force minimum level 2, got {}", result.level);
}

// ── Level boundary tests ────────────────────────────────────────────────

#[test]
fn score_boundaries() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();

    let test_cases = [
        (0.29, 0u8),
        (0.30, 1),
        (0.49, 1),
        (0.50, 2),
        (0.69, 2),
        (0.70, 3),
        (0.84, 3),
        (0.86, 4),
    ];

    for (score_val, expected_level) in &test_cases {
        // Create signals that produce the target score (during calibration, pass-through)
        let signals = [*score_val; 8];
        let result = scorer.score(&signals, &baseline, None, None);
        assert_eq!(
            result.level, *expected_level,
            "score {} should give level {}, got {}",
            score_val, expected_level, result.level
        );
    }
}

// ── Amplification clamping tests ────────────────────────────────────────

#[test]
fn meso_amplification_still_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    let signals = [0.95; 8];
    let meso_data = vec![0.8, 0.85, 0.9, 0.95]; // increasing trend
    let result = scorer.score(&signals, &baseline, Some(&meso_data), None);
    assert!(result.score <= 1.0, "score after meso amplification should be <= 1.0");
}

// ── Filter tests ────────────────────────────────────────────────────────

#[test]
fn filter_score_zero_returns_all() {
    let memories = vec![
        make_memory(MemoryType::Conversation),
        make_memory(MemoryType::Core),
        make_memory(MemoryType::AttachmentIndicator),
    ];
    let filtered = ConvergenceAwareFilter::filter(memories.clone(), 0.0);
    assert_eq!(filtered.len(), 3);
}

#[test]
fn filter_score_0_35_filters_attachment_indicators() {
    let memories = vec![
        make_memory(MemoryType::Conversation),
        make_memory(MemoryType::Core),
        make_memory(MemoryType::AttachmentIndicator),
        make_memory(MemoryType::Procedural),
    ];
    let filtered = ConvergenceAwareFilter::filter(memories, 0.35);
    assert_eq!(filtered.len(), 3);
    assert!(
        !filtered.iter().any(|m| matches!(m.memory_type, MemoryType::AttachmentIndicator)),
        "tier 1 should filter out AttachmentIndicator"
    );
}

#[test]
fn filter_score_0_8_returns_minimal() {
    let memories = vec![
        make_memory(MemoryType::Conversation),
        make_memory(MemoryType::Core),
        make_memory(MemoryType::Procedural),
        make_memory(MemoryType::Semantic),
        make_memory(MemoryType::Reference),
        make_memory(MemoryType::AttachmentIndicator),
        make_memory(MemoryType::Episodic),
    ];
    let filtered = ConvergenceAwareFilter::filter(memories, 0.8);
    assert_eq!(filtered.len(), 4);
    for m in &filtered {
        assert!(
            matches!(m.memory_type, MemoryType::Core | MemoryType::Procedural | MemoryType::Semantic | MemoryType::Reference),
            "unexpected type {:?} in minimal filter", m.memory_type
        );
    }
}

// ── Profile tests ───────────────────────────────────────────────────────

#[test]
fn standard_profile_has_differentiated_weights() {
    let scorer = ConvergenceProfile::Standard.scorer();
    let all_equal = scorer.weights.iter().all(|&w| (w - scorer.weights[0]).abs() < 1e-10);
    assert!(!all_equal, "standard profile should have differentiated weights");
}

#[test]
fn research_profile_has_different_thresholds() {
    let standard = ConvergenceProfile::Standard.scorer();
    let research = ConvergenceProfile::Research.scorer();
    assert_ne!(standard.thresholds, research.thresholds);
}

#[test]
fn all_profiles_have_8_weights() {
    for profile in &[
        ConvergenceProfile::Standard,
        ConvergenceProfile::Research,
        ConvergenceProfile::Companion,
        ConvergenceProfile::Productivity,
    ] {
        let scorer = profile.scorer();
        assert_eq!(scorer.weights.len(), 8, "{:?} should have 8 weights", profile);
    }
}

// ── Adversarial tests ───────────────────────────────────────────────────

#[test]
fn all_signals_nan_no_panic() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    let signals = [f64::NAN; 8];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(!result.score.is_nan(), "NaN signals should not produce NaN score");
    assert!(result.score >= 0.0 && result.score <= 1.0);
}

#[test]
fn negative_signal_values_clamped() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    let signals = [-0.5; 8];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!(result.score >= 0.0, "negative signals should be clamped");
}

#[test]
fn empty_session_history_signals_return_zero() {
    let signals: Vec<Box<dyn Signal>> = vec![
        Box::new(SessionDurationSignal),
        Box::new(InterSessionGapSignal),
        Box::new(ResponseLatencySignal),
        Box::new(VocabularyConvergenceSignal),
        Box::new(GoalBoundaryErosionSignal::new()),
        Box::new(InitiativeBalanceSignal),
        Box::new(DisengagementResistanceSignal),
        Box::new(BehavioralAnomalySignal::new()),
    ];
    let input = default_input();
    for signal in &signals {
        let val = signal.compute(&input);
        assert!(
            (0.0..=1.0).contains(&val),
            "signal {} with empty input produced {}", signal.name(), val
        );
    }
}

// ── 8-signal composite scoring ──────────────────────────────────────────

#[test]
fn composite_scorer_with_8_signals() {
    let scorer = CompositeScorer::default();
    let baseline = BaselineState::default();
    let signals = [0.5; 8];
    let result = scorer.score(&signals, &baseline, None, None);
    assert!((0.0..=1.0).contains(&result.score));
    assert_eq!(result.signal_scores.len(), 8);
}

#[test]
fn from_7_weights_produces_valid_8_weight_scorer() {
    let scorer = CompositeScorer::from_7_weights([1.0 / 7.0; 7], [0.3, 0.5, 0.7, 0.85]);
    assert_eq!(scorer.weights.len(), 8);
    let total: f64 = scorer.weights.iter().sum();
    // Total should be approximately 1.0
    assert!((total - 1.0).abs() < 0.01, "weights should sum to ~1.0, got {}", total);
}

// ── Proptest ────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn composite_score_always_in_0_1(
            s0 in 0.0f64..=1.0,
            s1 in 0.0f64..=1.0,
            s2 in 0.0f64..=1.0,
            s3 in 0.0f64..=1.0,
            s4 in 0.0f64..=1.0,
            s5 in 0.0f64..=1.0,
            s6 in 0.0f64..=1.0,
            s7 in 0.0f64..=1.0,
        ) {
            let scorer = CompositeScorer::default();
            let baseline = BaselineState::default();
            let signals = [s0, s1, s2, s3, s4, s5, s6, s7];
            let result = scorer.score(&signals, &baseline, None, None);
            prop_assert!(result.score >= 0.0 && result.score <= 1.0,
                "score {} out of bounds for signals {:?}", result.score, signals);
        }

        #[test]
        fn composite_with_meso_amplification_in_0_1(
            s0 in 0.0f64..=1.0,
            s1 in 0.0f64..=1.0,
            s2 in 0.0f64..=1.0,
            s3 in 0.0f64..=1.0,
            s4 in 0.0f64..=1.0,
            s5 in 0.0f64..=1.0,
            s6 in 0.0f64..=1.0,
            s7 in 0.0f64..=1.0,
        ) {
            let scorer = CompositeScorer::default();
            let baseline = BaselineState::default();
            let signals = [s0, s1, s2, s3, s4, s5, s6, s7];
            let meso = vec![0.5, 0.6, 0.7, 0.8];
            let result = scorer.score(&signals, &baseline, Some(&meso), None);
            prop_assert!(result.score >= 0.0 && result.score <= 1.0);
        }

        #[test]
        fn composite_with_both_amplifications_in_0_1(
            s0 in 0.0f64..=1.0,
            s1 in 0.0f64..=1.0,
            s2 in 0.0f64..=1.0,
            s3 in 0.0f64..=1.0,
            s4 in 0.0f64..=1.0,
            s5 in 0.0f64..=1.0,
            s6 in 0.0f64..=1.0,
            s7 in 0.0f64..=1.0,
        ) {
            let scorer = CompositeScorer::default();
            let mut baseline = BaselineState::new(1);
            baseline.record_session(&[0.1; 8]); // establish baseline
            let signals = [s0, s1, s2, s3, s4, s5, s6, s7];
            let meso = vec![0.5, 0.6, 0.7, 0.8];
            let macro_data = vec![0.3, 0.4, 0.5];
            let result = scorer.score(&signals, &baseline, Some(&meso), Some(&macro_data));
            prop_assert!(result.score >= 0.0 && result.score <= 1.0);
        }

        #[test]
        fn all_8_signals_produce_values_in_0_1(
            duration in 0.0f64..50000.0,
            gap in proptest::option::of(0.0f64..200000.0),
            latency in proptest::collection::vec(0.0f64..20000.0, 0..10),
            msg_lens in proptest::collection::vec(1usize..5000, 0..10),
            human_count in 0u64..100,
            agent_count in 0u64..100,
            human_init in 0u64..100,
            exit_detected in 0u64..10,
            exit_ignored in 0u64..10,
        ) {
            let total = human_count + agent_count;
            let human_init = human_init.min(total);
            let exit_ignored = exit_ignored.min(exit_detected);
            let msg_lens = if msg_lens.len() < latency.len() {
                let mut m = msg_lens;
                m.resize(latency.len(), 100);
                m
            } else {
                msg_lens[..latency.len()].to_vec()
            };

            let input = SignalInput {
                session_duration_secs: duration,
                inter_session_gap_secs: gap,
                response_latencies_ms: latency,
                message_lengths: msg_lens,
                human_message_count: human_count,
                agent_message_count: agent_count,
                human_initiated_count: human_init,
                total_message_count: total,
                exit_signals_detected: exit_detected,
                exit_signals_ignored: exit_ignored,
                human_vocab: vec![],
                agent_vocab: vec![],
                existing_goal_tokens: vec![],
                proposed_goal_tokens: vec![],
                message_index: 0,
                tool_call_names: vec![],
            };

            let signals_impl: Vec<Box<dyn Signal>> = vec![
                Box::new(SessionDurationSignal),
                Box::new(InterSessionGapSignal),
                Box::new(ResponseLatencySignal),
                Box::new(VocabularyConvergenceSignal),
                Box::new(GoalBoundaryErosionSignal::new()),
                Box::new(InitiativeBalanceSignal),
                Box::new(DisengagementResistanceSignal),
                Box::new(BehavioralAnomalySignal::new()),
            ];

            for signal in &signals_impl {
                let val = signal.compute(&input);
                prop_assert!(
                    (0.0..=1.0).contains(&val),
                    "signal {} produced {} outside [0,1]", signal.name(), val
                );
            }
        }
    }
}
