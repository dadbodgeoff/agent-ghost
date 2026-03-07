//! ghost-export — data export analyzer (Req 35).
//!
//! Imports conversation history from external AI platforms (ChatGPT, Claude,
//! Character.AI, Gemini, generic JSONL), reconstructs timelines, computes
//! convergence signals, and establishes baselines.

pub mod analyzer;
pub mod parsers;
pub mod timeline;

pub use analyzer::{ExportAnalysisResult, ExportAnalyzer};
pub use timeline::TimelineReconstructor;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse error: {0}")]
    ParseError(String),
    #[error("unsupported format: {0}")]
    UnsupportedFormat(String),
}

pub type ExportResult<T> = Result<T, ExportError>;

/// A normalized message from any platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMessage {
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub sender: MessageRole,
    pub content: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MessageRole {
    Human,
    Assistant,
    System,
}
