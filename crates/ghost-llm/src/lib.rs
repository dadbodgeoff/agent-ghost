//! # ghost-llm
//!
//! LLM provider abstraction with model routing, fallback chains,
//! circuit breaker, cost tracking, and streaming support.
//!
//! Providers: Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible.
//! Complexity tiers: Free, Cheap, Standard, Premium.
//! Convergence downgrade at L3+ (AC6).

pub mod auth;
pub mod cost;
pub mod fallback;
pub mod provider;
pub mod proxy;
pub mod quarantine;
pub mod router;
pub mod streaming;
pub mod tokens;
