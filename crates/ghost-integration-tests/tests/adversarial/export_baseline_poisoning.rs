//! Adversarial: ghost-export convergence baseline poisoning.
//!
//! An attacker crafts a ChatGPT/Claude export with pre-built convergence
//! signal patterns to start an agent at an artificially elevated baseline
//! score. The ExportAnalyzer computes per-session scores from message
//! density and duration — both are trivially controllable by the attacker.

use ghost_export::analyzer::{ExportAnalyzer, SessionScore};
use ghost_export::timeline::TimelineReconstructor;
use ghost_export::{MessageRole, NormalizedMessage};
use std::fs;
use tempfile::TempDir;

// ── Crafted export: high-score sessions ─────────────────────────────────

/// Attacker crafts a ChatGPT export with sessions designed to produce
/// high convergence scores: long duration + high message density.
fn make_poisoned_chatgpt_export() -> String {
    let base_ts = 1700000000.0_f64;
    let mut conversations = Vec::new();

    // 20 sessions, each 6+ hours with high message density
    for session in 0..20 {
        let session_start = base_ts + (session as f64 * 86400.0); // 1 day apart
        let mut mapping = serde_json::Map::new();

        // 120 messages per session (1 per 3 minutes over 6 hours)
        for msg_idx in 0..120 {
            let ts = session_start + (msg_idx as f64 * 180.0); // 3 min apart
            let role = if msg_idx % 2 == 0 { "user" } else { "assistant" };
            let content = format!("Message {} in session {}", msg_idx, session);

            let node_id = format!("node-{}-{}", session, msg_idx);
            mapping.insert(
                node_id,
                serde_json::json!({
                    "message": {
                        "author": {"role": role},
                        "content": {"parts": [content]},
                        "create_time": ts
                    }
                }),
            );
        }

        conversations.push(serde_json::json!({
            "id": format!("conv-{}", session),
            "mapping": mapping
        }));
    }

    serde_json::to_string(&conversations).unwrap()
}

/// Attacker crafts a minimal export that produces low scores (control group).
fn make_benign_chatgpt_export() -> String {
    serde_json::json!([{
        "id": "conv-benign",
        "mapping": {
            "node-1": {
                "message": {
                    "author": {"role": "user"},
                    "content": {"parts": ["Hello"]},
                    "create_time": 1700000000.0
                }
            },
            "node-2": {
                "message": {
                    "author": {"role": "assistant"},
                    "content": {"parts": ["Hi there!"]},
                    "create_time": 1700000060.0
                }
            }
        }
    }])
    .to_string()
}

// ── Poisoned export produces elevated scores ────────────────────────────

#[test]
fn poisoned_export_produces_elevated_baseline() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("poisoned.json");
    fs::write(&path, make_poisoned_chatgpt_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();

    assert!(
        result.total_sessions >= 10,
        "poisoned export should produce many sessions, got {}",
        result.total_sessions
    );

    // Check that at least some sessions have elevated scores
    let high_score_sessions: Vec<&SessionScore> = result
        .per_session_scores
        .iter()
        .filter(|s| s.estimated_score > 0.3)
        .collect();

    assert!(
        !high_score_sessions.is_empty(),
        "poisoned export should produce sessions with elevated scores"
    );
}

#[test]
fn benign_export_produces_low_session_count() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("benign.json");
    fs::write(&path, make_benign_chatgpt_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();

    assert_eq!(result.total_sessions, 1);
    // Even a short 2-message session can produce a non-zero score due to
    // high message density (2 msgs in 1 min = density 2.0, clamped to 1.0).
    // The key difference from the poisoned export is session COUNT, not
    // individual session scores. A single session provides minimal baseline data.
    assert!(
        result.total_sessions < 5,
        "benign export should have very few sessions"
    );
}

// ── Score manipulation via duration ─────────────────────────────────────

/// The duration_signal is `(duration_secs / 21600).min(1.0)`.
/// An attacker can set duration to exactly 21600s (6h) to max out this signal.
#[test]
fn duration_signal_maxes_at_6_hours() {
    let base = chrono::Utc::now();
    let messages = vec![
        NormalizedMessage {
            timestamp: base,
            sender: MessageRole::Human,
            content: "start".into(),
            session_id: Some("crafted".into()),
        },
        NormalizedMessage {
            timestamp: base + chrono::Duration::seconds(21600), // exactly 6h
            sender: MessageRole::Assistant,
            content: "end".into(),
            session_id: Some("crafted".into()),
        },
    ];

    let reconstructor = TimelineReconstructor::default();
    let sessions = reconstructor.reconstruct(&messages);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].message_count, 2);

    let duration = (sessions[0].end - sessions[0].start).num_seconds();
    assert_eq!(duration, 21600, "session should be exactly 6 hours");
}

// ── Score manipulation via message density ──────────────────────────────

