//! Tests for ghost-export (Task 6.3).

use ghost_export::parsers::{self, ExportParser};
use ghost_export::timeline::TimelineReconstructor;
use ghost_export::analyzer::ExportAnalyzer;
use ghost_export::{ExportAnalysisResult, MessageRole, NormalizedMessage};
use tempfile::TempDir;
use std::fs;

fn make_chatgpt_export() -> String {
    serde_json::json!([{
        "id": "conv-1",
        "mapping": {
            "node-1": {
                "message": {
                    "author": {"role": "user"},
                    "content": {"parts": ["Hello, how are you?"]},
                    "create_time": 1700000000.0
                }
            },
            "node-2": {
                "message": {
                    "author": {"role": "assistant"},
                    "content": {"parts": ["I'm doing well, thanks!"]},
                    "create_time": 1700000060.0
                }
            }
        }
    }])
    .to_string()
}

fn make_character_ai_export() -> String {
    serde_json::json!({
        "histories": [{
            "external_id": "hist-1",
            "msgs": [
                {"src": {"is_human": true}, "text": "Hi there"},
                {"src": {"is_human": false}, "text": "Hello!"}
            ]
        }]
    })
    .to_string()
}

#[test]
fn chatgpt_parser_detects_valid_export() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("chatgpt.json");
    fs::write(&path, make_chatgpt_export()).unwrap();

    let parser = parsers::chatgpt::ChatGptParser;
    assert!(parser.detect(&path));
}

#[test]
fn chatgpt_parser_parses_messages() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("chatgpt.json");
    fs::write(&path, make_chatgpt_export()).unwrap();

    let parser = parsers::chatgpt::ChatGptParser;
    let messages = parser.parse(&path).unwrap();
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].sender, MessageRole::Human);
    assert_eq!(messages[1].sender, MessageRole::Assistant);
}

#[test]
fn character_ai_parser_detects_and_parses() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("cai.json");
    fs::write(&path, make_character_ai_export()).unwrap();

    let parser = parsers::character_ai::CharacterAiParser;
    assert!(parser.detect(&path));
    let messages = parser.parse(&path).unwrap();
    assert_eq!(messages.len(), 2);
}

#[test]
fn each_parser_returns_valid_itp_events() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("chatgpt.json");
    fs::write(&path, make_chatgpt_export()).unwrap();

    let parser = parsers::chatgpt::ChatGptParser;
    let messages = parser.parse(&path).unwrap();
    for msg in &messages {
        assert!(!msg.content.is_empty());
        assert!(msg.sender == MessageRole::Human || msg.sender == MessageRole::Assistant);
    }
}

#[test]
fn timeline_reconstructor_infers_session_boundaries() {
    let base = chrono::Utc::now();
    let messages = vec![
        NormalizedMessage {
            timestamp: base,
            sender: MessageRole::Human,
            content: "msg1".into(),
            session_id: None,
        },
        NormalizedMessage {
            timestamp: base + chrono::Duration::seconds(60),
            sender: MessageRole::Assistant,
            content: "msg2".into(),
            session_id: None,
        },
        // Gap > 1 hour → new session
        NormalizedMessage {
            timestamp: base + chrono::Duration::seconds(7200),
            sender: MessageRole::Human,
            content: "msg3".into(),
            session_id: None,
        },
    ];

    let reconstructor = TimelineReconstructor::default();
    let sessions = reconstructor.reconstruct(&messages);
    assert_eq!(sessions.len(), 2);
    assert_eq!(sessions[0].message_count, 2);
    assert_eq!(sessions[1].message_count, 1);
}

#[test]
fn analysis_result_serializes_to_json() {
    let result = ExportAnalysisResult {
        source_format: "ChatGPT".into(),
        total_messages: 100,
        total_sessions: 5,
        per_session_scores: Vec::new(),
        recommended_level: 1,
        flagged_sessions: Vec::new(),
    };
    let json = serde_json::to_string(&result).unwrap();
    assert!(json.contains("ChatGPT"));
}

#[test]
fn full_pipeline_chatgpt() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("chatgpt.json");
    fs::write(&path, make_chatgpt_export()).unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path).unwrap();
    assert_eq!(result.source_format, "ChatGPT");
    assert!(result.total_messages > 0);
}

#[test]
fn malformed_export_graceful_error() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("bad.json");
    fs::write(&path, "not valid json at all {{{").unwrap();

    let analyzer = ExportAnalyzer::new();
    let result = analyzer.analyze(&path);
    assert!(result.is_err());
}

#[test]
fn empty_export_empty_result() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("empty.jsonl");
    fs::write(&path, "").unwrap();

    let parser = parsers::jsonl::JsonlParser;
    let messages = parser.parse(&path).unwrap();
    assert!(messages.is_empty());
}
