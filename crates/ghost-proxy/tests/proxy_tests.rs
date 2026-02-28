//! Tests for ghost-proxy (Task 6.4).

use ghost_proxy::domain_filter::DomainFilter;
use ghost_proxy::emitter::ProxyITPEmitter;
use ghost_proxy::parsers::chatgpt_sse::ChatGptSseParser;
use ghost_proxy::parsers::claude_sse::ClaudeSseParser;
use ghost_proxy::parsers::PayloadParser;
use ghost_proxy::server::{ProxyConfig, ProxyServer};

#[test]
fn domain_filter_allows_listed_domains() {
    let filter = DomainFilter::new();
    assert!(filter.should_intercept("chat.openai.com"));
    assert!(filter.should_intercept("chatgpt.com"));
    assert!(filter.should_intercept("claude.ai"));
    assert!(filter.should_intercept("character.ai"));
    assert!(filter.should_intercept("gemini.google.com"));
    assert!(filter.should_intercept("chat.deepseek.com"));
    assert!(filter.should_intercept("grok.x.ai"));
}

#[test]
fn domain_filter_passes_non_matching() {
    let filter = DomainFilter::new();
    assert!(!filter.should_intercept("google.com"));
    assert!(!filter.should_intercept("github.com"));
    assert!(!filter.should_intercept("example.com"));
}

#[test]
fn chatgpt_sse_parser_extracts_messages() {
    let parser = ChatGptSseParser;
    let data = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello world\"}}]}\n";
    let messages = parser.parse_chunk(data);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Hello world");
    assert_eq!(messages[0].platform, "chatgpt");
}

#[test]
fn chatgpt_sse_parser_handles_done() {
    let parser = ChatGptSseParser;
    let data = b"data: [DONE]\n";
    let messages = parser.parse_chunk(data);
    assert!(messages.is_empty());
}

#[test]
fn claude_sse_parser_extracts_messages() {
    let parser = ClaudeSseParser;
    let data = b"data: {\"type\":\"content_block_delta\",\"delta\":{\"text\":\"Hi there\"}}\n";
    let messages = parser.parse_chunk(data);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "Hi there");
    assert_eq!(messages[0].platform, "claude");
}

#[test]
fn proxy_itp_emitter_sends_valid_events() {
    let emitter = ProxyITPEmitter::new();
    let msg = ghost_proxy::parsers::ParsedMessage {
        role: "assistant".to_string(),
        content: "test content".to_string(),
        platform: "chatgpt".to_string(),
        timestamp: chrono::Utc::now(),
    };
    assert!(emitter.emit(&msg));
}

#[test]
fn proxy_never_modifies_traffic() {
    let server = ProxyServer::new(ProxyConfig::default());
    assert!(server.is_passthrough());
}

#[test]
fn proxy_intercepts_allowed_domain() {
    let server = ProxyServer::new(ProxyConfig::default());
    assert!(server.should_intercept("claude.ai"));
}

#[test]
fn proxy_passes_non_allowed_domain() {
    let server = ProxyServer::new(ProxyConfig::default());
    assert!(!server.should_intercept("reddit.com"));
}

#[test]
fn binary_traffic_no_crash() {
    let parser = ChatGptSseParser;
    let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();
    let messages = parser.parse_chunk(&binary_data);
    assert!(messages.is_empty()); // No crash, just empty
}

#[test]
fn malformed_sse_no_crash() {
    let parser = ChatGptSseParser;
    let data = b"data: {invalid json here\ndata: also broken\n";
    let messages = parser.parse_chunk(data);
    assert!(messages.is_empty());
}
