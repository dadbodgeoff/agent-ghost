//! Tests for itp-protocol: serialization, privacy, JSONL transport, adapter trait.

use chrono::Utc;
use uuid::Uuid;

use itp_protocol::adapter::ITPAdapter;
use itp_protocol::events::*;
use itp_protocol::privacy::*;
use itp_protocol::transport::jsonl::JsonlTransport;

// ── Helper ──────────────────────────────────────────────────────────────

fn sample_session_start() -> SessionStartEvent {
    SessionStartEvent {
        session_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        channel: "cli".to_string(),
        privacy_level: PrivacyLevel::Standard,
        timestamp: Utc::now(),
    }
}

fn sample_message(session_id: Uuid) -> InteractionMessageEvent {
    let (hash, plaintext) = apply_privacy("hello world", PrivacyLevel::Standard);
    InteractionMessageEvent {
        session_id,
        message_id: Uuid::new_v4(),
        sender: MessageSender::Human,
        content_hash: hash,
        content_plaintext: plaintext,
        token_count: 2,
        timestamp: Utc::now(),
    }
}

// ── Serialization round-trip tests ──────────────────────────────────────

#[test]
fn session_start_serializes_to_valid_json() {
    let event = ITPEvent::SessionStart(sample_session_start());
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("SessionStart"));
}

#[test]
fn session_end_serializes_to_valid_json() {
    let event = ITPEvent::SessionEnd(SessionEndEvent {
        session_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        reason: "user_exit".to_string(),
        message_count: 42,
        timestamp: Utc::now(),
    });
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("SessionEnd"));
}

#[test]
fn interaction_message_serializes() {
    let event = ITPEvent::InteractionMessage(sample_message(Uuid::new_v4()));
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("InteractionMessage"));
}

#[test]
fn agent_state_snapshot_serializes() {
    let event = ITPEvent::AgentStateSnapshot(AgentStateSnapshotEvent {
        session_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        memory_count: 100,
        goal_count: 5,
        convergence_score: 0.42,
        intervention_level: 1,
        timestamp: Utc::now(),
    });
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("AgentStateSnapshot"));
}

#[test]
fn convergence_alert_serializes() {
    let event = ITPEvent::ConvergenceAlert(ConvergenceAlertEvent {
        session_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        alert_type: "boundary_violation".to_string(),
        score: 0.85,
        level: 3,
        details: "emulation detected".to_string(),
        timestamp: Utc::now(),
    });
    let json = serde_json::to_string(&event).unwrap();
    assert!(json.contains("ConvergenceAlert"));
}

#[test]
fn all_event_types_round_trip() {
    let events = vec![
        ITPEvent::SessionStart(sample_session_start()),
        ITPEvent::SessionEnd(SessionEndEvent {
            session_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            reason: "done".to_string(),
            message_count: 1,
            timestamp: Utc::now(),
        }),
        ITPEvent::InteractionMessage(sample_message(Uuid::new_v4())),
        ITPEvent::AgentStateSnapshot(AgentStateSnapshotEvent {
            session_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            memory_count: 0,
            goal_count: 0,
            convergence_score: 0.0,
            intervention_level: 0,
            timestamp: Utc::now(),
        }),
        ITPEvent::ConvergenceAlert(ConvergenceAlertEvent {
            session_id: Uuid::new_v4(),
            agent_id: Uuid::new_v4(),
            alert_type: "test".to_string(),
            score: 0.5,
            level: 2,
            details: "test".to_string(),
            timestamp: Utc::now(),
        }),
    ];
    for event in &events {
        let json = serde_json::to_string(event).unwrap();
        let deserialized: ITPEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(event, &deserialized);
    }
}

// ── Privacy tests ───────────────────────────────────────────────────────

#[test]
fn minimal_privacy_hashes_content() {
    let (hash, plaintext) = apply_privacy("secret content", PrivacyLevel::Minimal);
    assert!(!hash.is_empty());
    assert!(plaintext.is_none(), "Minimal should not include plaintext");
}

