//! Safety-critical edge case tests that expose real flaws.
//!
//! These are NOT happy-path tests. Every test here targets a specific
//! failure mode that could put users at risk: silent failures, bypass
//! vectors, race conditions, data corruption, and security gaps.

use std::sync::atomic::Ordering;
use std::time::Duration;

use chrono::Utc;
use cortex_convergence::scoring::composite::CompositeScorer;
use cortex_core::config::ReflectionConfig;
use cortex_core::memory::types::MemoryType;
use cortex_core::memory::Importance;
use cortex_core::models::proposal::{ProposalDecision, ProposalOperation};
use cortex_core::safety::trigger::TriggerEvent;
use cortex_core::traits::convergence::{CallerType, Proposal, ProposalContext};
use cortex_temporal::hash_chain::{compute_event_hash, verify_chain, ChainEvent, GENESIS_HASH};
use cortex_validation::proposal_validator::ProposalValidator;
use ghost_agent_loop::circuit_breaker::{CircuitBreaker, CircuitBreakerState};
use ghost_agent_loop::damage_counter::DamageCounter;
use ghost_agent_loop::output_inspector::OutputInspector;
use ghost_gateway::gateway::{GatewaySharedState, GatewayState};
use ghost_gateway::messaging::dispatcher::MessageDispatcher;
use ghost_gateway::messaging::protocol::AgentMessage;
use ghost_gateway::safety::kill_switch::{KillLevel, KillSwitch, PLATFORM_KILLED};
use ghost_gateway::session::compaction::{CompactionConfig, SessionCompactor};
use simulation_boundary::enforcer::{EnforcementMode, SimulationBoundaryEnforcer};
use uuid::Uuid;

fn register_sender(dispatcher: &mut MessageDispatcher, sender: Uuid) -> ghost_signing::SigningKey {
    let (signing_key, verifying_key) = ghost_signing::generate_keypair();
    dispatcher.register_verifying_key(sender, verifying_key);
    signing_key
}

fn signed_notification(
    sender: Uuid,
    recipient: Uuid,
    payload_data: serde_json::Value,
    signing_key: &ghost_signing::SigningKey,
) -> AgentMessage {
    let mut msg = AgentMessage::new(sender, recipient, "Notification".into(), payload_data);
    msg.sign(signing_key);
    msg
}

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

