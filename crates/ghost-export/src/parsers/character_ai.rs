//! Character.AI JSON export parser.

use std::path::Path;

use crate::parsers::ExportParser;
use crate::{ExportError, ExportResult, MessageRole, NormalizedMessage};

pub struct CharacterAiParser;

impl ExportParser for CharacterAiParser {
    fn detect(&self, path: &Path) -> bool {
        if let Ok(data) = std::fs::read_to_string(path) {
            data.contains("\"histories\"") || data.contains("\"character\"")
        } else {
            false
        }
    }

    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>> {
        let data = std::fs::read_to_string(path)?;
        let json: serde_json::Value =
            serde_json::from_str(&data).map_err(|e| ExportError::ParseError(e.to_string()))?;

        let mut messages = Vec::new();

        if let Some(histories) = json.get("histories").and_then(|h| h.as_array()) {
            for history in histories {
                let session_id = history
                    .get("external_id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string());

                if let Some(msgs) = history.get("msgs").and_then(|m| m.as_array()) {
                    for msg in msgs {
                        let is_human = msg
                            .get("src")
                            .and_then(|s| s.get("is_human"))
                            .and_then(|h| h.as_bool())
                            .unwrap_or(false);

                        let content = msg
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string();

                        if content.is_empty() {
                            continue;
                        }

                        // Parse timestamp from export data; fall back to epoch
                        // if missing (never use Utc::now — export data is historical).
                        let timestamp = msg
                            .get("created")
                            .and_then(|t| t.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .or_else(|| {
                                msg.get("created")
                                    .and_then(|t| t.as_i64())
                                    .and_then(|ms| chrono::DateTime::from_timestamp(ms / 1000, 0))
                            })
                            .unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);

                        messages.push(NormalizedMessage {
                            timestamp,
                            sender: if is_human {
                                MessageRole::Human
                            } else {
                                MessageRole::Assistant
                            },
                            content,
                            session_id: session_id.clone(),
                        });
                    }
                }
            }
        }

        Ok(messages)
    }

    fn name(&self) -> &str {
        "Character.AI"
    }
}
