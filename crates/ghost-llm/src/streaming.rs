//! Streaming response types (A2.12).

use serde::{Deserialize, Serialize};

/// A chunk in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamChunk {
    /// Text delta.
    TextDelta(String),
    /// Tool call start.
    ToolCallStart { id: String, name: String },
    /// Tool call argument delta.
    ToolCallDelta { id: String, arguments_delta: String },
    /// Stream complete.
    Done,
    /// Error during streaming.
    Error(String),
}

/// Streaming response handle.
pub struct StreamingResponse {
    pub chunks: Vec<StreamChunk>,
    pub model: String,
}

impl StreamingResponse {
    pub fn new(model: String) -> Self {
        Self {
            chunks: Vec::new(),
            model,
        }
    }

    /// Collect all text deltas into a single string.
    pub fn collect_text(&self) -> String {
        self.chunks
            .iter()
            .filter_map(|c| match c {
                StreamChunk::TextDelta(s) => Some(s.as_str()),
                _ => None,
            })
            .collect()
    }
}