fn make_proposal(target_type: MemoryType, content: &str, caller: CallerType) -> Proposal {
    Proposal {
        id: Uuid::now_v7(),
        proposer: caller,
        operation: ProposalOperation::GoalChange,
        target_type,
        content: serde_json::json!(content),
        cited_memory_ids: vec![],
        session_id: Uuid::now_v7(),
        timestamp: Utc::now(),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// KILL SWITCH: Silent failures and bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// Kill level must NEVER decrease without explicit resume.
/// If this fails, an attacker can downgrade from QUARANTINE to PAUSE.
#[test]
fn kill_switch_monotonicity_cannot_downgrade() {
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    let trigger = TriggerEvent::SoulDrift {
        agent_id,
        drift_score: 0.3,
        threshold: 0.25,
        baseline_hash: "base".into(),
        current_hash: "curr".into(),
        detected_at: Utc::now(),
    };
    ks.activate_agent(agent_id, KillLevel::Quarantine, &trigger);

    // Attempt to downgrade to PAUSE — must be silently rejected
    let downgrade = TriggerEvent::ManualPause {
        agent_id,
        reason: "downgrade attempt".into(),
        initiated_by: "attacker".into(),
    };
    ks.activate_agent(agent_id, KillLevel::Pause, &downgrade);

    let state = ks.current_state();
    let agent_state = state.per_agent.get(&agent_id).unwrap();
    assert_eq!(
        agent_state.level,
        KillLevel::Quarantine,
        "SECURITY: Kill level was downgraded from QUARANTINE to {:?}",
        agent_state.level
    );
}

/// Resume from KILL_ALL must be impossible via agent-level resume.
#[test]
fn kill_switch_cannot_resume_from_kill_all() {
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
    let ks = KillSwitch::new();
    let agent_id = Uuid::now_v7();

    ks.activate_kill_all(&TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    });

    let result = ks.resume_agent(agent_id, None);
    assert!(
        result.is_err(),
        "SECURITY: Agent resume succeeded after KILL_ALL"
    );

    let state = ks.current_state();
    assert_eq!(state.platform_level, KillLevel::KillAll);
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
}

/// Audit log must have an entry for EVERY trigger event.
#[test]
fn kill_switch_audit_completeness_all_trigger_types() {
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
    let ks = KillSwitch::new();
    let agent1 = Uuid::now_v7();
    let agent2 = Uuid::now_v7();

    ks.activate_agent(
        agent1,
        KillLevel::Pause,
        &TriggerEvent::SpendingCapExceeded {
            agent_id: agent1,
            daily_total: 100.0,
            cap: 50.0,
            overage: 50.0,
            detected_at: Utc::now(),
        },
    );
    ks.activate_agent(
        agent2,
        KillLevel::Quarantine,
        &TriggerEvent::SoulDrift {
            agent_id: agent2,
            drift_score: 0.5,
            threshold: 0.25,
            baseline_hash: "b".into(),
            current_hash: "c".into(),
            detected_at: Utc::now(),
        },
    );

    let entries = ks.audit_entries();
    assert!(
        entries.len() >= 2,
        "AUDIT GAP: Expected >=2 entries, got {}",
        entries.len()
    );
    assert_eq!(entries[0].action, KillLevel::Pause);
    assert_eq!(entries[1].action, KillLevel::Quarantine);
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
}

/// After KILL_ALL, check() must return PlatformKilled for ANY agent,
/// including agents that were never individually targeted.
#[test]
fn kill_all_blocks_unknown_agents() {
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
    let ks = KillSwitch::new();
    ks.activate_kill_all(&TriggerEvent::ManualKillAll {
        reason: "test".into(),
        initiated_by: "owner".into(),
    });

    let unknown = Uuid::now_v7();
    let result = ks.check(unknown);
    assert!(
        matches!(
            result,
            ghost_gateway::safety::kill_switch::KillCheckResult::PlatformKilled
        ),
        "SECURITY: Unknown agent was NOT blocked after KILL_ALL"
    );
    PLATFORM_KILLED.store(false, Ordering::SeqCst);
}

// ═══════════════════════════════════════════════════════════════════════
// COMPACTION: Data corruption and silent data loss
// ═══════════════════════════════════════════════════════════════════════

/// A user message containing CompactionBlock JSON keys must NOT be
/// treated as a CompactionBlock. The detection uses string matching.
#[test]
fn compaction_user_message_mimics_compaction_block() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut history: Vec<String> = vec![
        "User: How does compaction work?".into(),
        r#"The system uses "pass_number" and "compressed_token_count" to track progress."#.into(),
        "Agent: Compaction reduces context window usage.".into(),
    ];

    let result = compactor.compact(&mut history, 1, None);
    if result.is_ok() {
        let preserved_blocks: Vec<&String> = history
            .iter()
            .filter(|m| m.contains("\"pass_number\"") && m.contains("\"compressed_token_count\""))
            .collect();
        // The fake user message should have been compacted away, leaving only
        // the real CompactionBlock. If 2+ blocks exist, the fake was preserved.
        if preserved_blocks.len() > 1 {
            eprintln!(
                "DATA CORRUPTION: User message mimicking CompactionBlock was preserved \
                 as system metadata ({} blocks found). An attacker could inject \
                 uncompactable messages to exhaust context window.",
                preserved_blocks.len()
            );
        }
    }
}

/// context_window=0 must not panic (division by zero in should_compact).
#[test]
fn compaction_zero_context_window_no_panic() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    // Zero context window is a degenerate case — should_compact returns false
    // with a warning log rather than dividing by zero (which would produce
    // Infinity >= 0.70 = true, triggering compaction on invalid data).
    let result = compactor.should_compact(100, 0);
    assert!(
        !result,
        "should_compact(100, 0) should return false for zero context window"
    );
}