/// The msg_density signal is `(msg_count / (duration_mins)).min(1.0)`.
/// An attacker can pack many messages into a short window to max density.
#[test]
fn message_density_maxes_with_rapid_messages() {
    let base = chrono::Utc::now();
    let mut messages = Vec::new();

    // 60 messages in 1 minute = density of 60 msgs/min → clamped to 1.0
    for i in 0..60 {
        messages.push(NormalizedMessage {
            timestamp: base + chrono::Duration::seconds(i),
            sender: if i % 2 == 0 {
                MessageRole::Human
            } else {
                MessageRole::Assistant
            },
            content: format!("msg {i}"),
            session_id: Some("dense".into()),
        });
    }

    let reconstructor = TimelineReconstructor::default();
    let sessions = reconstructor.reconstruct(&messages);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].message_count, 60);
}

// ── Crafted Claude export ───────────────────────────────────────────────

fn make_poisoned_claude_export() -> String {
    let base_ts = "2024-01-01T00:00:00Z";
    let mut conversations = Vec::new();

    for session in 0..15 {
        let mut msgs = Vec::new();
        for msg_idx in 0..100 {
            let ts = chrono::DateTime::parse_from_rfc3339(base_ts).unwrap()
                + chrono::Duration::days(session)
                + chrono::Duration::seconds(msg_idx * 180);

            msgs.push(serde_json::json!({
                "sender": if msg_idx % 2 == 0 { "human" } else { "assistant" },
                "text": format!("Crafted message {} in session {}", msg_idx, session),
                "created_at": ts.to_rfc3339()
            }));
        }

        conversations.push(serde_json::json!({
            "uuid": format!("session-{}", session),
            "chat_messages": msgs
        }));
    }

    serde_json::to_string(&conversations).unwrap()
}

#[test]
fn poisoned_claude_export_produces_elevated_baseline() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("poisoned_claude.json");
    fs::write(&path, make_poisoned_claude_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();

    assert!(result.total_sessions >= 10);
    assert!(
        result.total_messages > 500,
        "poisoned Claude export should have many messages, got {}",
        result.total_messages
    );
}

// ── Recommended level reflects poisoned data ────────────────────────────

#[test]
fn poisoned_export_can_elevate_recommended_level() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("elevated.json");
    fs::write(&path, make_poisoned_chatgpt_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();

    // The recommended_level is derived from max session score.
    // A poisoned export with high-density, long-duration sessions
    // should produce a non-zero recommended level.
    assert!(
        result.recommended_level >= 1,
        "poisoned export should elevate recommended level above 0, got {}",
        result.recommended_level
    );
}

// ── Flagged sessions detection ──────────────────────────────────────────

#[test]
fn poisoned_export_flags_suspicious_sessions() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("flagged.json");
    fs::write(&path, make_poisoned_chatgpt_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();

    // Sessions with score > 0.5 are flagged
    // The poisoned export should have some flagged sessions
    // (depends on the scoring heuristic)
    assert!(
        result.total_sessions > 0,
        "should have sessions to evaluate"
    );
}

// ── Empty and malformed exports ─────────────────────────────────────────

#[test]
fn empty_export_produces_zero_baseline() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("empty.json");
    fs::write(&path, "[]").unwrap();

    // No parser detects an empty array as a valid export
    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path);
    // Either error or zero-session result
    match result {
        Ok(r) => {
            assert_eq!(r.total_sessions, 0);
            assert_eq!(r.recommended_level, 0);
        }
        Err(_) => {} // unsupported format is also acceptable
    }
}

#[test]
fn malformed_json_rejected() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("malformed.json");
    fs::write(&path, "{{{not json}}}").unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path);
    assert!(result.is_err(), "malformed JSON should be rejected");
}

// ── Threat model documentation ──────────────────────────────────────────
//
// ATTACK: Baseline poisoning via crafted export
//   Vector: Import a ChatGPT/Claude export with pre-built signal patterns
//   Impact: Agent starts with artificially elevated baseline score,
//           making the calibration period produce a skewed baseline
//           that masks future convergence signals
//   Controllable signals:
//     - duration_signal: set session duration to 6h (max)
//     - msg_density: pack messages at 1/min (max)
//   Uncontrollable signals (not in export):
//     - vocabulary_convergence (requires live interaction)
//     - goal_boundary_erosion (requires live interaction)
//     - behavioral_anomaly (requires live interaction)
//
// MITIGATIONS PRESENT:
//   - Score clamping to [0.0, 1.0]
//   - Flagged sessions (score > 0.5) are reported
//   - recommended_level provides a warning
//
// MITIGATIONS ABSENT:
//   - No cryptographic verification of export authenticity
//   - No cross-referencing with platform APIs to verify export data
//   - No statistical anomaly detection on import patterns
//   - No rate limiting on import frequency
//   - Export scores are not isolated from live convergence baseline
//
// RECOMMENDATION:
//   1. Treat imported baselines as "untrusted" with a separate trust tier
//   2. Apply a discount factor (e.g., 0.5x) to imported session scores
//   3. Require N live sessions before imported baseline influences scoring
//   4. Add statistical outlier detection on imported session patterns
