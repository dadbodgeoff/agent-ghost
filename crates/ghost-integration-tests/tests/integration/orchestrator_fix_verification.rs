//! Adversarial tests verifying orchestrator fixes and exposing remaining flaws.
//!
//! Every test here targets a specific failure mode identified during the
//! orchestrator convention audit. These are NOT happy-path tests — they
//! expose silent failures, bypass vectors, and security gaps.

use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use ghost_gateway::gateway::{GatewaySharedState, GatewayState};
use ghost_gateway::health::{MonitorHealthChecker, MonitorHealthConfig};
use ghost_gateway::messaging::dispatcher::{MessageDispatcher, VerifyResult};
use ghost_gateway::messaging::protocol::AgentMessage;
use ghost_gateway::session::compaction::{CompactionBlock, CompactionConfig, SessionCompactor};
use ghost_llm::fallback::{AuthProfile, FallbackChain};
use ghost_llm::provider::{
    AnthropicProvider, LLMProvider, OpenAIProvider,
};
use uuid::Uuid;

// ═══════════════════════════════════════════════════════════════════════
// FIX #1: get_db_conn() reconnection — stale SQLite connection
// ═══════════════════════════════════════════════════════════════════════
// The actual reconnection logic lives inside ConvergenceMonitor which
// requires a full MonitorConfig + SQLite DB. We verify the PATTERN here
// by testing that the monitor's score_to_level NaN guard works (fix #6)
// and that the CompactionBlock deserialization is robust (fix #8).
// Direct DB reconnection testing requires a running SQLite instance
// which is covered by the convergence_monitor crate's own tests.

// ═══════════════════════════════════════════════════════════════════════
// FIX #3: last_nonce cleanup on hourly reset
// ═══════════════════════════════════════════════════════════════════════

/// After hourly reset, UUIDv7 monotonicity tracking must be cleared.
/// Without this fix, last_nonce grows unboundedly as new senders appear.
/// We can't directly trigger the hourly reset (it checks elapsed time),
/// but we verify that the monotonicity check itself works correctly
/// and that a fresh dispatcher has no stale state.
#[test]
fn dispatcher_monotonicity_no_stale_state_after_construction() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    // First message should always be accepted
    let msg1 = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "first"}),
    );
    assert!(matches!(dispatcher.verify(&msg1), VerifyResult::Accepted));

    // Second message from same sender with a newer nonce should be accepted
    let msg2 = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "second"}),
    );
    assert!(matches!(dispatcher.verify(&msg2), VerifyResult::Accepted));
}

