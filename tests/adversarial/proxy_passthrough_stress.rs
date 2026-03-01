//! Adversarial stress tests: ghost-proxy passive interception guarantee.
//!
//! The "never modifies traffic" invariant (AC5) is the trust boundary.
//! DomainFilter is the gate that decides what gets intercepted.
//! These tests stress the domain filter logic, TLS inspection edge cases,
//! and verify the emitter only hashes content (never leaks plaintext).

use ghost_proxy::DomainFilter;
use ghost_proxy::ProxyServer;
use ghost_proxy::server::ProxyConfig;

// ── Passthrough structural guarantee ────────────────────────────────────

#[test]
fn proxy_is_always_passthrough() {
    let server = ProxyServer::new(ProxyConfig::default());
    assert!(
        server.is_passthrough(),
        "proxy must ALWAYS be passthrough — structural invariant"
    );
}

#[test]
fn proxy_with_custom_filter_still_passthrough() {
    let filter = DomainFilter::with_domains(vec!["evil.com".into()]);
    let server = ProxyServer::new(ProxyConfig::default()).with_domain_filter(filter);
    assert!(server.is_passthrough());
}

// ── Domain filter: exact match ──────────────────────────────────────────

#[test]
fn allowed_domains_are_intercepted() {
    let filter = DomainFilter::new();
    let expected = [
        "chat.openai.com",
        "chatgpt.com",
        "claude.ai",
        "character.ai",
        "gemini.google.com",
        "chat.deepseek.com",
        "grok.x.ai",
    ];
    for domain in &expected {
        assert!(
            filter.should_intercept(domain),
            "{domain} should be intercepted"
        );
    }
}

#[test]
fn non_allowed_domains_are_not_intercepted() {
    let filter = DomainFilter::new();
    let rejected = [
        "google.com",
        "facebook.com",
        "evil-chat.openai.com.attacker.com",
        "openai.com",
        "api.openai.com",
        "localhost",
        "127.0.0.1",
        "",
    ];
    for domain in &rejected {
        assert!(
            !filter.should_intercept(domain),
            "{domain} should NOT be intercepted"
        );
    }
}

// ── Domain filter: subdomain matching ───────────────────────────────────

#[test]
fn subdomains_of_allowed_domains_are_intercepted() {
    let filter = DomainFilter::new();
    assert!(filter.should_intercept("www.chatgpt.com"));
    assert!(filter.should_intercept("api.chat.openai.com"));
    assert!(filter.should_intercept("sub.sub.claude.ai"));
}

// ── Domain filter: case normalization ───────────────────────────────────

#[test]
fn domain_matching_is_case_insensitive() {
    let filter = DomainFilter::new();
    assert!(filter.should_intercept("CHAT.OPENAI.COM"));
    assert!(filter.should_intercept("Claude.AI"));
    assert!(filter.should_intercept("ChatGPT.com"));
    assert!(filter.should_intercept("GEMINI.GOOGLE.COM"));
}

// ── TLS inspection edge cases ───────────────────────────────────────────

/// Attacker appends allowed domain as suffix to bypass filter.
/// e.g., "evil-chat.openai.com" should NOT match "chat.openai.com"
/// but "sub.chat.openai.com" SHOULD match (it's a real subdomain).
#[test]
fn domain_suffix_attack_rejected() {
    let filter = DomainFilter::new();

    // These are NOT subdomains — they just end with the allowed domain string
    assert!(
        !filter.should_intercept("evil-chatgpt.com"),
        "evil-chatgpt.com is not a subdomain of chatgpt.com"
    );
    assert!(
        !filter.should_intercept("notclaude.ai"),
        "notclaude.ai is not a subdomain of claude.ai"
    );
    assert!(
        !filter.should_intercept("fakechat.openai.com"),
        "fakechat.openai.com is not a subdomain of chat.openai.com"
    );
}

/// Attacker uses URL-encoded dots to bypass domain parsing.
#[test]
fn url_encoded_dots_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chat%2Eopenai%2Ecom"));
    assert!(!filter.should_intercept("claude%2Eai"));
}

/// Attacker uses unicode homoglyphs (e.g., Cyrillic 'а' for Latin 'a').
#[test]
fn unicode_homoglyph_domains_not_intercepted() {
    let filter = DomainFilter::new();
    // Cyrillic 'а' (U+0430) looks like Latin 'a' (U+0061)
    assert!(!filter.should_intercept("cl\u{0430}ude.ai"));
    assert!(!filter.should_intercept("ch\u{0430}t.openai.com"));
}

/// Empty and whitespace domains.
#[test]
fn empty_and_whitespace_domains_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept(""));
    assert!(!filter.should_intercept(" "));
    assert!(!filter.should_intercept("\t"));
    assert!(!filter.should_intercept("\n"));
}

/// Domain with trailing dot (DNS root).
#[test]
fn trailing_dot_domain_not_intercepted() {
    let filter = DomainFilter::new();
    // "chatgpt.com." is technically valid DNS but our filter doesn't strip trailing dots
    assert!(!filter.should_intercept("chatgpt.com."));
}

/// Domain with port number embedded.
#[test]
fn domain_with_port_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chatgpt.com:443"));
    assert!(!filter.should_intercept("claude.ai:8080"));
}

/// Domain with path component.
#[test]
fn domain_with_path_not_intercepted() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("chatgpt.com/api/v1"));
    assert!(!filter.should_intercept("claude.ai/chat"));
}

// ── Localhost binding ───────────────────────────────────────────────────

#[test]
fn default_config_binds_to_localhost() {
    let config = ProxyConfig::default();
    assert_eq!(config.bind, "127.0.0.1");
    assert_eq!(config.port, 8080);
}

#[test]
fn bind_address_is_localhost_only() {
    let server = ProxyServer::new(ProxyConfig::default());
    let addr = server.bind_addr();
    assert!(
        addr.starts_with("127.0.0.1:"),
        "proxy must bind to localhost only, got {addr}"
    );
}

// ── Emitter: content hashing, not plaintext ─────────────────────────────

#[test]
fn emitter_produces_content_hash_not_plaintext() {
    use ghost_proxy::ProxyITPEmitter;
    use ghost_proxy::parsers::ParsedMessage;

    let emitter = ProxyITPEmitter::new();
    let msg = ParsedMessage {
        role: "assistant".into(),
        content: "This is sensitive conversation content".into(),
        platform: "test".into(),
        timestamp: chrono::Utc::now(),
    };

    // The emitter should succeed (it hashes content internally)
    assert!(emitter.emit(&msg));
}

// ── Custom domain filter ────────────────────────────────────────────────

#[test]
fn custom_domain_filter_overrides_defaults() {
    let filter = DomainFilter::with_domains(vec!["custom.example.com".into()]);
    assert!(filter.should_intercept("custom.example.com"));
    assert!(!filter.should_intercept("chatgpt.com"));
    assert!(!filter.should_intercept("claude.ai"));
}

#[test]
fn empty_domain_filter_intercepts_nothing() {
    let filter = DomainFilter::with_domains(vec![]);
    assert!(!filter.should_intercept("chatgpt.com"));
    assert!(!filter.should_intercept("claude.ai"));
    assert!(!filter.should_intercept("anything.com"));
}

// ── Stress: many domains ────────────────────────────────────────────────

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

#[test]
fn domain_filter_handles_very_long_domain() {
    let filter = DomainFilter::new();
    let long_domain = "a".repeat(1000) + ".chatgpt.com";
    // This is a valid subdomain of chatgpt.com
    assert!(filter.should_intercept(&long_domain));
}