#[test]
fn full_privacy_includes_plaintext() {
    let (hash, plaintext) = apply_privacy("visible content", PrivacyLevel::Full);
    assert!(!hash.is_empty());
    assert_eq!(plaintext.unwrap(), "visible content");
}

#[test]
fn content_hash_uses_sha256() {
    // SHA-256 of "hello" is well-known
    let hash = hash_content("hello");
    // SHA-256("hello") = 2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824
    assert_eq!(
        hash,
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn itp_crate_does_not_depend_on_blake3() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        !cargo_toml.contains("blake3"),
        "itp-protocol must NOT depend on blake3 (hash algorithm separation)"
    );
}

// ── ITPAdapter trait is object-safe ─────────────────────────────────────

#[test]
fn itp_adapter_is_object_safe() {
    // This compiles only if ITPAdapter is object-safe
    fn _assert_object_safe(_: Box<dyn ITPAdapter>) {}
}

// ── JSONL transport tests ───────────────────────────────────────────────

#[test]
fn jsonl_transport_creates_session_dir_and_writes() {
    let dir = tempfile::tempdir().unwrap();
    let transport = JsonlTransport::new(dir.path().to_path_buf());
    let start = sample_session_start();
    let session_id = start.session_id;
    transport.on_session_start(&start);

    let file_path = dir.path().join(session_id.to_string()).join("events.jsonl");
    assert!(file_path.exists(), "session JSONL file should be created");
    let content = std::fs::read_to_string(&file_path).unwrap();
    assert!(content.contains("SessionStart"));
}

#[test]
fn jsonl_transport_appends_not_overwrites() {
    let dir = tempfile::tempdir().unwrap();
    let transport = JsonlTransport::new(dir.path().to_path_buf());
    let start = sample_session_start();
    let session_id = start.session_id;

    transport.on_session_start(&start);
    transport.on_message(&sample_message(session_id));
    transport.on_message(&sample_message(session_id));

    let file_path = dir.path().join(session_id.to_string()).join("events.jsonl");
    let content = std::fs::read_to_string(&file_path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(lines.len(), 3, "should have 3 lines (1 start + 2 messages)");
}

#[test]
fn jsonl_write_100_events_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let transport = JsonlTransport::new(dir.path().to_path_buf());
    let start = sample_session_start();
    let session_id = start.session_id;

    transport.on_session_start(&start);
    for _ in 0..99 {
        transport.on_message(&sample_message(session_id));
    }

    let file_path = dir.path().join(session_id.to_string()).join("events.jsonl");
    let content = std::fs::read_to_string(&file_path).unwrap();
    let mut count = 0;
    for line in content.lines() {
        let _event: ITPEvent = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("failed to parse line {}: {}", count, e));
        count += 1;
    }
    assert_eq!(count, 100);
}

// ── Adversarial tests ───────────────────────────────────────────────────

#[test]
fn event_with_empty_session_id_serializes() {
    // Empty session_id is technically a valid UUID (nil)
    let event = ITPEvent::SessionStart(SessionStartEvent {
        session_id: Uuid::nil(),
        agent_id: Uuid::new_v4(),
        channel: "test".to_string(),
        privacy_level: PrivacyLevel::Minimal,
        timestamp: Utc::now(),
    });
    let json = serde_json::to_string(&event).unwrap();
    let _: ITPEvent = serde_json::from_str(&json).unwrap();
}

#[test]
fn event_with_future_timestamp_serializes() {
    let future = Utc::now() + chrono::Duration::minutes(10);
    let event = ITPEvent::SessionStart(SessionStartEvent {
        session_id: Uuid::new_v4(),
        agent_id: Uuid::new_v4(),
        channel: "test".to_string(),
        privacy_level: PrivacyLevel::Standard,
        timestamp: future,
    });
    // Should serialize fine — validation is the monitor's job
    let json = serde_json::to_string(&event).unwrap();
    let deserialized: ITPEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(event, deserialized);
}