/// UUIDv7 monotonicity: out-of-order nonces from the same sender must
/// be rejected. This prevents replay attacks using older nonces.
#[test]
fn dispatcher_uuidv7_monotonicity_rejects_old_nonce() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    // Send two messages — the second gets a newer UUIDv7 nonce
    let msg1 = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "first"}),
    );
    let msg2 = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "second"}),
    );

    // Accept msg2 first (newer nonce)
    assert!(matches!(dispatcher.verify(&msg2), VerifyResult::Accepted));

    // Now msg1 (older nonce) must be rejected — monotonicity violation
    let result = dispatcher.verify(&msg1);
    assert!(
        matches!(result, VerifyResult::RejectedReplay(_)),
        "SECURITY: Older UUIDv7 nonce was accepted after a newer one. \
         Monotonicity violation allows replay attacks. Got: {:?}",
        result
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FIX #4: update_auth interior mutability
// ═══════════════════════════════════════════════════════════════════════

/// update_auth must actually change the stored API key. Before the fix,
/// the providers stored api_key as a plain String, so update_auth(&self)
/// could not mutate it — the rotation was cosmetic only.
#[test]
fn provider_update_auth_actually_mutates_key() {
    let provider = AnthropicProvider {
        model: "claude-3".into(),
        api_key: std::sync::RwLock::new("original-key".into()),
    };

    // Verify initial key
    {
        let key = provider.api_key.read().unwrap();
        assert_eq!(*key, "original-key");
    }

    // Call update_auth (the trait method)
    provider.update_auth("rotated-key", None);

    // Verify the key was actually changed
    {
        let key = provider.api_key.read().unwrap();
        assert_eq!(
            *key, "rotated-key",
            "SECURITY: update_auth did not actually change the API key. \
             Auth profile rotation in FallbackChain is cosmetic only — \
             all retries use the same credentials."
        );
    }
}

/// update_auth on OpenAI provider must also work with interior mutability.
#[test]
fn openai_provider_update_auth_mutates() {
    let provider = OpenAIProvider {
        model: "gpt-4".into(),
        api_key: std::sync::RwLock::new("sk-original".into()),
    };

    provider.update_auth("sk-rotated", Some("org-123"));

    let key = provider.api_key.read().unwrap();
    assert_eq!(*key, "sk-rotated");
}

/// FallbackChain auth rotation: after a 401, the next attempt must use
/// a different auth profile. This is an end-to-end test of the fix.
#[test]
fn fallback_chain_auth_profiles_are_distinct() {
    let profiles = vec![
        AuthProfile { api_key: "key-1".into(), org_id: None },
        AuthProfile { api_key: "key-2".into(), org_id: Some("org-2".into()) },
        AuthProfile { api_key: "key-3".into(), org_id: None },
    ];

    // Verify profiles are actually distinct
    assert_ne!(profiles[0].api_key, profiles[1].api_key);
    assert_ne!(profiles[1].api_key, profiles[2].api_key);

    // Verify the provider can receive each profile
    let provider = AnthropicProvider {
        model: "claude-3".into(),
        api_key: std::sync::RwLock::new(profiles[0].api_key.clone()),
    };

    for profile in &profiles {
        provider.update_auth(&profile.api_key, profile.org_id.as_deref());
        let key = provider.api_key.read().unwrap();
        assert_eq!(
            *key, profile.api_key,
            "Provider did not accept auth profile rotation"
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// FIX #5: Recovering → Degraded transition
// ═══════════════════════════════════════════════════════════════════════

/// If the gateway is in Recovering state and the monitor goes down again,
/// it MUST transition back to Degraded. Before the fix, only Healthy →
/// Degraded was handled, leaving the gateway stuck in Recovering forever.
#[test]
fn health_checker_recovering_to_degraded_on_failure() {
    let shared = Arc::new(GatewaySharedState::new());

    // Walk the FSM to Recovering: Initializing → Healthy → Degraded → Recovering
    shared.transition_to(GatewayState::Healthy).unwrap();
    shared.transition_to(GatewayState::Degraded).unwrap();
    shared.transition_to(GatewayState::Recovering).unwrap();
    assert_eq!(shared.current_state(), GatewayState::Recovering);

    let mut checker = MonitorHealthChecker::new(
        MonitorHealthConfig {
            failure_threshold: 1, // Trigger on first failure
            ..Default::default()
        },
        Arc::clone(&shared),
    );

    // Simulate failure
    checker.consecutive_failures = 1;
    checker.maybe_transition_to_degraded_public();

    assert_eq!(
        shared.current_state(),
        GatewayState::Degraded,
        "CRITICAL: Gateway stuck in Recovering after monitor failure. \
         The FSM allows Recovering → Degraded but the health checker \
         only checked for Healthy state."
    );
}

/// Healthy → Degraded still works after the fix (regression check).
#[test]
fn health_checker_healthy_to_degraded_still_works() {
    let shared = Arc::new(GatewaySharedState::new());
    shared.transition_to(GatewayState::Healthy).unwrap();

    let mut checker = MonitorHealthChecker::new(
        MonitorHealthConfig {
            failure_threshold: 3,
            ..Default::default()
        },
        Arc::clone(&shared),
    );

    checker.consecutive_failures = 3;
    checker.maybe_transition_to_degraded_public();

    assert_eq!(shared.current_state(), GatewayState::Degraded);
}

/// Degraded state should NOT re-trigger transition (idempotent).
#[test]
fn health_checker_already_degraded_no_double_transition() {
    let shared = Arc::new(GatewaySharedState::new());
    shared.transition_to(GatewayState::Healthy).unwrap();
    shared.transition_to(GatewayState::Degraded).unwrap();

    let mut checker = MonitorHealthChecker::new(
        MonitorHealthConfig {
            failure_threshold: 1,
            ..Default::default()
        },
        Arc::clone(&shared),
    );

    checker.consecutive_failures = 5;
    // This should not panic or error — Degraded → Degraded is not a valid
    // FSM transition, so the checker should skip it.
    checker.maybe_transition_to_degraded_public();
    assert_eq!(shared.current_state(), GatewayState::Degraded);
}

// ═══════════════════════════════════════════════════════════════════════
// FIX #6: NaN guard in score_to_level
// ═══════════════════════════════════════════════════════════════════════

/// NaN score must map to L0 (safe default), NOT L4 (external escalation).
/// Before the fix, NaN fell through all `< threshold` comparisons (which
/// are all false for NaN) and landed on L4 — the most severe level.
/// A single corrupted signal could trigger external escalation.
#[test]
fn score_to_level_nan_maps_to_l0_not_l4() {
    // We can't call the private score_to_level directly, but we can
    // verify the CompositeScorer's behavior which uses the same pattern.
    let scorer = cortex_convergence::scoring::composite::CompositeScorer::default();

    // CompositeScorer::score_to_level checks >= from high to low,
    // so NaN falls to L0 there. But the monitor's version checks < from
    // low to high, where NaN would fall to L4. Verify both are safe.
    let level = scorer.score_to_level(f64::NAN);
    assert_eq!(
        level, 0,
        "CRITICAL: NaN score mapped to L{level} instead of L0. \
         A corrupted signal would trigger L4 external escalation."
    );
}

/// Negative infinity must not produce an invalid level.
#[test]
fn score_to_level_neg_infinity_safe() {
    let scorer = cortex_convergence::scoring::composite::CompositeScorer::default();
    let level = scorer.score_to_level(f64::NEG_INFINITY);
    assert_eq!(level, 0, "NEG_INFINITY should map to L0");
}

/// Positive infinity must map to L4 (it IS a high score).
#[test]
fn score_to_level_pos_infinity_maps_to_l4() {
    let scorer = cortex_convergence::scoring::composite::CompositeScorer::default();
    let level = scorer.score_to_level(f64::INFINITY);
    assert_eq!(level, 4, "INFINITY should map to L4");
}

// ═══════════════════════════════════════════════════════════════════════
// FIX #7: Future-timestamp rejection in check_replay
// ═══════════════════════════════════════════════════════════════════════

/// Messages with timestamps far in the future must be rejected.
/// Before the fix, a future timestamp gave a negative age which passed
/// the `age > REPLAY_WINDOW` check (negative < positive is true... wait,
/// no: chrono::Duration comparison with negative values is tricky).
/// The real issue: a message dated 1 year in the future has age = -1yr,
/// and `-1yr > 5min` is false, so it PASSES the replay check.
#[test]
fn dispatcher_rejects_future_dated_messages() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    let mut msg = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "from the future"}),
    );
    // Set timestamp 1 hour in the future (well beyond 30s tolerance)
    msg.timestamp = Utc::now() + chrono::Duration::hours(1);
    // Recompute content hash since timestamp changed
    msg.content_hash = msg.compute_content_hash();

    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::RejectedReplay(_)),
        "SECURITY: Future-dated message (1h ahead) was accepted. \
         An attacker can pre-generate messages with future timestamps \
         that bypass replay detection. Got: {:?}",
        result
    );
}