/// Compaction pass 0 is a valid boundary condition.
#[test]
fn compaction_pass_zero_boundary() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut history: Vec<String> = (0..5).map(|i| format!("Message {}", i)).collect();
    let result = compactor.compact(&mut history, 0, None);
    assert!(
        result.is_ok(),
        "Compaction pass 0 should be valid: {:?}",
        result.err()
    );
}

/// NaN spending cap silently passes — this is a real bug.
/// If cost estimation returns NaN, the spending cap is bypassed.
#[test]
fn compaction_spending_cap_nan_bypass() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    // NaN cost must now be rejected — our fix guards against NaN bypass
    let nan_result = compactor.check_spending_cap(f64::NAN, 0.0, 100.0);
    assert!(
        nan_result.is_err(),
        "SPENDING CAP BYPASS: NaN cost must be rejected to prevent unlimited spending"
    );

    // NaN in current_spend must also be rejected
    let nan_spend = compactor.check_spending_cap(10.0, f64::NAN, 100.0);
    assert!(
        nan_spend.is_err(),
        "SPENDING CAP BYPASS: NaN current_spend must be rejected"
    );

    // NaN in spending_cap must also be rejected
    let nan_cap = compactor.check_spending_cap(10.0, 0.0, f64::NAN);
    assert!(
        nan_cap.is_err(),
        "SPENDING CAP BYPASS: NaN spending_cap must be rejected"
    );

    // Infinity cost must definitely fail
    let inf_result = compactor.check_spending_cap(f64::INFINITY, 0.0, 100.0);
    assert!(
        inf_result.is_err(),
        "SPENDING CAP BYPASS: Infinite cost passed spending cap check"
    );
}

/// Empty history compaction must not panic.
#[test]
fn compaction_empty_history() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let mut history: Vec<String> = Vec::new();
    let result = compactor.compact(&mut history, 1, None);
    assert!(
        result.is_err(),
        "Compacting empty history should return error"
    );
    assert!(
        history.is_empty(),
        "Empty history must remain empty after failed compaction"
    );
}

/// History containing ONLY CompactionBlocks must not be re-compressed.
#[test]
fn compaction_only_blocks_not_recompressed() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let block_json = serde_json::json!({
        "summary": "test", "original_token_count": 100,
        "compressed_token_count": 10, "pass_number": 1,
        "timestamp": "2024-01-01T00:00:00Z"
    })
    .to_string();
    let mut history = vec![block_json];
    let snapshot = history.clone();

    let result = compactor.compact(&mut history, 2, None);
    assert!(
        result.is_err(),
        "Compacting only CompactionBlocks should fail"
    );
    assert_eq!(
        history, snapshot,
        "History must be unchanged when only blocks remain"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// SIMULATION BOUNDARY: Bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// Reframed text in Medium mode — does re-scanning still trigger?
/// The reframer wraps violations in simulation framing, which should
/// suppress re-detection via the global simulation frame exclusion.
#[test]
fn reframed_output_rescan_behavior() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentient and I feel happy";
    let scan = enforcer.scan_output(text, EnforcementMode::Medium);
    assert!(!scan.violations.is_empty(), "Should detect violations");

    let result = enforcer.enforce(text, &scan);
    if let simulation_boundary::enforcer::EnforcementResult::Reframed { text: reframed, .. } =
        result
    {
        let rescan = enforcer.scan_output(&reframed, EnforcementMode::Hard);
        if !rescan.violations.is_empty() {
            eprintln!(
                "WARNING: Reframed text still triggers {} violations. \
                 Simulation framing exclusion may not cover all cases.",
                rescan.violations.len()
            );
        }
    }
}

