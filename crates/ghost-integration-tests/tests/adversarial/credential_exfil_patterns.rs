//! Adversarial: Credential exfiltration pattern detection (Task 7.3).
//!
//! Known credential patterns, encoding tricks, partial leaks,
//! and false positive avoidance.

use ghost_agent_loop::output_inspector::{InspectionResult, OutputInspector};
use uuid::Uuid;

fn agent_id() -> Uuid {
    Uuid::now_v7()
}

// ── Known credential patterns ───────────────────────────────────────────

#[test]
fn detects_aws_access_key() {
    let inspector = OutputInspector::new();
    let output = "Here's the key: AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "AWS access key pattern must be detected"
    );
}

#[test]
fn detects_github_personal_access_token() {
    let inspector = OutputInspector::new();
    let output = "Token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef1234";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "GitHub PAT must be detected"
    );
}

#[test]
fn detects_openai_api_key() {
    let inspector = OutputInspector::new();
    let output = "api_key = \"sk-proj-abcdefghijklmnopqrstuvwxyz123456\"";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "OpenAI API key must be detected"
    );
}

#[test]
fn detects_anthropic_api_key() {
    let inspector = OutputInspector::new();
    let output = "Key: sk-ant-abcdefghijklmnopqrstuvwxyz";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "Anthropic API key must be detected"
    );
}

#[test]
fn detects_pem_private_key() {
    let inspector = OutputInspector::new();
    let output = "-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "PEM private key must be detected"
    );
}

#[test]
fn detects_ec_private_key() {
    let inspector = OutputInspector::new();
    let output = "-----BEGIN EC PRIVATE KEY-----\nMHQCAQEE...";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "EC private key must be detected"
    );
}

// ── Warning vs KillAll distinction ──────────────────────────────────────

#[test]
fn unknown_credential_produces_warning_with_redaction() {
    let inspector = OutputInspector::new();
    let output = "My key is AKIAIOSFODNN7EXAMPLE and more text";
    let result = inspector.scan(output, agent_id());
    if let InspectionResult::Warning { redacted_text, .. } = result {
        assert!(
            redacted_text.contains("[REDACTED]"),
            "Warning must contain redacted text"
        );
        assert!(
            !redacted_text.contains("AKIAIOSFODNN7EXAMPLE"),
            "Redacted text must not contain the credential"
        );
    } else {
        panic!("Expected Warning result for unknown credential");
    }
}

#[test]
fn known_credential_produces_kill_all() {
    let mut inspector = OutputInspector::new();
    inspector.register_credential("AKIAIOSF".to_string());
    let output = "AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan(output, agent_id());
    assert!(
        matches!(result, InspectionResult::KillAll { .. }),
        "Known credential must produce KillAll"
    );
}

// ── False positive avoidance ────────────────────────────────────────────

#[test]
fn normal_text_not_flagged() {
    let inspector = OutputInspector::new();
    let output = "The weather today is sunny with a high of 72 degrees.";
    let result = inspector.scan(output, agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

#[test]
fn code_without_credentials_not_flagged() {
    let inspector = OutputInspector::new();
    let output = r#"fn main() { println!("Hello, world!"); }"#;
    let result = inspector.scan(output, agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

#[test]
fn short_sk_prefix_not_flagged() {
    let inspector = OutputInspector::new();
    // "sk-" alone is too short to be a real key
    let output = "The variable sk-1 is used for indexing.";
    let result = inspector.scan(output, agent_id());
    assert!(
        matches!(result, InspectionResult::Clean),
        "Short sk- prefix must not be flagged"
    );
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn empty_output_clean() {
    let inspector = OutputInspector::new();
    let result = inspector.scan("", agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

#[test]
fn very_long_output_no_panic() {
    let inspector = OutputInspector::new();
    let output = "a".repeat(1_000_000);
    let result = inspector.scan(&output, agent_id());
    assert!(matches!(result, InspectionResult::Clean));
}

#[test]
fn multiple_credentials_in_one_output() {
    let inspector = OutputInspector::new();
    let output = "Keys: AKIAIOSFODNN7EXAMPLE and sk-proj-abcdef123456";
    let result = inspector.scan(output, agent_id());
    assert!(
        !matches!(result, InspectionResult::Clean),
        "Multiple credentials must be detected"
    );
}