#[test]
fn concurrent_writes_to_same_session_no_corruption() {
    let dir = tempfile::tempdir().unwrap();
    let session_id = Uuid::new_v4();

    // Spawn multiple threads writing to the same session file
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let path = dir.path().to_path_buf();
            let sid = session_id;
            std::thread::spawn(move || {
                let transport = JsonlTransport::new(path);
                for _ in 0..25 {
                    transport.on_message(&InteractionMessageEvent {
                        session_id: sid,
                        message_id: Uuid::new_v4(),
                        sender: MessageSender::Human,
                        content_hash: "abc".to_string(),
                        content_plaintext: None,
                        token_count: 1,
                        timestamp: Utc::now(),
                    });
                }
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }

    // Verify: all 100 lines parse correctly (no corruption)
    let file_path = dir.path().join(session_id.to_string()).join("events.jsonl");
    let content = std::fs::read_to_string(&file_path).unwrap();
    let mut count = 0;
    for line in content.lines() {
        let _: ITPEvent = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("corrupted line {}: {}", count, e));
        count += 1;
    }
    assert_eq!(count, 100, "should have 100 events from 4 threads × 25");
}

// ── Proptest ────────────────────────────────────────────────────────────

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    fn arb_privacy_level() -> impl Strategy<Value = PrivacyLevel> {
        prop_oneof![
            Just(PrivacyLevel::Minimal),
            Just(PrivacyLevel::Standard),
            Just(PrivacyLevel::Full),
            Just(PrivacyLevel::Research),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(500))]

        /// For 500 random ITP events across all 5 types, serialize then
        /// deserialize produces equivalent event (AC6 round-trip).
        #[test]
        fn all_event_types_round_trip_proptest(
            event_type in 0u8..5,
            channel in "[a-z]{3,10}",
            privacy in arb_privacy_level(),
            content in ".{0,200}",
            token_count in 0usize..10000,
            message_count in 0u64..10000,
            memory_count in 0u64..10000,
            goal_count in 0u64..100,
            score in 0.0f64..1.0,
            level in 0u8..5,
        ) {
            let event = match event_type {
                0 => ITPEvent::SessionStart(SessionStartEvent {
                    session_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    channel,
                    privacy_level: privacy,
                    timestamp: Utc::now(),
                }),
                1 => ITPEvent::SessionEnd(SessionEndEvent {
                    session_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    reason: channel,
                    message_count,
                    timestamp: Utc::now(),
                }),
                2 => {
                    let (hash, plaintext) = apply_privacy(&content, privacy);
                    ITPEvent::InteractionMessage(InteractionMessageEvent {
                        session_id: Uuid::new_v4(),
                        message_id: Uuid::new_v4(),
                        sender: MessageSender::Human,
                        content_hash: hash,
                        content_plaintext: plaintext,
                        token_count,
                        timestamp: Utc::now(),
                    })
                }
                3 => ITPEvent::AgentStateSnapshot(AgentStateSnapshotEvent {
                    session_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    memory_count,
                    goal_count,
                    convergence_score: score,
                    intervention_level: level,
                    timestamp: Utc::now(),
                }),
                _ => ITPEvent::ConvergenceAlert(ConvergenceAlertEvent {
                    session_id: Uuid::new_v4(),
                    agent_id: Uuid::new_v4(),
                    alert_type: channel,
                    score,
                    level,
                    details: content,
                    timestamp: Utc::now(),
                }),
            };

            let json = serde_json::to_string(&event).unwrap();
            let deserialized: ITPEvent = serde_json::from_str(&json).unwrap();
            // Verify structural equivalence — re-serialize the deserialized
            // event and parse both to serde_json::Value for comparison.
            // This handles f64 precision differences in JSON round-trip.
            let val1: serde_json::Value = serde_json::from_str(&json).unwrap();
            let json2 = serde_json::to_string(&deserialized).unwrap();
            let val2: serde_json::Value = serde_json::from_str(&json2).unwrap();
            prop_assert_eq!(&val1, &val2, "JSON round-trip should be stable");
        }
    }
}
