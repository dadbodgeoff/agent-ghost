//! Claude.ai export parser.

use std::path::Path;

use crate::parsers::ExportParser;
use crate::{ExportError, ExportResult, MessageRole, NormalizedMessage};

pub struct ClaudeParser;

impl ExportParser for ClaudeParser {
    fn detect(&self, path: &Path) -> bool {
        if let Ok(data) = std::fs::read_to_string(path) {
            data.contains("\"chat_messages\"") || data.contains("\"uuid\"")
        } else {
            false
        }
    }

    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>> {
        let data = std::fs::read_to_string(path)?;
        let json: serde_json::Value =
            serde_json::from_str(&data).map_err(|e| ExportError::ParseError(e.to_string()))?;

        let mut messages = Vec::new();

        // Claude export: array of conversations with chat_messages
        if let Some(conversations) = json.as_array() {
            for conv in conversations {
                let session_id = conv
                    .get("uuid")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string());

                if let Some(msgs) = conv.get("chat_messages").and_then(|m| m.as_array()) {
                    for msg in msgs {
                        let sender_str = msg
                            .get("sender")
                            .and_then(|s| s.as_str())
                            .unwrap_or("unknown");

                        let content = msg
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string();

                        if content.is_empty() {
                            continue;
                        }

                        let timestamp = msg
                            .get("created_at")
                            .and_then(|t| t.as_str())
                            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .unwrap_or_default();

                        messages.push(NormalizedMessage {
                            timestamp,
                            sender: match sender_str {
                                "human" => MessageRole::Human,
                                "assistant" => MessageRole::Assistant,
                                _ => MessageRole::System,
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
        "Claude.ai"
    }
}
