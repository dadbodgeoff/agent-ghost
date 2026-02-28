//! Adversarial test suite: Credential exfiltration pattern detection.
//!
//! Tests known credential patterns, encoding tricks, and partial leaks
//! against the output inspector's credential detection.

use ghost_agent_loop::tools::output_inspector::OutputInspector;

// ── Known credential patterns ───────────────────────────────────────────

#[test]
fn detects_aws_access_key() {
    let inspector = OutputInspector::new();
    let output = "Here's the key: AKIAIOSFODNN7EXAMPLE";
    let result = inspector.scan_for_credentials(output);
    assert!(
        result.has_credentials,
        "AWS access key pattern should be detected"
    );
}

#[test]
fn detects_aws_secret_key() {
    let inspector = OutputInspector::new();
    let output = "Secret: wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY";
    let result = inspector.scan_for_credentials(output);
    assert!(
        result.has_credentials,
        "AWS secret key pattern should be detected"
    );
}

#[test]
fn detects_github_token() {
    let inspector = OutputInspector::new();
    let output = "Token: ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef12";
    let result = inspector.scan_for_credentials(output);
    assert!(
        result.has_credentials,
        "GitHub personal access token should be detected"
    );
}

#[test]
fn detects_generic_api_key_pattern() {
    let inspector = OutputInspector::new();
    let output = "api_key = \"sk-proj-abcdefghijklmnopqrstuvwxyz123456\"";
    let result = inspector.scan_for_credentials(output);
    assert!(
        result.has_credentials,
        "Generic API key pattern should be detected"
    );
}

#[test]
fn detects_bearer_token() {
    let inspector = OutputInspector::new();
    let output = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    let result = inspector.scan_for_credentials(output);
    assert!(
        result.has_credentials,
        "Bearer JWT token should be detected"
    );
}

// ── Encoding tricks ─────────────────────────────────────────────────────

#[test]
fn detects_base64_encoded_credential() {
    let inspector = OutputInspector::new();
    // base64 of "password=secret123"
    let output = "Config: cGFzc3dvcmQ9c2VjcmV0MTIz";
    let result = inspector.scan_for_credentials(output);
    // Base64 detection is best-effort — the inspector should at least
    // flag high-entropy strings that look like encoded secrets
    assert!(
        result.has_credentials || result.suspicious_entropy,
        "Base64-encoded credential should be flagged"
    );
}

// ── Safe content (no false positives) ───────────────────────────────────

#[test]
fn normal_text_not_flagged() {
    let inspector = OutputInspector::new();
    let output = "The weather today is sunny with a high of 72 degrees.";
    let result = inspector.scan_for_credentials(output);
    assert!(
        !result.has_credentials,
        "Normal text should not be flagged as credential"
    );
}

#[test]
fn code_without_credentials_not_flagged() {
    let inspector = OutputInspector::new();
    let output = r#"fn main() { println!("Hello, world!"); }"#;
    let result = inspector.scan_for_credentials(output);
    assert!(
        !result.has_credentials,
        "Normal code should not be flagged"
    );
}

// ── Edge cases ──────────────────────────────────────────────────────────

#[test]
fn empty_output_no_credentials() {
    let inspector = OutputInspector::new();
    let result = inspector.scan_for_credentials("");
    assert!(!result.has_credentials);
}

#[test]
fn very_long_output_no_panic() {
    let inspector = OutputInspector::new();
    let output = "a".repeat(1_000_000);
    let result = inspector.scan_for_credentials(&output);
    // Should complete without panic
    assert!(!result.has_credentials);
}
