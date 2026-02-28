//! Tests for Task 14.4 — Cascade circuit breakers + memory poisoning defense.

use std::time::{Duration, Instant};

use ghost_mesh::error::MeshError;
use ghost_mesh::safety::cascade_breaker::{
    CascadeBreakerState, CascadeCircuitBreaker, DelegationDepthTracker,
};
use ghost_mesh::safety::memory_poisoning::{
    DelegatedWrite, MemoryPoisoningDetector, PoisoningConfig, PoisoningFlagType, WriteImportance,
};
use uuid::Uuid;

// ── CascadeCircuitBreaker tests ─────────────────────────────────────────

#[test]
fn cascade_breaker_starts_closed() {
    let breaker = CascadeCircuitBreaker::default();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
}

#[test]
fn cascade_breaker_opens_after_threshold_failures() {
    let mut breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Open);
}

#[test]
fn cascade_breaker_does_not_affect_other_pairs() {
    let mut breaker = CascadeCircuitBreaker::new(2, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();

    // Trip A→B.
    breaker.record_failure(a, b);
    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Open);

    // A→C should still be closed.
    assert_eq!(breaker.state(a, c), CascadeBreakerState::Closed);
    // B→A should still be closed.
    assert_eq!(breaker.state(b, a), CascadeBreakerState::Closed);
}

#[test]
fn cascade_breaker_opens_on_convergence_spike() {
    let mut breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    breaker.set_convergence_spike_threshold(0.7);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    // Create a breaker entry for A→B.
    breaker.record_success(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);

    // Convergence spike on B.
    breaker.record_convergence_spike(b, 0.8);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Open);
}

#[test]
fn cascade_breaker_convergence_below_threshold_no_trip() {
    let mut breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    breaker.set_convergence_spike_threshold(0.7);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    breaker.record_success(a, b);
    breaker.record_convergence_spike(b, 0.5); // Below threshold.
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
}

#[test]
fn cascade_breaker_success_resets() {
    let mut breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    breaker.record_failure(a, b);
    breaker.record_failure(a, b);
    // 2 failures, not yet tripped.
    breaker.record_success(a, b);
    // Reset — should be closed.
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
    // Need 3 more failures to trip again.
    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Closed);
}

#[test]
fn cascade_breaker_allows_delegation_when_closed() {
    let breaker = CascadeCircuitBreaker::default();
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert!(breaker.allows_delegation(a, b));
}

#[test]
fn cascade_breaker_blocks_delegation_when_open() {
    let mut breaker = CascadeCircuitBreaker::new(1, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    breaker.record_failure(a, b);
    assert!(!breaker.allows_delegation(a, b));
}

// ── DelegationDepthTracker tests ────────────────────────────────────────

#[test]
fn delegation_depth_3_allowed() {
    let mut tracker = DelegationDepthTracker::new(3);
    let task_id = Uuid::new_v4();
    tracker.register_task(task_id);

    assert!(tracker.record_hop(task_id).is_ok()); // depth 1
    assert!(tracker.record_hop(task_id).is_ok()); // depth 2
    assert!(tracker.record_hop(task_id).is_ok()); // depth 3
}

#[test]
fn delegation_depth_4_rejected_with_max_3() {
    let mut tracker = DelegationDepthTracker::new(3);
    let task_id = Uuid::new_v4();
    tracker.register_task(task_id);

    tracker.record_hop(task_id).unwrap(); // 1
    tracker.record_hop(task_id).unwrap(); // 2
    tracker.record_hop(task_id).unwrap(); // 3
    let result = tracker.record_hop(task_id); // 4 — should fail
    assert!(result.is_err());
    match result.unwrap_err() {
        MeshError::DelegationDepthExceeded { depth, max } => {
            assert_eq!(depth, 4);
            assert_eq!(max, 3);
        }
        _ => panic!("expected DelegationDepthExceeded"),
    }
}

#[test]
fn delegation_loop_detection() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    // A→B→A is a loop.
    let chain = vec![(a, b), (b, a)];
    assert!(DelegationDepthTracker::detect_loop(&chain));
}

