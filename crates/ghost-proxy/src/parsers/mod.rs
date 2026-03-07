//! Per-platform payload parsers (Req 36 AC3).

pub mod character_ai_ws;
pub mod chatgpt_sse;
pub mod claude_sse;
pub mod gemini_stream;

use serde::{Deserialize, Serialize};

/// A parsed message extracted from proxy traffic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedMessage {
    pub role: String,
    pub content: String,
    pub platform: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Trait for platform-specific payload parsers.
pub trait PayloadParser: Send + Sync {
    /// Parse a chunk of response data into messages.
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage>;

    /// Platform name.
    fn platform(&self) -> &str;
}
