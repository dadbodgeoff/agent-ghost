//! Adversarial stress tests: ghost-proxy passive interception guarantee.
//!
//! DomainFilter is the trust boundary. These tests stress domain filter
//! logic, TLS inspection edge cases, and verify passthrough invariants.

use ghost_proxy::DomainFilter;
use ghost_proxy::ProxyServer;
use ghost_proxy::server::ProxyConfig;

// ── Passthrough structural guarantee ────────────────────────────────────

#[test]
fn proxy_is_always_passthrough() {
    let server = ProxyServer::new(ProxyConfig::default());
    assert!(server.is_passthrough());
}

// ── Domain filter: exact match ──────────────────────────────────────────

#[test]
fn allowed_domains_are_intercepted() {
    let filter = DomainFilter::new();
    for domain in &[
        "chat.openai.com", "chatgpt.com", "claude.ai",
        "character.ai", "gemini.google.com", "chat.deepseek.com", "grok.x.ai",
    ] {
        assert!(filter.should_intercept(domain), "{domain} should be intercepted");
    }
}

#[test]
fn non_allowed_domains_are_not_intercepted() {
    let filter = DomainFilter::new();
    for domain in &[
        "google.com", "facebook.com", "evil-chat.openai.com.attacker.com",
        "openai.com", "api.openai.com", "localhost", "127.0.0.1", "",
    ] {
        assert!(!filter.should_intercept(domain), "{domain} should NOT be intercepted");
    }
}

// ── TLS inspection edge cases ───────────────────────────────────────────

#[test]
fn domain_suffix_attack_rejected() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("evil-chatgpt.com"));
    assert!(!filter.should_intercept("notclaude.ai"));
    assert!(!filter.should_intercept("fakechat.openai.com"));
}

#[test]
fn case_insensitive_matching() {
    let filter = DomainFilter::new();
    assert!(filter.should_intercept("CHAT.OPENAI.COM"));
    assert!(filter.should_intercept("Claude.AI"));
    assert!(filter.should_intercept("ChatGPT.com"));
}

#[test]
fn unicode_homoglyph_domains_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("cl\u{0430}ude.ai"));
    assert!(!filter.should_intercept("ch\u{0430}t.openai.com"));
}

#[test]
fn url_encoded_dots_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chat%2Eopenai%2Ecom"));
}

#[test]
fn trailing_dot_domain_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chatgpt.com."));
}

#[test]
fn domain_with_port_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chatgpt.com:443"));
}

// ── Localhost binding ───────────────────────────────────────────────────

#[test]
fn default_config_binds_to_localhost() {
    let config = ProxyConfig::default();
    assert_eq!(config.bind, "127.0.0.1");
}

// ── Stress: large allowlist ─────────────────────────────────────────────

#[test]
fn domain_filter_handles_large_allowlist() {
    let domains: Vec<String> = (0..10_000)
        .map(|i| format!("domain-{i}.example.com"))
        .collect();
    let filter = DomainFilter::with_domains(domains);
    assert!(filter.should_intercept("domain-0.example.com"));
    assert!(filter.should_intercept("domain-9999.example.com"));
    assert!(!filter.should_intercept("domain-10000.example.com"));
}
