//! ChatGPT JSON export parser.

use std::path::Path;

use crate::parsers::ExportParser;
use crate::{ExportError, ExportResult, MessageRole, NormalizedMessage};

pub struct ChatGptParser;

impl ExportParser for ChatGptParser {
    fn detect(&self, path: &Path) -> bool {
        if let Ok(data) = std::fs::read_to_string(path) {
            // ChatGPT exports are JSON arrays with "mapping" keys
            data.contains("\"mapping\"") && data.contains("\"message\"")
        } else {
            false
        }
    }

    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>> {
        let data = std::fs::read_to_string(path)?;
        let json: serde_json::Value =
            serde_json::from_str(&data).map_err(|e| ExportError::ParseError(e.to_string()))?;

        let mut messages = Vec::new();

        if let Some(conversations) = json.as_array() {
            for conv in conversations {
                if let Some(mapping) = conv.get("mapping").and_then(|m| m.as_object()) {
                    for (_id, node) in mapping {
                        if let Some(msg) = node.get("message") {
                            let role = msg
                                .get("author")
                                .and_then(|a| a.get("role"))
                                .and_then(|r| r.as_str())
                                .unwrap_or("unknown");

                            let content = msg
                                .get("content")
                                .and_then(|c| c.get("parts"))
                                .and_then(|p| p.as_array())
                                .map(|parts| {
                                    parts
                                        .iter()
                                        .filter_map(|p| p.as_str())
                                        .collect::<Vec<_>>()
                                        .join("\n")
                                })
                                .unwrap_or_default();

                            if content.is_empty() {
                                continue;
                            }

                            let timestamp = msg
                                .get("create_time")
                                .and_then(|t| t.as_f64())
                                .map(|t| {
                                    chrono::DateTime::from_timestamp(t as i64, 0)
                                        .unwrap_or_default()
                                })
                                .unwrap_or_default();

                            let sender = match role {
                                "user" => MessageRole::Human,
                                "assistant" => MessageRole::Assistant,
                                _ => MessageRole::System,
                            };

                            let conv_id = conv
                                .get("id")
                                .and_then(|id| id.as_str())
                                .map(|s| s.to_string());

                            messages.push(NormalizedMessage {
                                timestamp,
                                sender,
                                content,
                                session_id: conv_id,
                            });
                        }
                    }
                }
            }
        }

        Ok(messages)
    }

    fn name(&self) -> &str {
        "ChatGPT"
    }
}
