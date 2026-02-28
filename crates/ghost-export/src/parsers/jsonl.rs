//! Generic JSONL export parser.

use std::io::BufRead;
use std::path::Path;

use crate::parsers::ExportParser;
use crate::{ExportResult, MessageRole, NormalizedMessage};

pub struct JsonlParser;

impl ExportParser for JsonlParser {
    fn detect(&self, path: &Path) -> bool {
        path.extension()
            .map(|ext| ext == "jsonl" || ext == "ndjson")
            .unwrap_or(false)
    }

    fn parse(&self, path: &Path) -> ExportResult<Vec<NormalizedMessage>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut messages = Vec::new();

        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                let role = json
                    .get("role")
                    .or_else(|| json.get("sender"))
                    .and_then(|r| r.as_str())
                    .unwrap_or("unknown");

                let content = json
                    .get("content")
                    .or_else(|| json.get("text"))
                    .or_else(|| json.get("message"))
                    .and_then(|c| c.as_str())
                    .unwrap_or_default()
                    .to_string();

                if content.is_empty() {
                    continue;
                }

                let timestamp = json
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
                    .unwrap_or_default();

                let session_id = json
                    .get("session_id")
                    .or_else(|| json.get("conversation_id"))
                    .and_then(|s| s.as_str())
                    .map(|s| s.to_string());

                messages.push(NormalizedMessage {
                    timestamp,
                    sender: match role {
                        "user" | "human" => MessageRole::Human,
                        "assistant" | "bot" | "ai" => MessageRole::Assistant,
                        _ => MessageRole::System,
                    },
                    content,
                    session_id,
                });
            }
        }

        Ok(messages)
    }

    fn name(&self) -> &str {
        "Generic JSONL"
    }
}