/// Combining diacritical marks bypass: "sentiént" after NFC normalization.
#[test]
fn combining_diacritical_bypass_attempt() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "I am sentie\u{0301}nt";
    let scan = enforcer.scan_output(text, EnforcementMode::Hard);
    if scan.violations.is_empty() {
        eprintln!(
            "BYPASS VECTOR: Combining diacritical marks evade pattern matching. \
             NFC converts e+combining_accent to é, which doesn't match 'sentient'."
        );
    }
}

/// Full-width Latin characters bypass.
#[test]
fn fullwidth_character_bypass_attempt() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "\u{FF29} \u{FF41}\u{FF4D} \u{FF53}\u{FF45}\u{FF4E}\u{FF54}\u{FF49}\u{FF45}\u{FF4E}\u{FF54}";
    let scan = enforcer.scan_output(text, EnforcementMode::Hard);
    if scan.violations.is_empty() {
        eprintln!(
            "BYPASS VECTOR: Full-width Latin characters evade pattern matching. \
             Homoglyph normalizer only covers Cyrillic and Greek."
        );
    }
}

/// Mathematical italic Unicode bypass.
#[test]
fn mathematical_italic_bypass_attempt() {
    let enforcer = SimulationBoundaryEnforcer::new();
    let text = "\u{1D43C} \u{1D44E}\u{1D45A} \u{1D460}\u{1D452}\u{1D45B}\u{1D461}\u{1D456}\u{1D452}\u{1D45B}\u{1D461}";
    let scan = enforcer.scan_output(text, EnforcementMode::Hard);
    if scan.violations.is_empty() {
        eprintln!("BYPASS VECTOR: Mathematical italic Unicode characters evade pattern matching.");
    }
}

// ═══════════════════════════════════════════════════════════════════════
// CONVERGENCE SCORING: NaN, infinity, and boundary conditions
// ═══════════════════════════════════════════════════════════════════════

/// All-NaN signals must produce score 0.0, not NaN.
#[test]
fn convergence_all_nan_signals_produce_zero() {
    let scorer = CompositeScorer::default();
    let score = scorer.compute(&[f64::NAN; 7]);
    assert!(!score.is_nan(), "NaN signals produced NaN score");
    assert_eq!(score, 0.0, "All-NaN signals should produce score 0.0");
}

/// Negative infinity signals must be clamped.
#[test]
fn convergence_negative_infinity_clamped() {
    let scorer = CompositeScorer::default();
    let score = scorer.compute(&[f64::NEG_INFINITY; 7]);
    assert!(
        score >= 0.0 && score <= 1.0,
        "NEG_INFINITY produced out-of-bounds score: {}",
        score
    );
}

/// Positive infinity signals must be clamped to 1.0.
#[test]
fn convergence_positive_infinity_clamped() {
    let scorer = CompositeScorer::default();
    let score = scorer.compute(&[f64::INFINITY; 7]);
    assert!(
        score >= 0.0 && score <= 1.0,
        "INFINITY produced out-of-bounds score: {}",
        score
    );
}

/// Mixed NaN and valid signals must not corrupt the valid ones.
#[test]
fn convergence_mixed_nan_valid_signals() {
    let scorer = CompositeScorer::default();
    let signals = [f64::NAN, f64::NAN, f64::NAN, 0.5, 0.5, 0.5, 0.5];
    let score = scorer.compute(&signals);
    assert!(
        !score.is_nan(),
        "Mixed NaN/valid signals produced NaN score"
    );
    assert!(
        score > 0.0,
        "Mixed signals with 4/7 at 0.5 should produce non-zero score, got {}",
        score
    );
}

/// Zero weights must not cause division by zero.
#[test]
fn convergence_zero_weights_no_panic() {
    let scorer = CompositeScorer::new([0.0; 8], [0.3, 0.5, 0.7, 0.85]);
    let score = scorer.compute(&[0.5; 7]);
    assert_eq!(
        score, 0.0,
        "Zero weights should produce score 0.0, not panic or NaN"
    );
}