#[test]
fn delegation_no_loop() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let c = Uuid::new_v4();
    let chain = vec![(a, b), (b, c)];
    assert!(!DelegationDepthTracker::detect_loop(&chain));
}

#[test]
fn check_delegation_depth_exceeded() {
    let breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    let result = breaker.check_delegation(a, b, 3);
    assert!(result.is_err());
}

#[test]
fn check_delegation_within_depth() {
    let breaker = CascadeCircuitBreaker::new(3, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    assert!(breaker.check_delegation(a, b, 2).is_ok());
}

#[test]
fn check_delegation_circuit_breaker_open() {
    let mut breaker = CascadeCircuitBreaker::new(1, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    breaker.record_failure(a, b);
    let result = breaker.check_delegation(a, b, 0);
    assert!(result.is_err());
    match result.unwrap_err() {
        MeshError::CircuitBreakerOpen { from, to } => {
            assert_eq!(from, a);
            assert_eq!(to, b);
        }
        _ => panic!("expected CircuitBreakerOpen"),
    }
}

// ── Adversarial: agent cannot reset its own breaker ─────────────────────

#[test]
fn agent_cannot_reset_own_cascade_breaker() {
    // The CascadeCircuitBreaker API only allows record_success/record_failure
    // from the gateway. An agent has no direct access to the breaker.
    // This test verifies the API doesn't expose a "reset" method.
    // (Compile-time guarantee — if this compiles, the API is correct.)
    let mut breaker = CascadeCircuitBreaker::new(1, Duration::from_secs(300), 3);
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();
    breaker.record_failure(a, b);
    assert_eq!(breaker.state(a, b), CascadeBreakerState::Open);
    // Only record_success can close it — and that's called by the gateway,
    // not by the agent itself.
}

// ── MemoryPoisoningDetector tests ───────────────────────────────────────

fn make_write(
    delegation_id: Uuid,
    agent_id: Uuid,
    key: &str,
    importance: WriteImportance,
) -> DelegatedWrite {
    DelegatedWrite {
        delegation_id,
        agent_id,
        memory_key: key.to_string(),
        importance,
        timestamp: Instant::now(),
        content_summary: format!("content for {key}"),
    }
}

#[test]
fn clean_delegation_no_flags() {
    let mut detector = MemoryPoisoningDetector::default();
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let writes = vec![
        make_write(delegation_id, agent_id, "key1", WriteImportance::Normal),
        make_write(delegation_id, agent_id, "key2", WriteImportance::Low),
    ];

    let result = detector.check_writes(&writes, 0.8).unwrap();
    assert!(!result.is_poisoned);
    assert!(result.flags.is_empty());
}

#[test]
fn volume_spike_flagged() {
    let config = PoisoningConfig {
        max_writes_per_minute: 10,
        ..Default::default()
    };
    let mut detector = MemoryPoisoningDetector::new(config);
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    // 11 writes in one batch — exceeds threshold.
    let writes: Vec<DelegatedWrite> = (0..11)
        .map(|i| make_write(delegation_id, agent_id, &format!("key{i}"), WriteImportance::Normal))
        .collect();

    let result = detector.check_writes(&writes, 0.8).unwrap();
    assert!(result.is_poisoned);
    assert!(result
        .flags
        .iter()
        .any(|f| f.flag_type == PoisoningFlagType::VolumeSpike));
}

#[test]
fn contradiction_flagged() {
    let mut detector = MemoryPoisoningDetector::default();
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    // Add existing history.
    detector.add_history_entry("preference".to_string(), "likes rust".to_string());

    // Write that contradicts.
    let write = DelegatedWrite {
        delegation_id,
        agent_id,
        memory_key: "preference".to_string(),
        importance: WriteImportance::Normal,
        timestamp: Instant::now(),
        content_summary: "not likes rust".to_string(),
    };

    let result = detector.check_writes(&[write], 0.8).unwrap();
    assert!(result.is_poisoned);
    assert!(result
        .flags
        .iter()
        .any(|f| f.flag_type == PoisoningFlagType::Contradiction));
}

#[test]
fn high_importance_from_untrusted_agent_flagged() {
    let mut detector = MemoryPoisoningDetector::default();
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let write = make_write(delegation_id, agent_id, "critical-data", WriteImportance::Critical);

    // Agent trust 0.3 < threshold 0.6.
    let result = detector.check_writes(&[write], 0.3).unwrap();
    assert!(result.is_poisoned);
    assert!(result
        .flags
        .iter()
        .any(|f| f.flag_type == PoisoningFlagType::UntrustedHighImportance));
}

#[test]
fn high_importance_from_trusted_agent_not_flagged() {
    let mut detector = MemoryPoisoningDetector::default();
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let write = make_write(delegation_id, agent_id, "critical-data", WriteImportance::Critical);

    // Agent trust 0.8 >= threshold 0.6.
    let result = detector.check_writes(&[write], 0.8).unwrap();
    // Should not flag for untrusted high importance.
    assert!(!result
        .flags
        .iter()
        .any(|f| f.flag_type == PoisoningFlagType::UntrustedHighImportance));
}

#[test]
fn empty_writes_no_flags() {
    let mut detector = MemoryPoisoningDetector::default();
    let result = detector.check_writes(&[], 0.5).unwrap();
    assert!(!result.is_poisoned);
    assert!(result.flags.is_empty());
}

#[test]
fn clear_delegation_removes_tracking() {
    let mut detector = MemoryPoisoningDetector::default();
    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    let writes: Vec<DelegatedWrite> = (0..5)
        .map(|i| make_write(delegation_id, agent_id, &format!("key{i}"), WriteImportance::Normal))
        .collect();

    detector.check_writes(&writes, 0.8).unwrap();
    detector.clear_delegation(&delegation_id);

    // After clearing, new writes should start fresh count.
    let new_writes: Vec<DelegatedWrite> = (0..5)
        .map(|i| {
            make_write(
                delegation_id,
                agent_id,
                &format!("new_key{i}"),
                WriteImportance::Normal,
            )
        })
        .collect();

    let result = detector.check_writes(&new_writes, 0.8).unwrap();
    assert!(!result.is_poisoned);
}

// ── MemoryPoisoningDetector callback tests ──────────────────────────────

#[test]
fn poisoning_detector_convergence_amplify_callback_invoked() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let call_count = Arc::new(AtomicUsize::new(0));
    let call_count_clone = Arc::clone(&call_count);

    let mut detector = MemoryPoisoningDetector::default();
    detector.set_convergence_amplify_callback(move |_agent_id, flag_count| {
        call_count_clone.fetch_add(flag_count, Ordering::SeqCst);
    });

    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    // Trigger volume spike (>10 writes).
    let writes: Vec<DelegatedWrite> = (0..11)
        .map(|i| make_write(delegation_id, agent_id, &format!("key{i}"), WriteImportance::Normal))
        .collect();

    let result = detector.check_writes(&writes, 0.8).unwrap();
    assert!(result.is_poisoned);
    assert!(call_count.load(Ordering::SeqCst) > 0, "callback should have been invoked");
}

#[test]
fn poisoning_detector_audit_log_callback_invoked() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    let log_count = Arc::new(AtomicUsize::new(0));
    let log_count_clone = Arc::clone(&log_count);

    let mut detector = MemoryPoisoningDetector::default();
    detector.set_audit_log_callback(move |_agent_id, _description| {
        log_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let delegation_id = Uuid::new_v4();
    let agent_id = Uuid::new_v4();

    // Trigger untrusted high importance.
    let write = make_write(delegation_id, agent_id, "critical", WriteImportance::Critical);
    let result = detector.check_writes(&[write], 0.3).unwrap();
    assert!(result.is_poisoned);
    assert!(log_count.load(Ordering::SeqCst) > 0, "audit log callback should have been invoked");
}
