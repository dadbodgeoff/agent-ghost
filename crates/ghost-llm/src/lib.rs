//! # ghost-llm
//!
//! LLM provider abstraction with model routing, fallback chains,
//! circuit breaker, cost tracking, and streaming support.
//!
//! Providers: Anthropic, OpenAI, Gemini, Ollama, OpenAI-compatible.
//! Complexity tiers: Free, Cheap, Standard, Premium.
//! Convergence downgrade at L3+ (AC6).

pub mod provider;
pub mod router;
pub mod fallback;
pub mod cost;
pub mod tokens;
pub mod streaming;
pub mod auth;
pub mod quarantine;
pub mod proxy;