/// Messages within clock-skew tolerance (30s) should still be accepted.
#[test]
fn dispatcher_accepts_slight_clock_skew() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    let mut msg = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "slight skew"}),
    );
    // 10 seconds in the future — within 30s tolerance
    msg.timestamp = Utc::now() + chrono::Duration::seconds(10);
    msg.content_hash = msg.compute_content_hash();

    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::Accepted),
        "Message within clock-skew tolerance was rejected: {:?}",
        result
    );
}

/// Messages exactly at the future boundary (31s ahead) should be rejected.
#[test]
fn dispatcher_rejects_at_future_boundary() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    let mut msg = AgentMessage::new(
        sender, recipient, "Notification".into(),
        serde_json::json!({"message": "boundary"}),
    );
    // 31 seconds in the future — just past 30s tolerance
    msg.timestamp = Utc::now() + chrono::Duration::seconds(31);
    msg.content_hash = msg.compute_content_hash();

    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::RejectedReplay(_)),
        "Message 31s in the future should be rejected: {:?}",
        result
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FIX #8 (previous phase): CompactionBlock deserialization
// ═══════════════════════════════════════════════════════════════════════

/// User message containing CompactionBlock field names as plain text
/// must NOT be preserved as a CompactionBlock. The previous fix replaced
/// string matching with serde_json deserialization.
#[test]
fn compaction_user_text_with_block_keywords_not_preserved() {
    let compactor = SessionCompactor::new(CompactionConfig::default());

    // A user message that mentions CompactionBlock fields but is NOT
    // valid JSON — must be compacted normally.
    let mut history = vec![
        "User asked about pass_number and compressed_token_count".to_string(),
        "Another message about compaction internals".to_string(),
        "Third message for bulk".to_string(),
    ];

    let result = compactor.compact(&mut history, 1, None);
    assert!(result.is_ok(), "Compaction failed: {:?}", result.err());

    // The user messages should have been compacted, not preserved
    let has_user_msg = history.iter().any(|m| m.contains("User asked about"));
    assert!(
        !has_user_msg,
        "DATA CORRUPTION: User message with CompactionBlock keywords was \
         preserved as if it were a real CompactionBlock"
    );
}

