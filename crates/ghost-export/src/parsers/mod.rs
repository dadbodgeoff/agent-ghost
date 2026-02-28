//! Platform-specific export parsers (Req 35 AC2).

pub mod chatgpt;
pub mod character_ai;
pub mod google_takeout;
pub mod claude;
pub mod jsonl;

use std::path::Path;

use crate::{ExportResult, NormalizedMessage};

/// Trait for platform-specific export parsers.
pub trait ExportParser: Send + Sync {
    /// Returns true if this parser can handle the given file.
    fn detect(&self, path: &Path) -> bool;

    /// Parse the export file into normalized messages.
    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>>;

    /// Human-readable name of the parser.
    fn name(&self) -> &str;
}

/// Returns all available parsers in detection priority order.
pub fn all_parsers() -> Vec<Box<dyn ExportParser>> {
    vec![
        Box::new(chatgpt::ChatGptParser),
        Box::new(character_ai::CharacterAiParser),
        Box::new(google_takeout::GoogleTakeoutParser),
        Box::new(claude::ClaudeParser),
        Box::new(jsonl::JsonlParser),
    ]
}
