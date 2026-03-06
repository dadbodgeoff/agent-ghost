//! # cortex-embeddings
//!
//! Embedding generation for Cortex memories.
//! Ships with a TF-IDF fallback provider (zero external deps).
//! Designed for extension with ONNX/Ollama/API providers behind Cargo features.
//!
//! ## Architecture
//!
//! ```text
//! EmbeddingEngine
//! ├── TfIdfProvider (always available)
//! ├── EmbeddingCache (in-memory HashMap with capacity limit)
//! └── Enrichment (metadata prefix for better embedding quality)
//! ```

pub mod cache;
pub mod engine;
pub mod enrichment;
pub mod tfidf;

pub use engine::{EmbeddingConfig, EmbeddingEngine};