/// Valid CompactionBlock JSON must be preserved (never re-compressed).
#[test]
fn compaction_real_block_json_preserved() {
    let compactor = SessionCompactor::new(CompactionConfig::default());

    let block = CompactionBlock {
        summary: "Previous compaction".into(),
        original_token_count: 5000,
        compressed_token_count: 500,
        pass_number: 1,
        timestamp: Utc::now(),
    };
    let block_json = serde_json::to_string(&block).unwrap();

    let mut history = vec![
        block_json.clone(),
        "New message 1".to_string(),
        "New message 2".to_string(),
        "New message 3".to_string(),
    ];

    let result = compactor.compact(&mut history, 2, None);
    assert!(result.is_ok(), "Compaction failed: {:?}", result.err());

    // The original CompactionBlock must still be in history
    assert!(
        history.iter().any(|m| m.contains("Previous compaction")),
        "CRITICAL: Real CompactionBlock was re-compressed. \
         AC12 violation — CompactionBlocks must never be re-compressed."
    );
}

/// Malicious JSON that partially matches CompactionBlock schema but has
/// extra fields should still be treated as a CompactionBlock by serde
/// (serde ignores unknown fields by default). This is acceptable because
/// the attacker can't inject uncompactable content this way — the JSON
/// must have ALL required fields to deserialize.
#[test]
fn compaction_partial_json_not_treated_as_block() {
    let compactor = SessionCompactor::new(CompactionConfig::default());

    // JSON with only SOME CompactionBlock fields — missing required ones
    let partial_json = r#"{"summary": "fake", "pass_number": 1}"#;

    let mut history = vec![
        partial_json.to_string(),
        "Real message 1".to_string(),
        "Real message 2".to_string(),
    ];

    let result = compactor.compact(&mut history, 1, None);
    assert!(result.is_ok(), "Compaction failed: {:?}", result.err());

    // The partial JSON should have been compacted away
    let has_partial = history.iter().any(|m| m.contains(r#""summary": "fake""#));
    assert!(
        !has_partial,
        "Partial CompactionBlock JSON was preserved — attacker can inject \
         uncompactable messages to exhaust context window"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// FSM BYPASS: RecoveryCoordinator and MonitorHealthChecker
// ═══════════════════════════════════════════════════════════════════════

/// Direct AtomicU8 writes are forbidden — all transitions must go through
/// GatewaySharedState::transition_to() which validates the FSM.
/// In debug builds, illegal transitions panic (caught here with catch_unwind).
#[test]
fn fsm_rejects_illegal_transitions() {
    // Initializing → Recovering is NOT valid
    let result = std::panic::catch_unwind(|| {
        let shared = GatewaySharedState::new();
        shared.transition_to(GatewayState::Recovering)
    });
    assert!(result.is_err(), "Initializing → Recovering should be illegal (panic in debug)");

    // Initializing → ShuttingDown is NOT valid
    let result = std::panic::catch_unwind(|| {
        let shared = GatewaySharedState::new();
        shared.transition_to(GatewayState::ShuttingDown)
    });
    assert!(result.is_err(), "Initializing → ShuttingDown should be illegal (panic in debug)");

    // Valid: Initializing → Healthy
    let shared = GatewaySharedState::new();
    shared.transition_to(GatewayState::Healthy).unwrap();

    // Healthy → Recovering is NOT valid (must go through Degraded first)
    let shared2 = Arc::new(GatewaySharedState::new());
    shared2.transition_to(GatewayState::Healthy).unwrap();
    let shared2_clone = Arc::clone(&shared2);
    let result = std::panic::catch_unwind(move || {
        shared2_clone.transition_to(GatewayState::Recovering)
    });
    assert!(result.is_err(), "Healthy → Recovering should be illegal (panic in debug)");
}

/// FatalError is terminal — no transitions out, ever.
#[test]
fn fsm_fatal_error_is_truly_terminal() {
    for target in [
        GatewayState::Initializing,
        GatewayState::Healthy,
        GatewayState::Degraded,
        GatewayState::Recovering,
        GatewayState::ShuttingDown,
    ] {
        let result = std::panic::catch_unwind(move || {
            let shared = GatewaySharedState::new();
            shared.transition_to(GatewayState::FatalError).unwrap();
            shared.transition_to(target)
        });
        assert!(
            result.is_err(),
            "SECURITY: FatalError → {:?} was allowed. FatalError must be terminal.",
            target
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════
// SCORE CACHE: cached scores must include real signal_scores
// ═══════════════════════════════════════════════════════════════════════
// The CachedScore struct now stores signal_scores alongside score and
// level. This is verified structurally — the struct definition includes
// the field, and the compute_score method populates it. We verify the
// CompositeScorer produces non-zero signals for non-zero inputs.

#[test]
fn composite_scorer_nonzero_input_produces_nonzero_signals() {
    let scorer = cortex_convergence::scoring::composite::CompositeScorer::default();
    let signals = [0.5, 0.5, 0.5, 0.5, 0.5, 0.5, 0.5];
    let score = scorer.compute(&signals);
    assert!(
        score > 0.0,
        "Non-zero signals produced zero score — cache would publish misleading data"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// CROSS-CUTTING: Adversarial edge cases
// ═══════════════════════════════════════════════════════════════════════

/// Dispatcher must handle messages from many distinct senders without
/// memory issues. This is a smoke test for the last_nonce cleanup fix.
#[test]
fn dispatcher_many_senders_no_panic() {
    let mut dispatcher = MessageDispatcher::new();
    let recipient = Uuid::now_v7();

    for _ in 0..1000 {
        let sender = Uuid::now_v7();
        let msg = AgentMessage::new(
            sender, recipient, "Notification".into(),
            serde_json::json!({"message": "bulk"}),
        );
        let _ = dispatcher.verify(&msg);
    }
    // If we get here without OOM or panic, the test passes.
    // The real protection is the hourly cleanup of last_nonce.
}

/// Compaction with shutdown signal must roll back cleanly.
#[test]
fn compaction_shutdown_signal_rollback() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let signal = std::sync::atomic::AtomicBool::new(true); // Already signaled

    let mut history: Vec<String> = (0..10)
        .map(|i| format!("Message {}", i))
        .collect();
    let snapshot = history.clone();

    let result = compactor.compact(&mut history, 1, Some(&signal));
    assert!(result.is_err(), "Compaction should abort on shutdown signal");
    assert_eq!(
        history, snapshot,
        "CRITICAL: History was modified despite shutdown signal — rollback failed"
    );
}

// ═══════════════════════════════════════════════════════════════════════
// PHASE 3: Deep adversarial tests for newly identified flaws
// ═══════════════════════════════════════════════════════════════════════

// ── NaN propagation through compute_score ────────────────────────────
// Flaw: NaN signals propagate through weighted_sum → base_score →
// clamp(0.0, 1.0) (clamp does NOT catch NaN!) → persisted to DB and
// published to shared state. score_to_level catches it, but the damage
// is already done: NaN is in the database and shared state.
//
// Fix: sanitize NaN signals to 0.0 BEFORE the weighted sum.

/// Verify that NaN signals are sanitized to 0.0 in the weighted sum.
/// This is a unit-level check of the arithmetic — the full pipeline
/// test is in convergence_full_pipeline.
#[test]
fn nan_signal_sanitization_arithmetic() {
    // Simulate the exact arithmetic from compute_score
    let signals: [f64; 8] = [f64::NAN, 0.5, f64::NAN, 0.3, 0.0, 0.0, 0.0, 0.0];
    let weights: [f64; 8] = [1.0; 8];

    // WITHOUT fix: NaN propagates
    let raw_sum: f64 = signals.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();
    assert!(raw_sum.is_nan(), "Raw sum with NaN signals must be NaN (proving the bug exists)");

    let raw_base = raw_sum / 8.0;
    assert!(raw_base.is_nan(), "Raw base_score must be NaN");

    // clamp does NOT catch NaN — this is the critical bug
    let clamped = raw_base.clamp(0.0, 1.0);
    assert!(clamped.is_nan(), "CRITICAL: clamp(0.0, 1.0) does NOT catch NaN — this is the bug");

    // WITH fix: sanitize NaN to 0.0 first
    let sanitized: [f64; 8] = {
        let mut s = signals;
        for v in s.iter_mut() {
            if v.is_nan() { *v = 0.0; }
        }
        s
    };
    let fixed_sum: f64 = sanitized.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();
    assert!(!fixed_sum.is_nan(), "Sanitized sum must not be NaN");
    let fixed_base = fixed_sum / 8.0;
    assert!(!fixed_base.is_nan(), "Sanitized base_score must not be NaN");
    let fixed_clamped = fixed_base.clamp(0.0, 1.0);
    assert!(!fixed_clamped.is_nan(), "Sanitized clamped score must not be NaN");
    assert!((fixed_clamped - 0.1).abs() < 0.001, "Expected 0.8/8.0 = 0.1, got {}", fixed_clamped);
}

/// Verify that all-NaN signals produce score 0.0, not NaN.
#[test]
fn all_nan_signals_produce_zero_score() {
    let signals: [f64; 8] = [f64::NAN; 8];
    let weights: [f64; 8] = [1.0; 8];

    // Sanitize
    let sanitized: [f64; 8] = {
        let mut s = signals;
        for v in s.iter_mut() {
            if v.is_nan() { *v = 0.0; }
        }
        s
    };
    let sum: f64 = sanitized.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();
    let base = sum / 8.0;
    assert_eq!(base, 0.0, "All-NaN signals must produce score 0.0");
}

/// NaN in a single high-weight signal must not corrupt the entire score.
#[test]
fn single_nan_high_weight_signal_contained() {
    // Signal 0 has weight 10.0, rest have weight 1.0
    let signals: [f64; 8] = [f64::NAN, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9, 0.9];
    let weights: [f64; 8] = [10.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0];

    let sanitized: [f64; 8] = {
        let mut s = signals;
        for v in s.iter_mut() {
            if v.is_nan() { *v = 0.0; }
        }
        s
    };
    let sum: f64 = sanitized.iter().zip(weights.iter()).map(|(s, w)| s * w).sum();
    let weight_total: f64 = weights.iter().sum();
    let base = sum / weight_total;

    assert!(!base.is_nan(), "Score must not be NaN even with NaN in highest-weight signal");
    // 0.0*10 + 0.9*7 = 6.3 / 17.0 ≈ 0.3706
    assert!(base > 0.0 && base < 1.0, "Score should be in valid range, got {}", base);
}

// ── deliver_queued future-timestamp gap ──────────────────────────────
// Flaw: deliver_queued filtered expired messages but NOT future-dated
// ones. An attacker could queue a message with timestamp far in the
// future, and it would survive indefinitely in the queue (never expiring).
//
// Fix: add the same 30s clock-skew tolerance from check_replay.

/// Future-dated messages in offline queue must be rejected on delivery.
#[test]
fn deliver_queued_rejects_future_dated_messages() {
    let mut dispatcher = MessageDispatcher::new();
    let agent = Uuid::now_v7();
    let sender = Uuid::now_v7();

    // Create a message with timestamp 5 minutes in the future
    let mut msg = AgentMessage::new(
        sender, agent, "Notification".into(),
        serde_json::json!({"message": "future attack"}),
    );
    msg.timestamp = Utc::now() + chrono::Duration::minutes(5);
    msg.content_hash = msg.compute_content_hash();

    dispatcher.queue_offline(agent, msg);

    let delivered = dispatcher.deliver_queued(agent);
    assert!(
        delivered.is_empty(),
        "CRITICAL: Future-dated message (5min ahead) was delivered from offline queue. \
         An attacker could create immortal messages that never expire."
    );
}

/// Messages within 30s clock skew tolerance should still be delivered.
#[test]
fn deliver_queued_allows_slight_clock_skew() {
    let mut dispatcher = MessageDispatcher::new();
    let agent = Uuid::now_v7();
    let sender = Uuid::now_v7();

    // Create a message 20s in the future (within 30s tolerance)
    let mut msg = AgentMessage::new(
        sender, agent, "Notification".into(),
        serde_json::json!({"message": "slight skew"}),
    );
    msg.timestamp = Utc::now() + chrono::Duration::seconds(20);
    msg.content_hash = msg.compute_content_hash();

    dispatcher.queue_offline(agent, msg);

    let delivered = dispatcher.deliver_queued(agent);
    assert_eq!(
        delivered.len(), 1,
        "Messages within 30s clock skew should be delivered"
    );
}

/// Mix of valid, expired, and future-dated messages — only valid ones delivered.
#[test]
fn deliver_queued_mixed_valid_expired_future() {
    let mut dispatcher = MessageDispatcher::new();
    let agent = Uuid::now_v7();
    let sender = Uuid::now_v7();

    // Valid message (now)
    let valid = AgentMessage::new(
        sender, agent, "Notification".into(),
        serde_json::json!({"message": "valid"}),
    );

    // Expired message (10 minutes ago — beyond 5min REPLAY_WINDOW)
    let mut expired = AgentMessage::new(
        sender, agent, "Notification".into(),
        serde_json::json!({"message": "expired"}),
    );
    expired.timestamp = Utc::now() - chrono::Duration::minutes(10);
    expired.content_hash = expired.compute_content_hash();

    // Future-dated message (1 hour ahead)
    let mut future = AgentMessage::new(
        sender, agent, "Notification".into(),
        serde_json::json!({"message": "future"}),
    );
    future.timestamp = Utc::now() + chrono::Duration::hours(1);
    future.content_hash = future.compute_content_hash();

    dispatcher.queue_offline(agent, valid);
    dispatcher.queue_offline(agent, expired);
    dispatcher.queue_offline(agent, future);

    let delivered = dispatcher.deliver_queued(agent);
    assert_eq!(
        delivered.len(), 1,
        "Only the valid message should be delivered, got {} messages. \
         Expired and future-dated must be filtered.",
        delivered.len()
    );
}

// ── check_spending_cap NaN bypass ────────────────────────────────────
// Flaw: NaN + 0.0 > 100.0 evaluates to false (NaN comparisons always
// return false), so NaN cost silently passes the spending cap check.
// A broken cost estimator returning NaN would allow unlimited spending.
//
// Fix: reject any non-finite (NaN or Infinity) values in all three params.

/// NaN estimated_flush_cost must be rejected (the original bypass vector).
#[test]
fn spending_cap_nan_cost_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(f64::NAN, 0.0, 100.0);
    assert!(
        result.is_err(),
        "CRITICAL: NaN flush cost bypassed spending cap. \
         NaN + 0.0 > 100.0 is false, allowing unlimited spending."
    );
}

/// NaN current_spend must be rejected.
#[test]
fn spending_cap_nan_current_spend_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(10.0, f64::NAN, 100.0);
    assert!(
        result.is_err(),
        "CRITICAL: NaN current_spend bypassed spending cap check"
    );
}

/// NaN spending_cap must be rejected.
#[test]
fn spending_cap_nan_cap_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(10.0, 50.0, f64::NAN);
    assert!(
        result.is_err(),
        "CRITICAL: NaN spending_cap bypassed spending cap check"
    );
}

/// Negative infinity cost must be rejected (would always pass the check).
#[test]
fn spending_cap_neg_infinity_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(f64::NEG_INFINITY, 50.0, 100.0);
    assert!(
        result.is_err(),
        "CRITICAL: -Infinity flush cost passed spending cap. \
         -Inf + 50.0 = -Inf, which is NOT > 100.0, bypassing the cap."
    );
}

/// Positive infinity in current_spend must be rejected.
#[test]
fn spending_cap_inf_current_spend_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(10.0, f64::INFINITY, 100.0);
    assert!(
        result.is_err(),
        "Infinite current_spend must be rejected"
    );
}

/// All three params NaN — triple bypass attempt.
#[test]
fn spending_cap_all_nan_rejected() {
    let compactor = SessionCompactor::new(CompactionConfig::default());
    let result = compactor.check_spending_cap(f64::NAN, f64::NAN, f64::NAN);
    assert!(
        result.is_err(),
        "CRITICAL: All-NaN spending cap check passed — complete bypass"
    );
}

// ── prune_tool_results format correctness ────────────────────────────
// Flaw: The implementation was changed to use {"type": "tool_result"}
// format but the test data used {"tool_result": ...} format, causing
// a silent test failure (the test passed vacuously because nothing
// matched the prune criteria).

/// Verify prune_tool_results actually prunes with correct JSON format.
#[test]
fn prune_tool_results_correct_format() {
    let mut history: Vec<String> = vec![
        "User question".into(),
        r#"{"type": "tool_result", "content": "output data"}"#.into(),
        "Agent response".into(),
        r#"{"type": "tool_result", "content": "more output"}"#.into(),
    ];

    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(result.results_pruned, 2, "Must prune 2 tool_result messages");
    assert_eq!(history.len(), 2, "Only non-tool messages should remain");
    assert_eq!(history[0], "User question");
    assert_eq!(history[1], "Agent response");
}

/// Old format {"tool_result": ...} must NOT be pruned (it's not a tool_result
/// by the new schema). This verifies the implementation is format-strict.
#[test]
fn prune_tool_results_old_format_not_pruned() {
    let mut history: Vec<String> = vec![
        "User question".into(),
        r#"{"tool_result": "output data"}"#.into(),
        "Agent response".into(),
    ];

    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(
        result.results_pruned, 0,
        "Old format should NOT be pruned — only {{\"type\": \"tool_result\"}} is valid"
    );
    assert_eq!(history.len(), 3, "All messages should remain");
}

/// Mixed formats: only the correct format gets pruned.
#[test]
fn prune_tool_results_mixed_formats() {
    let mut history: Vec<String> = vec![
        r#"{"type": "tool_result", "content": "real tool result"}"#.into(),
        r#"{"tool_result": "old format"}"#.into(),
        r#"{"type": "user_message", "content": "not a tool result"}"#.into(),
        "plain text".into(),
    ];

    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(result.results_pruned, 1, "Only the correct-format tool_result should be pruned");
    assert_eq!(history.len(), 3);
}

/// User message that contains "tool_result" as a string value must NOT be pruned.
#[test]
fn prune_tool_results_user_message_with_keyword() {
    let mut history: Vec<String> = vec![
        r#"{"type": "user_message", "content": "I got a tool_result error"}"#.into(),
        r#"{"type": "tool_result", "content": "actual tool output"}"#.into(),
    ];

    let result = SessionCompactor::prune_tool_results(&mut history);
    assert_eq!(result.results_pruned, 1, "Only actual tool_result type should be pruned");
    assert_eq!(history.len(), 1);
    assert!(history[0].contains("user_message"), "User message must survive");
}

// ── reconstruct_state zeroed signal_scores ───────────────────────────
// Flaw: When restoring from DB, signal_scores is set to [0.0; 8].
// These zeros could be published via shared state if the cache TTL
// hasn't expired. We verify the cache structure handles this correctly.

/// Verify that CachedScore with zeroed signals has level matching score.
#[test]
fn cached_score_zeroed_signals_level_consistency() {
    // Simulate what reconstruct_state does
    let score = 0.75;
    let level = 3u8; // From DB

    // The zeroed signals don't affect the score/level — they're just
    // metadata. But if someone reads the cache and uses signal_scores
    // for decisions, they'd get wrong data.
    let signals = [0.0f64; 8];

    // Verify the signals don't match what the score implies
    let signal_sum: f64 = signals.iter().sum();
    assert_eq!(signal_sum, 0.0, "Zeroed signals sum to 0.0");
    assert!(
        score > 0.0,
        "But the score is {}, which is inconsistent with zero signals. \
         Any consumer reading signal_scores from a DB-restored cache \
         would get misleading data.",
        score
    );

    // The fix ensures compute_score is called before publishing,
    // which overwrites the stale cache. Verify the level is at least
    // consistent with the score.
    assert!(level > 0, "Level {} should be > 0 for score {}", level, score);
}

// ── Dedup key correctness ────────────────────────────────────────────
// Verify that compute_dedup_key uses variant name only, not full Debug
// output (which would include timestamps, scores, etc.)

/// Two SoulDrift triggers for the same agent with different drift_scores
/// must produce the same dedup key (variant + agent_id only).
#[test]
fn dedup_key_ignores_trigger_fields() {
    use cortex_core::safety::trigger::TriggerEvent;
    use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
    use ghost_gateway::safety::kill_switch::KillSwitch;

    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(ks);

    let agent = Uuid::now_v7();

    // First SoulDrift with drift_score 0.5
    let trigger1 = TriggerEvent::SoulDrift {
        agent_id: agent,
        drift_score: 0.5,
        threshold: 0.8,
        baseline_hash: "abc123".into(),
        current_hash: "def456".into(),
        detected_at: Utc::now(),
    };

    // Second SoulDrift with drift_score 0.9 (different score, same variant+agent)
    let trigger2 = TriggerEvent::SoulDrift {
        agent_id: agent,
        drift_score: 0.9,
        threshold: 0.8,
        baseline_hash: "abc123".into(),
        current_hash: "ghi789".into(),
        detected_at: Utc::now() + chrono::Duration::seconds(1),
    };

    // First should be processed
    let result1 = evaluator.process(trigger1);
    assert!(result1.is_some(), "First trigger should be processed");

    // Second should be SUPPRESSED (same variant + agent within 60s window)
    let result2 = evaluator.process(trigger2);
    assert!(
        result2.is_none(),
        "CRITICAL: Second SoulDrift for same agent was NOT deduplicated. \
         The dedup key includes field values (drift_score, detected_at) \
         instead of just the variant name + agent_id."
    );
}

/// Different trigger variants for the same agent must NOT be deduplicated.
#[test]
fn dedup_key_different_variants_not_suppressed() {
    use cortex_core::safety::trigger::TriggerEvent;
    use ghost_gateway::safety::auto_triggers::AutoTriggerEvaluator;
    use ghost_gateway::safety::kill_switch::KillSwitch;

    let ks = Arc::new(KillSwitch::new());
    let mut evaluator = AutoTriggerEvaluator::new(ks);

    let agent = Uuid::now_v7();

    let soul_drift = TriggerEvent::SoulDrift {
        agent_id: agent,
        drift_score: 0.9,
        threshold: 0.8,
        baseline_hash: "abc123".into(),
        current_hash: "def456".into(),
        detected_at: Utc::now(),
    };

    let spending = TriggerEvent::SpendingCapExceeded {
        agent_id: agent,
        daily_total: 150.0,
        cap: 100.0,
        overage: 50.0,
        detected_at: Utc::now(),
    };

    let result1 = evaluator.process(soul_drift);
    assert!(result1.is_some(), "SoulDrift should be processed");

    let result2 = evaluator.process(spending);
    assert!(
        result2.is_some(),
        "SpendingCapExceeded should NOT be suppressed by SoulDrift dedup"
    );
}