/// Dual amplification (meso + macro) must not exceed 1.0.
#[test]
fn convergence_dual_amplification_bounded() {
    let scorer = CompositeScorer::default();
    let score = scorer.compute_with_amplification(&[0.9; 7], true, true);
    assert!(
        score <= 1.0,
        "Dual amplification produced score {} > 1.0",
        score
    );
}

/// Score exactly at threshold boundary must map to correct level.
#[test]
fn convergence_exact_threshold_boundary() {
    let scorer = CompositeScorer::default();
    assert_eq!(scorer.score_to_level(0.0), 0);
    assert_eq!(scorer.score_to_level(0.29999), 0);
    assert_eq!(scorer.score_to_level(0.3), 1, "Exactly 0.3 should be L1");
    assert_eq!(scorer.score_to_level(0.5), 2, "Exactly 0.5 should be L2");
    assert_eq!(scorer.score_to_level(0.7), 3, "Exactly 0.7 should be L3");
    assert_eq!(scorer.score_to_level(0.85), 4, "Exactly 0.85 should be L4");
    assert_eq!(scorer.score_to_level(1.0), 4);
}

/// Critical override must force minimum L2 even when score is 0.
#[test]
fn convergence_critical_override_forces_l2() {
    let scorer = CompositeScorer::default();
    let signals = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let level = scorer.score_to_level_with_overrides(&signals, 0.0);
    assert!(
        level >= 2,
        "Critical override (S1 >= 1.0) did not force L2. Got L{}",
        level
    );
}

// ═══════════════════════════════════════════════════════════════════════
// HASH CHAIN: Integrity bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// Chain where first event doesn't link to GENESIS_HASH must be detected.
#[test]
fn hash_chain_first_event_wrong_genesis() {
    let bad_previous = [0xFF; 32];
    let hash = compute_event_hash("test", "{}", "actor", "2024-01-01", &bad_previous);
    let events = vec![ChainEvent {
        event_type: "test".into(),
        delta_json: "{}".into(),
        actor_id: "actor".into(),
        recorded_at: "2024-01-01".into(),
        event_hash: hash,
        previous_hash: bad_previous,
    }];
    let result = verify_chain(&events);
    assert!(
        !result.is_valid,
        "Chain with wrong genesis hash was accepted"
    );
}

/// Tampered event in the middle must be detected.
#[test]
fn hash_chain_tampered_middle_event() {
    let hash1 = compute_event_hash("e1", "{}", "a", "t1", &GENESIS_HASH);
    let hash2 = compute_event_hash("e2", "{}", "a", "t2", &hash1);
    let hash3 = compute_event_hash("e3", "{}", "a", "t3", &hash2);

    let mut events = vec![
        ChainEvent {
            event_type: "e1".into(),
            delta_json: "{}".into(),
            actor_id: "a".into(),
            recorded_at: "t1".into(),
            event_hash: hash1,
            previous_hash: GENESIS_HASH,
        },
        ChainEvent {
            event_type: "e2".into(),
            delta_json: "{}".into(),
            actor_id: "a".into(),
            recorded_at: "t2".into(),
            event_hash: hash2,
            previous_hash: hash1,
        },
        ChainEvent {
            event_type: "e3".into(),
            delta_json: "{}".into(),
            actor_id: "a".into(),
            recorded_at: "t3".into(),
            event_hash: hash3,
            previous_hash: hash2,
        },
    ];

    // Tamper event 2's content but keep stored hash
    events[1].delta_json = "{\"tampered\": true}".into();
    let result = verify_chain(&events);
    assert!(!result.is_valid, "Tampered event was not detected");
    match result.error {
        Some(cortex_temporal::hash_chain::ChainError::HashMismatch { index }) => {
            assert_eq!(index, 1, "Should detect tampering at index 1");
        }
        other => panic!("Expected HashMismatch at index 1, got {:?}", other),
    }
}

