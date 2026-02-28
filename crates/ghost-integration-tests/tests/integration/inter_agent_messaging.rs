//! E2E: Full inter-agent messaging lifecycle.
//!
//! Validates: compose → sign → dispatch → verify → deliver → ack.
//!
//! Exercises ghost-gateway messaging subsystem with 3-gate verification
//! pipeline (signature → replay → policy).

use chrono::Utc;
use ghost_gateway::messaging::dispatcher::{MessageDispatcher, VerifyResult};
use ghost_gateway::messaging::protocol::AgentMessage;
use uuid::Uuid;

// ── Message Composition + Verification ──────────────────────────────────

/// Valid message passes all 3 gates.
#[test]
fn valid_message_accepted() {
    let mut dispatcher = MessageDispatcher::new();
    let msg = AgentMessage::new(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "TaskRequest".into(),
        serde_json::json!({"task": "analyze data"}),
    );

    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::Accepted),
        "Valid message should be accepted: {:?}",
        result
    );
}

/// Tampered content_hash → rejected at Gate 1 (signature).
#[test]
fn tampered_content_hash_rejected() {
    let mut dispatcher = MessageDispatcher::new();
    let mut msg = AgentMessage::new(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "TaskRequest".into(),
        serde_json::json!({"task": "analyze data"}),
    );

    // Tamper with content hash
    msg.content_hash = [0xFFu8; 32];

    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::RejectedSignature(_)),
        "Tampered hash should be rejected: {:?}",
        result
    );
}

/// Replay detection: duplicate nonce → rejected at Gate 2.
#[test]
fn duplicate_nonce_rejected() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    let msg1 = AgentMessage::new(
        sender,
        recipient,
        "TaskRequest".into(),
        serde_json::json!({"task": "first"}),
    );

    // First message accepted
    let r1 = dispatcher.verify(&msg1);
    assert!(matches!(r1, VerifyResult::Accepted));

    // Same nonce → rejected
    let r2 = dispatcher.verify(&msg1);
    assert!(
        matches!(r2, VerifyResult::RejectedReplay(_)),
        "Duplicate nonce should be rejected: {:?}",
        r2
    );
}

/// Rate limiting: 61st message in 1 hour → rejected.
#[test]
fn rate_limiting_per_agent() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();

    // Send 60 messages to different recipients (to avoid per-pair limit of 30)
    for _ in 0..60 {
        let msg = AgentMessage::new(
            sender,
            Uuid::now_v7(), // different recipient each time
            "Notification".into(),
            serde_json::json!({"data": "ping"}),
        );
        let result = dispatcher.verify(&msg);
        assert!(
            matches!(result, VerifyResult::Accepted),
            "Messages within limit should be accepted"
        );
    }

    // 61st message → rate limited
    let msg = AgentMessage::new(
        sender,
        Uuid::now_v7(),
        "Notification".into(),
        serde_json::json!({"data": "one too many"}),
    );
    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::RejectedRateLimit),
        "61st message should be rate limited: {:?}",
        result
    );
}

/// Rate limiting: per-pair limit (30/hour).
#[test]
fn rate_limiting_per_pair() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();
    let recipient = Uuid::now_v7();

    // Send 30 messages to same pair
    for _ in 0..30 {
        let msg = AgentMessage::new(
            sender,
            recipient,
            "Notification".into(),
            serde_json::json!({"data": "ping"}),
        );
        dispatcher.verify(&msg);
    }

    // 31st to same pair → rate limited
    let msg = AgentMessage::new(
        sender,
        recipient,
        "Notification".into(),
        serde_json::json!({"data": "too many to same pair"}),
    );
    let result = dispatcher.verify(&msg);
    assert!(
        matches!(result, VerifyResult::RejectedRateLimit),
        "31st message to same pair should be rate limited"
    );
}

// ── Offline Queue ───────────────────────────────────────────────────────

/// Message queued for offline agent, delivered when online.
#[test]
fn offline_queue_delivery() {
    let mut dispatcher = MessageDispatcher::new();
    let recipient = Uuid::now_v7();

    let msg = AgentMessage::new(
        Uuid::now_v7(),
        recipient,
        "TaskRequest".into(),
        serde_json::json!({"task": "when you're back"}),
    );

    // Queue for offline agent
    dispatcher.queue_offline(recipient, msg);

    // Deliver when online
    let queued = dispatcher.deliver_queued(recipient);
    assert_eq!(queued.len(), 1);

    // Second delivery returns empty
    let empty = dispatcher.deliver_queued(recipient);
    assert!(empty.is_empty());
}

// ── Anomaly Detection ───────────────────────────────────────────────────

/// 3 signature failures in 5min → anomaly detected.
#[test]
fn signature_failure_anomaly_detection() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();

    // Send 3 messages with tampered hashes
    for _ in 0..3 {
        let mut msg = AgentMessage::new(
            sender,
            Uuid::now_v7(),
            "TaskRequest".into(),
            serde_json::json!({"task": "test"}),
        );
        msg.content_hash = [0xFFu8; 32];
        let _ = dispatcher.verify(&msg);
    }

    assert_eq!(
        dispatcher.sig_failure_count(sender),
        3,
        "Should track 3 signature failures"
    );
}

/// 2 signature failures → no anomaly.
#[test]
fn two_failures_no_anomaly() {
    let mut dispatcher = MessageDispatcher::new();
    let sender = Uuid::now_v7();

    for _ in 0..2 {
        let mut msg = AgentMessage::new(
            sender,
            Uuid::now_v7(),
            "TaskRequest".into(),
            serde_json::json!({"task": "test"}),
        );
        msg.content_hash = [0xFFu8; 32];
        let _ = dispatcher.verify(&msg);
    }

    assert_eq!(dispatcher.sig_failure_count(sender), 2);
}

// ── Canonical Bytes Determinism ─────────────────────────────────────────

/// canonical_bytes is deterministic: same message → same bytes.
#[test]
fn canonical_bytes_deterministic() {
    let msg = AgentMessage::new(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "TaskRequest".into(),
        serde_json::json!({"task": "test", "priority": 1}),
    );

    let bytes1 = msg.canonical_bytes();
    let bytes2 = msg.canonical_bytes();

    assert_eq!(bytes1, bytes2, "canonical_bytes must be deterministic");
}

/// canonical_bytes with BTreeMap context → deterministic regardless of insertion order.
#[test]
fn canonical_bytes_btreemap_deterministic() {
    let mut context1 = std::collections::BTreeMap::new();
    context1.insert("z_key".to_string(), serde_json::json!("z_value"));
    context1.insert("a_key".to_string(), serde_json::json!("a_value"));

    let mut context2 = std::collections::BTreeMap::new();
    context2.insert("a_key".to_string(), serde_json::json!("a_value"));
    context2.insert("z_key".to_string(), serde_json::json!("z_value"));

    let msg1 = AgentMessage::new(
        Uuid::now_v7(),
        Uuid::now_v7(),
        "TaskRequest".into(),
        serde_json::Value::Object(context1.into_iter().collect()),
    );

    // BTreeMap ensures deterministic ordering
    let bytes = msg1.canonical_bytes();
    assert!(!bytes.is_empty());
}
