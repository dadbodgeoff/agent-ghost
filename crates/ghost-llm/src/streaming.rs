//! Streaming response types (A2.12).

use std::pin::Pin;

use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::provider::{LLMError, UsageStats};

/// A chunk in a streaming response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamChunk {
    /// Text delta.
    TextDelta(String),
    /// Tool call start.
    ToolCallStart { id: String, name: String },
    /// Tool call argument delta.
    ToolCallDelta { id: String, arguments_delta: String },
    /// Stream complete with usage stats.
    Done(UsageStats),
    /// Error during streaming.
    Error(String),
}

/// A boxed async stream of streaming chunks.
pub type StreamChunkStream = Pin<Box<dyn Stream<Item = Result<StreamChunk, LLMError>> + Send>>;

/// Collect all text deltas from a vec of chunks into a single string.
pub fn collect_text_from_chunks(chunks: &[StreamChunk]) -> String {
    chunks
        .iter()
        .filter_map(|c| match c {
            StreamChunk::TextDelta(s) => Some(s.as_str()),
            _ => None,
        })
        .collect()
}