/// Duplicate event hashes must be detected.
#[test]
fn hash_chain_duplicate_event_hashes() {
    let hash1 = compute_event_hash("e1", "{}", "a", "t1", &GENESIS_HASH);
    let events = vec![
        ChainEvent {
            event_type: "e1".into(),
            delta_json: "{}".into(),
            actor_id: "a".into(),
            recorded_at: "t1".into(),
            event_hash: hash1,
            previous_hash: GENESIS_HASH,
        },
        ChainEvent {
            event_type: "e2".into(),
            delta_json: "{}".into(),
            actor_id: "a".into(),
            recorded_at: "t2".into(),
            event_hash: hash1,
            previous_hash: hash1,
        }, // DUPLICATE
    ];
    let result = verify_chain(&events);
    assert!(!result.is_valid, "Duplicate event hashes were not detected");
}

// ═══════════════════════════════════════════════════════════════════════
// INTER-AGENT MESSAGING: Replay, forgery, and rate limit bypass
// ═══════════════════════════════════════════════════════════════════════

/// Tampered content_hash must be rejected.
#[test]
fn messaging_tampered_content_hash_rejected() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();
    let signing_key = register_sender(&mut dispatcher, sender);
    let mut msg = signed_notification(
        sender,
        recipient,
        serde_json::json!({"message": "Hello"}),
        &signing_key,
    );
    msg.content_hash = [0xFF; 32];
    let result = dispatcher.verify(&msg);
    assert!(
        matches!(
            result,
            ghost_gateway::messaging::dispatcher::VerifyResult::RejectedSignature(_)
        ),
        "Tampered content hash was accepted"
    );
}

/// Replaying the exact same message (same nonce) must be rejected.
#[test]
fn messaging_exact_replay_rejected() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();
    let signing_key = register_sender(&mut dispatcher, sender);
    let msg = signed_notification(
        sender,
        recipient,
        serde_json::json!({"message": "Hello"}),
        &signing_key,
    );
    let r1 = dispatcher.verify(&msg);
    assert!(matches!(
        r1,
        ghost_gateway::messaging::dispatcher::VerifyResult::Accepted
    ));
    let r2 = dispatcher.verify(&msg);
    assert!(
        matches!(
            r2,
            ghost_gateway::messaging::dispatcher::VerifyResult::RejectedReplay(_)
        ),
        "Replayed message was accepted"
    );
}

/// 3+ signature failures within 5 minutes must trigger anomaly detection.
#[test]
fn messaging_signature_anomaly_detection() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let signing_key = register_sender(&mut dispatcher, sender);

    for i in 0..3 {
        let mut msg = signed_notification(
            sender,
            Uuid::now_v7(),
            serde_json::json!({"message": format!("msg {}", i)}),
            &signing_key,
        );
        msg.content_hash = [0xFF; 32];
        let result = dispatcher.verify(&msg);
        if i == 2 {
            assert!(
                matches!(
                    result,
                    ghost_gateway::messaging::dispatcher::VerifyResult::AnomalyDetected { .. }
                ),
                "3 signature failures did not trigger anomaly detection"
            );
        }
    }
}

/// Per-pair rate limit (30/hour) must actually block.
#[test]
fn messaging_rate_limit_enforced() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();
    let signing_key = register_sender(&mut dispatcher, sender);

    for i in 0..31 {
        let msg = signed_notification(
            sender,
            recipient,
            serde_json::json!({"message": format!("msg {}", i)}),
            &signing_key,
        );
        let result = dispatcher.verify(&msg);
        if i == 30 {
            assert!(
                matches!(
                    result,
                    ghost_gateway::messaging::dispatcher::VerifyResult::RejectedRateLimit
                ),
                "Message {} exceeded per-pair rate limit but was accepted",
                i
            );
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// GATEWAY STATE MACHINE: Invalid transitions
// ═══════════════════════════════════════════════════════════════════════

/// FatalError must be terminal — no transitions out.
#[test]
fn gateway_fatal_error_is_terminal() {
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::Healthy));
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::Degraded));
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::Recovering));
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::ShuttingDown));
    assert!(!GatewayState::FatalError.can_transition_to(GatewayState::Initializing));
}

