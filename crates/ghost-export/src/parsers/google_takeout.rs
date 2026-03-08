//! Google Takeout (Gemini) JSON export parser.

use std::path::Path;

use crate::parsers::ExportParser;
use crate::{ExportError, ExportResult, MessageRole, NormalizedMessage};

pub struct GoogleTakeoutParser;

impl ExportParser for GoogleTakeoutParser {
    fn detect(&self, path: &Path) -> bool {
        if let Ok(data) = std::fs::read_to_string(path) {
            data.contains("\"Takeout\"") || data.contains("\"Bard\"") || data.contains("\"Gemini\"")
        } else {
            false
        }
    }

    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>> {
        let data = std::fs::read_to_string(path)?;
        let json: serde_json::Value =
            serde_json::from_str(&data).map_err(|e| ExportError::ParseError(e.to_string()))?;

        let mut messages = Vec::new();

        // Google Takeout Gemini format: array of conversations
        if let Some(conversations) = json.as_array() {
            for conv in conversations {
                let session_id = conv
                    .get("id")
                    .and_then(|id| id.as_str())
                    .map(|s| s.to_string());

                if let Some(turns) = conv.get("turns").and_then(|t| t.as_array()) {
                    for turn in turns {
                        let role = turn
                            .get("role")
                            .and_then(|r| r.as_str())
                            .unwrap_or("unknown");

                        let content = turn
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or_default()
                            .to_string();

                        if content.is_empty() {
                            continue;
                        }

                        // Parse timestamp from export data; fall back to epoch
                        // if missing (never use Utc::now — export data is historical).
                        let timestamp = turn
                            .get("timestamp")
                            .and_then(|t| t.as_str())
                            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                            .map(|dt| dt.with_timezone(&chrono::Utc))
                            .or_else(|| {
                                turn.get("createTime")
                                    .and_then(|t| t.as_str())
                                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                                    .map(|dt| dt.with_timezone(&chrono::Utc))
                            })
                            .or_else(|| {
                                // Try epoch millis format
                                turn.get("timestamp")
                                    .and_then(|t| t.as_i64())
                                    .and_then(|ms| chrono::DateTime::from_timestamp(ms / 1000, 0))
                            })
                            .unwrap_or(chrono::DateTime::UNIX_EPOCH);

                        messages.push(NormalizedMessage {
                            timestamp,
                            sender: match role {
                                "USER" | "user" => MessageRole::Human,
                                "MODEL" | "model" => MessageRole::Assistant,
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
        "Google Takeout (Gemini)"
    }
}
