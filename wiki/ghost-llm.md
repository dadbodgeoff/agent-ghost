# ghost-llm

> LLM provider abstraction — model routing, fallback chains, circuit breakers, and convergence-aware downgrades.

## Quick Reference

| Attribute | Value |
|-----------|-------|
| Layer | 4 (Ghost Infrastructure) |
| Type | Library |
| Location | `crates/ghost-llm/` |
| Workspace deps | `cortex-core` (Layer 1), `ghost-secrets` (Layer 0) |
| External deps | `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, `tokio`, `tracing`, `async-trait`, `reqwest`, `rand` |
| Modules | `provider` (types), `router` (complexity-based routing), `fallback` (circuit breaker), `cost` (token pricing), `tokens` (counting), `streaming` (SSE), `auth` (credential management), `quarantine` (compression config), `proxy` (HTTP proxy) |
| Public API | `LLMResponse`, `ChatMessage`, `ModelRouter`, `ComplexityTier`, `ProviderCircuitBreaker`, `CostCalculator`, `TokenCounter`, `StreamingResponse`, `AuthProfileManager`, `ProxyConfig` |
| Test coverage | Dev-dependencies include proptest |
| Downstream consumers | `ghost-agent-loop`, `ghost-gateway` |

---

## Why This Crate Exists

GHOST needs to call LLMs — but not just one LLM. Different tasks need different models (a simple classification doesn't need GPT-4), providers go down (need fallback chains), costs need tracking, and at high convergence, the model should be downgraded to reduce the agent's capability.

`ghost-llm` abstracts all of this behind a unified interface. The agent loop calls `ghost-llm`; `ghost-llm` decides which provider, which model, handles failures, tracks costs, and enforces convergence-based downgrades.

---

## Module Breakdown

### `provider.rs` — Core Types

```rust
pub enum LLMResponse {
    Text { content: String, usage: UsageStats },
    ToolCall { calls: Vec<LLMToolCall>, usage: UsageStats },
    Error(LLMError),
}

pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
}

pub enum MessageRole { System, User, Assistant, Tool }
```

The provider module defines the types that all LLM interactions use. `LLMResponse` is a discriminated union — a response is either text, a tool call request, or an error. There's no "partial" or "streaming" variant here; streaming is handled separately by `StreamingResponse`.

### `router.rs` — Complexity-Based Model Routing

```rust
pub enum ComplexityTier {
    Free,      // Local/free models (Ollama)
    Cheap,     // Low-cost models (GPT-3.5, Haiku)
    Standard,  // Mid-tier models (GPT-4o-mini, Sonnet)
    Premium,   // Top-tier models (GPT-4, Opus)
}
```

The `ComplexityClassifier` analyzes a request and assigns a complexity tier. The `ModelRouter` maps tiers to specific provider/model combinations. This means a simple "summarize this text" request goes to a cheap model, while a complex "analyze this codebase" request goes to a premium model.

**Convergence downgrade (AC6):** At intervention level 3+, the router downgrades the complexity tier. A request that would normally use Premium gets routed to Standard. This reduces the agent's capability at high convergence — a less capable model is less likely to generate sophisticated manipulative content.

### `fallback.rs` — Circuit Breaker

```rust
pub struct ProviderCircuitBreaker {
    threshold: u32,        // failures before opening
    cooldown: Duration,    // time before half-open
    state: CBState,
    failure_count: u32,
    last_failure: Option<Instant>,
}
```

Standard circuit breaker pattern with three states:
- **Closed:** Normal operation, requests go through
- **Open:** Provider is down, requests fail immediately (no network call)
- **HalfOpen:** After cooldown, one request is allowed through to test recovery

Each LLM provider has its own circuit breaker. If Anthropic goes down, requests fall back to OpenAI without waiting for Anthropic timeouts.

### `cost.rs` — Token Pricing and Cost Tracking

```rust
pub struct CostCalculator;

impl CostCalculator {
    pub fn estimate(messages: &[ChatMessage], pricing: &TokenPricing) -> CostEstimate
    pub fn actual(usage: &UsageStats, pricing: &TokenPricing) -> CostActual
}
```

Pre-request cost estimation and post-request actual cost calculation. Used by the gateway to enforce per-agent cost budgets and by the audit system to track spending.

### `tokens.rs` — Token Counting

```rust
pub enum TokenStrategy {
    CharDiv4,    // chars / 4 (rough estimate)
    // Future: tiktoken, sentencepiece
}
```

Currently uses a simple chars/4 estimate. The `TokenStrategy` enum is designed for future integration with proper tokenizers (tiktoken for OpenAI, sentencepiece for others).

### `streaming.rs` — Server-Sent Events

```rust
pub struct StreamingResponse {
    pub model: String,
    pub chunks: Vec<StreamChunk>,
}
```

Collects streaming response chunks into a complete response. Used for real-time output display in the dashboard.

### `auth.rs` — Credential Management

```rust
pub struct AuthProfileManager {
    provider_name: String,
    credentials: Vec<String>,
}
```

Manages API keys for LLM providers. Supports multiple keys per provider (for load balancing) and falls back to environment variables. Integrates with `ghost-secrets` for secure credential storage.

### `quarantine.rs` — Compression Configuration

Configures how context is compressed when approaching token limits. Includes quarantine model tier selection — at high convergence, compression uses a lower-tier model to reduce the risk of the compression step itself being manipulated.

### `proxy.rs` — HTTP Proxy Support

```rust
pub struct ProxyConfig {
    pub url: String,
    pub auth: Option<String>,
}
```

Configures HTTP proxy for LLM API calls. Used in enterprise deployments where all outbound traffic must go through a corporate proxy.

---

## Security Properties

### Credential Isolation

API keys are loaded through `ghost-secrets` and never logged or serialized. The `AuthProfileManager` holds keys in memory only.

### Convergence-Aware Downgrade

At high convergence, the model router automatically selects less capable models. This is a defense-in-depth measure — even if all other convergence controls fail, a less capable model is inherently less dangerous.

### Circuit Breaker Prevents Cascade

If a provider fails, the circuit breaker prevents repeated timeout-inducing calls. This protects both the GHOST process (no thread starvation from hanging HTTP calls) and the provider (no retry storms).

---

## Downstream Consumer Map

```
ghost-llm (Layer 4)
├── ghost-agent-loop (Layer 7)
│   └── All LLM calls go through ghost-llm
└── ghost-gateway (Layer 8)
    └── Configures providers, monitors costs
```

---

## File Map

```
crates/ghost-llm/
├── Cargo.toml
├── src/
│   ├── lib.rs            # Module declarations
│   ├── provider.rs       # LLMResponse, ChatMessage, MessageRole
│   ├── router.rs         # ComplexityTier, ModelRouter, convergence downgrade
│   ├── fallback.rs       # ProviderCircuitBreaker (Closed/Open/HalfOpen)
│   ├── cost.rs           # CostEstimate, CostActual, CostCalculator
│   ├── tokens.rs         # TokenCounter with strategy pattern
│   ├── streaming.rs      # StreamingResponse, StreamChunk
│   ├── auth.rs           # AuthProfileManager, credential loading
│   ├── quarantine.rs     # Compression config, quarantine model tiers
│   └── proxy.rs          # ProxyConfig, ProxyRegistry
```