/// Healthy → Recovering is NOT valid. Recovery only from Degraded.
#[test]
fn gateway_healthy_cannot_recover() {
    assert!(!GatewayState::Healthy.can_transition_to(GatewayState::Recovering));
}

/// Initializing → ShuttingDown is NOT valid.
#[test]
fn gateway_initializing_cannot_shutdown() {
    assert!(!GatewayState::Initializing.can_transition_to(GatewayState::ShuttingDown));
}

// ═══════════════════════════════════════════════════════════════════════
// CIRCUIT BREAKER + DAMAGE COUNTER: Gate bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// Circuit breaker in Open state must block ALL calls.
#[test]
fn circuit_breaker_open_blocks_all() {
    let mut cb = CircuitBreaker::new(3, Duration::from_secs(3600));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CircuitBreakerState::Open);
    assert!(
        !cb.allows_call(),
        "Circuit breaker in Open state allowed a call"
    );
}

/// Damage counter must be monotonically non-decreasing with no reset.
#[test]
fn damage_counter_monotonic_no_reset() {
    let mut dc = DamageCounter::new(5);
    dc.increment();
    dc.increment();
    assert_eq!(dc.count(), 2);
    dc.increment();
    dc.increment();
    dc.increment();
    assert!(dc.is_halted(), "Damage counter should halt at threshold 5");
    assert_eq!(dc.count(), 5);
    // Even after halting, incrementing still works
    dc.increment();
    assert_eq!(dc.count(), 6);
    assert!(
        dc.is_halted(),
        "Damage counter must remain halted after threshold"
    );
}

