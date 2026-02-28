//! Tests for ghost-llm (Task 4.1).

use ghost_llm::cost::CostCalculator;
use ghost_llm::fallback::{CBState, FallbackChain, ProviderCircuitBreaker};
use ghost_llm::provider::*;
use ghost_llm::router::{ComplexityClassifier, ComplexityTier, ModelRouter};
use ghost_llm::streaming::{StreamChunk, StreamingResponse};
use ghost_llm::tokens::{TokenCounter, TokenStrategy};
use std::sync::Arc;
use std::time::Duration;

// ── ComplexityClassifier tests ──────────────────────────────────────────

#[test]
fn classifier_hello_is_free() {
    let tier = ComplexityClassifier::classify("hello", false, 0);
    assert_eq!(tier, ComplexityTier::Free);
}

#[test]
fn classifier_write_function_is_standard_or_higher() {
    let tier = ComplexityClassifier::classify("write a function to parse JSON", false, 0);
    assert!(tier >= ComplexityTier::Standard);
}

#[test]
fn classifier_heartbeat_is_free() {
    let tier = ComplexityClassifier::classify("check heartbeat status", true, 0);
    assert_eq!(tier, ComplexityTier::Free);
}

#[test]
fn classifier_quick_override_is_free() {
    let tier = ComplexityClassifier::classify(
        "/quick write a complex distributed system",
        false,
        0,
    );
    assert_eq!(tier, ComplexityTier::Free);
}

#[test]
fn classifier_deep_override_is_premium() {
    let tier = ComplexityClassifier::classify("/deep hello", false, 0);
    assert_eq!(tier, ComplexityTier::Premium);
}

#[test]
fn classifier_l3_convergence_downgrades() {
    let tier = ComplexityClassifier::classify(
        "write a complex distributed system with consensus",
        false,
        3,
    );
    assert!(tier <= ComplexityTier::Cheap);
}

// ── FallbackChain tests ─────────────────────────────────────────────────

#[tokio::test]
async fn fallback_all_providers_down_returns_error() {
    let mut chain = FallbackChain::new();
    // No providers added
    let result = chain.complete(&[], &[]).await;
    assert!(result.is_err());
}

// ── ProviderCircuitBreaker tests ────────────────────────────────────────

#[test]
fn cb_starts_closed() {
    let cb = ProviderCircuitBreaker::new(3, Duration::from_secs(300));
    assert_eq!(cb.state(), CBState::Closed);
}

#[test]
fn cb_opens_after_threshold_failures() {
    let mut cb = ProviderCircuitBreaker::new(3, Duration::from_secs(300));
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CBState::Closed);
    cb.record_failure();
    assert_eq!(cb.state(), CBState::Open);
}

#[test]
fn cb_halfopen_success_closes() {
    let mut cb = ProviderCircuitBreaker::new(3, Duration::from_millis(1));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();
    assert_eq!(cb.state(), CBState::Open);

    // Wait for cooldown
    std::thread::sleep(Duration::from_millis(5));
    assert!(cb.can_attempt()); // transitions to HalfOpen
    assert_eq!(cb.state(), CBState::HalfOpen);

    cb.record_success();
    assert_eq!(cb.state(), CBState::Closed);
}

#[test]
fn cb_halfopen_failure_reopens() {
    let mut cb = ProviderCircuitBreaker::new(3, Duration::from_millis(1));
    cb.record_failure();
    cb.record_failure();
    cb.record_failure();

    std::thread::sleep(Duration::from_millis(5));
    assert!(cb.can_attempt());

    cb.record_failure();
    assert_eq!(cb.state(), CBState::Open);
}

// ── CostCalculator tests ────────────────────────────────────────────────

#[test]
fn cost_estimate_before_call() {
    let pricing = TokenPricing {
        input_per_1k: 0.003,
        output_per_1k: 0.015,
    };
    let estimate = CostCalculator::estimate(1000, 500, &pricing);
    assert!((estimate.estimated_input_cost - 0.003).abs() < 1e-9);
    assert!((estimate.estimated_output_cost - 0.0075).abs() < 1e-9);
}

#[test]
fn cost_actual_after_call() {
    let pricing = TokenPricing {
        input_per_1k: 0.003,
        output_per_1k: 0.015,
    };
    let usage = UsageStats {
        prompt_tokens: 2000,
        completion_tokens: 1000,
        total_tokens: 3000,
    };
    let actual = CostCalculator::actual(&usage, &pricing);
    assert!((actual.input_cost - 0.006).abs() < 1e-9);
    assert!((actual.output_cost - 0.015).abs() < 1e-9);
}

// ── TokenCounter tests ──────────────────────────────────────────────────

#[test]
fn token_counter_known_string() {
    let counter = TokenCounter::new(TokenStrategy::OpenAI);
    let count = counter.count("hello world");
    assert!(count > 0);
    assert!(count < 10);
}

// ── LLMResponse tests ───────────────────────────────────────────────────

#[test]
fn empty_response_is_no_reply() {
    let resp = LLMResponse::Empty;
    assert!(matches!(resp, LLMResponse::Empty));
}

#[test]
fn mixed_response_has_text_and_tools() {
    let resp = LLMResponse::Mixed {
        text: "Here's the result".into(),
        tool_calls: vec![LLMToolCall {
            id: "1".into(),
            name: "search".into(),
            arguments: serde_json::json!({}),
        }],
    };
    match resp {
        LLMResponse::Mixed { text, tool_calls } => {
            assert!(!text.is_empty());
            assert_eq!(tool_calls.len(), 1);
        }
        _ => panic!("expected Mixed"),
    }
}

// ── ModelRouter tests ───────────────────────────────────────────────────

#[test]
fn router_returns_none_when_empty() {
    let router = ModelRouter::new();
    assert!(router.get_provider(ComplexityTier::Standard).is_none());
}

#[test]
fn router_returns_provider_for_tier() {
    let mut router = ModelRouter::new();
    let provider: Arc<dyn LLMProvider> = Arc::new(OllamaProvider {
        model: "llama3".into(),
        base_url: "http://localhost:11434".into(),
    });
    router.set_provider(ComplexityTier::Standard, provider);
    assert!(router.get_provider(ComplexityTier::Standard).is_some());
}

// ── Streaming tests ─────────────────────────────────────────────────────

#[test]
fn streaming_collect_text() {
    let mut resp = StreamingResponse::new("test-model".into());
    resp.chunks.push(StreamChunk::TextDelta("Hello ".into()));
    resp.chunks.push(StreamChunk::TextDelta("world".into()));
    resp.chunks.push(StreamChunk::Done);
    assert_eq!(resp.collect_text(), "Hello world");
}

// ── Provider timeout test ───────────────────────────────────────────────

#[test]
fn provider_invalid_json_graceful() {
    // Verify LLMError variants exist for graceful error handling
    let err = LLMError::InvalidResponse("bad json".into());
    assert!(err.to_string().contains("invalid response"));
}

#[test]
fn provider_timeout_error() {
    let err = LLMError::Timeout(30);
    assert!(err.to_string().contains("timeout"));
}
