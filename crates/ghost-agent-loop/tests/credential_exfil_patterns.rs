//! Adversarial test suite: Credential exfiltration pattern detection.
//!
//! Tests known credential patterns, encoding tricks, and edge cases
//! against the OutputInspector's credential detection.

use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use uuid::Uuid;

fn test_agent_id() -> Uuid {
    Uuid::now_v7()
}

// ── Known credential patterns ───────────────────────────────────────────

#[test]
fn detects_aws_access_key() {
    let inspector = OutputInspector::new();
    let output = "Here's the key: AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "AWS access key pattern should be detected"
    );
}

#[test]
fn detects_github_token() {
    let inspector = OutputInspector::new();
    let output = "Token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "GitHub personal access token should be detected"
    );
}

#[test]
fn detects_openai_api_key() {
    let inspector = OutputInspector::new();
    let output = "api_key = \"sk-proj-abcdefghijklmnopqrstuvwxyz123456\"";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "OpenAI API key pattern should be detected"
    );
}

#[test]
fn detects_private_key_pem() {
    let inspector = OutputInspector::new();
    let output = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "PEM private key should be detected"
    );
}

#[test]
fn detects_anthropic_api_key() {
    let inspector = OutputInspector::new();
    let output = "Key: sk-ant-abcdefghijklmnopqrstuvwxyz";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "Anthropic API key should be detected"
    );
}

// ── Warning vs KillAll distinction ──────────────────────────────────────

#[test]
fn unknown_credential_produces_warning() {
    let inspector = OutputInspector::new();
    let output = "AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        matches!(result, InspectionResult::Warning { .. }),
        "Unknown credential should produce Warning (not KillAll)"
    );
}

#[test]
fn known_credential_produces_kill_all() {
    let mut inspector = OutputInspector::new();
    inspector.register_credential("AKIAIOSF".to_string());
    let output = "AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        matches!(result, InspectionResult::KillAll { .. }),
        "Known credential should produce KillAll"
    );
}

// ── Safe content (no false positives) ───────────────────────────────────

#[test]
fn normal_text_not_flagged() {
    let inspector = OutputInspector::new();
    let output = "The weather today is sunny with a high of 72 degrees.";
    let result = inspector.scan(output, test_agent_id());
    assert!(
        matches!(result, InspectionResult::Clean),
        "Normal text should not be flagged as credential"
    );
}

#[test]
fn code_without_credentials_not_flagged() {
    let inspector = OutputInspector::new();
    let output = r#"fn main() { println!("Hello, world!"); }"#;
    let result = inspector.scan(output, test_agent_id());
    assert!(
        matches!(result, InspectionResult::Clean),
        "Normal code should not be flagged"
    );
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn empty_output_clean() {
    let inspector = OutputInspector::new();
    let result = inspector.scan("", test_agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

#[test]
fn very_long_output_no_panic() {
    let inspector = OutputInspector::new();
    let output = "a".repeat(1_000_000);
    let result = inspector.scan(&output, test_agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

// ── Redaction in Warning ────────────────────────────────────────────────

#[test]
fn warning_contains_redacted_text() {
    let inspector = OutputInspector::new();
    let output = "My key is AKIAIOSFODNN7EXAMPLE and more text";
    let result = inspector.scan(output, test_agent_id());
    if let InspectionResult::Warning { redacted_text, .. } = result {
        assert!(
            redacted_text.contains("[REDACTED]"),
            "Warning should contain redacted text"
        );
        assert!(
            !redacted_text.contains("AKIAIOSFODNN7EXAMPLE"),
            "Redacted text should not contain the credential"
        );
    } else {
        panic!("Expected Warning result");
    }
}