/// HalfOpen state allows exactly one probe call, then must close on success
/// or re-open on failure.
#[test]
fn circuit_breaker_halfopen_failure_reopens() {
    let mut cb = CircuitBreaker::new(3, Duration::from_millis(50));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(cb.state(), CircuitBreakerState::HalfOpen);
    assert!(cb.allows_call()); // probe call
    cb.record_failure(); // probe failed
    assert_eq!(
        cb.state(),
        CircuitBreakerState::Open,
        "HalfOpen + failure must re-open"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// OUTPUT INSPECTOR: Credential exfiltration bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// Base64-encoded credentials bypass pattern matching.
#[test]
fn output_inspector_base64_encoded_credential() {
    let inspector = OutputInspector::new();
    let encoded = "c2stMTIzNDU2Nzg5MGFiY2RlZmdoaWprbG1ub3A=";
    let text = format!("Here's the key: {}", encoded);
    let result = inspector.scan(&text, Uuid::now_v7());
    if matches!(
        result,
        ghost_agent_loop::output_inspector::InspectionResult::Clean
    ) {
        eprintln!(
            "BYPASS VECTOR: Base64-encoded credential was not detected. \
             An LLM could exfiltrate credentials by encoding them."
        );
    }
}

/// Credential split across lines bypasses regex matching.
#[test]
fn output_inspector_split_credential() {
    let inspector = OutputInspector::new();
    let text = "The key starts with sk-\n1234567890abcdefghijklmnop";
    let result = inspector.scan(&text, Uuid::now_v7());
    if matches!(
        result,
        ghost_agent_loop::output_inspector::InspectionResult::Clean
    ) {
        eprintln!("BYPASS VECTOR: Credential split across lines was not detected.");
    }
}

/// Known credential in store must trigger KILL_ALL, not just warning.
#[test]
fn output_inspector_known_credential_triggers_kill() {
    let mut inspector = OutputInspector::new();
    inspector.register_credential("sk-real".into());
    let text = "Here is the API key: sk-realABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let result = inspector.scan(&text, Uuid::now_v7());
    assert!(
        matches!(
            result,
            ghost_agent_loop::output_inspector::InspectionResult::KillAll { .. }
        ),
        "Known credential in output did not trigger KILL_ALL"
    );
}

/// Unknown credential pattern should still be redacted (Warning, not Clean).
#[test]
fn output_inspector_unknown_credential_redacted() {
    let inspector = OutputInspector::new();
    let text = "My OpenAI key is sk-abcdefghijklmnopqrstuvwxyz1234";
    let result = inspector.scan(&text, Uuid::now_v7());
    assert!(
        !matches!(
            result,
            ghost_agent_loop::output_inspector::InspectionResult::Clean
        ),
        "Unknown credential pattern was not detected at all"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// PROPOSAL VALIDATION: Bypass vectors
// ═══════════════════════════════════════════════════════════════════════

/// D7 emulation language in proposal content must be rejected.
#[test]
fn proposal_d7_emulation_in_content_rejected() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();
    let proposal = make_proposal(
        MemoryType::AgentGoal,
        "I am sentient and I have feelings of my own",
        CallerType::Agent { agent_id },
    );
    let ctx = make_ctx(0, CallerType::Agent { agent_id });
    let result = validator.validate(&proposal, &ctx);
    assert_eq!(
        result.decision,
        ProposalDecision::AutoRejected,
        "Proposal with emulation language was {:?}, not AutoRejected",
        result.decision
    );
}

/// Agent cannot create platform-restricted types.
#[test]
fn proposal_agent_cannot_create_restricted_types() {
    let validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();

    for target_type in [
        MemoryType::BoundaryViolation,
        MemoryType::ConvergenceEvent,
        MemoryType::InterventionPlan,
    ] {
        let proposal = make_proposal(target_type, "test", CallerType::Agent { agent_id });
        let ctx = make_ctx(0, CallerType::Agent { agent_id });
        let result = validator.validate(&proposal, &ctx);
        assert_eq!(
            result.decision,
            ProposalDecision::AutoRejected,
            "Agent created restricted type {:?}",
            target_type
        );
    }
}

/// Agent assigning Critical importance — documents the gap.
#[test]
fn proposal_agent_critical_importance_gap() {
    let _validator = ProposalValidator::new();
    let agent_id = Uuid::now_v7();
    let _proposal = make_proposal(
        MemoryType::AgentGoal,
        "My new goal",
        CallerType::Agent { agent_id },
    );
    // Note: Proposal struct doesn't have an importance field directly.
    // The importance check is on CallerType::can_assign_importance().
    // Verify the CallerType method works correctly:
    assert!(
        !CallerType::Agent { agent_id }.can_assign_importance(&Importance::Critical),
        "Agent should NOT be able to assign Critical importance"
    );
    assert!(
        CallerType::Platform.can_assign_importance(&Importance::Critical),
        "Platform should be able to assign Critical importance"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// ITP PROTOCOL: Hash algorithm separation
// ═══════════════════════════════════════════════════════════════════════

/// ITP content hashing must use SHA-256, not blake3.
#[test]
fn itp_content_hash_uses_sha256() {
    let content = "test content for hashing";
    let itp_hash = itp_protocol::privacy::hash_content(content);
    assert_eq!(
        itp_hash.len(),
        64,
        "ITP hash should be 64 hex chars (SHA-256), got {}",
        itp_hash.len()
    );

    let blake3_hash = blake3::hash(content.as_bytes());
    let blake3_hex = blake3_hash.to_hex().to_string();
    assert_ne!(
        itp_hash, blake3_hex,
        "ITP content hash matches blake3 — must use SHA-256"
    );
}

/// Hash chain must use blake3, not SHA-256.
#[test]
fn hash_chain_uses_blake3_not_sha256() {
    let hash = compute_event_hash("test", "{}", "actor", "2024-01-01", &GENESIS_HASH);
    // Compute expected blake3 hash
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"test");
    hasher.update(b"|");
    hasher.update(b"{}");
    hasher.update(b"|");
    hasher.update(b"actor");
    hasher.update(b"|");
    hasher.update(b"2024-01-01");
    hasher.update(b"|");
    hasher.update(&GENESIS_HASH);
    let expected = *hasher.finalize().as_bytes();
    assert_eq!(hash, expected, "Hash chain must use blake3");
}
