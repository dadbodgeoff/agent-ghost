//! Claude SSE response parser.

use super::{ParsedMessage, PayloadParser};

pub struct ClaudeSseParser;

impl PayloadParser for ClaudeSseParser {
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage> {
        let text = match std::str::from_utf8(data) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut messages = Vec::new();

        for line in text.lines() {
            if let Some(json_str) = line.strip_prefix("data: ") {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    let event_type = json
                        .get("type")
                        .and_then(|t| t.as_str())
                        .unwrap_or_default();

                    if event_type == "content_block_delta" {
                        if let Some(text) = json
                            .get("delta")
                            .and_then(|d| d.get("text"))
                            .and_then(|t| t.as_str())
                        {
                            if !text.is_empty() {
                                messages.push(ParsedMessage {
                                    role: "assistant".to_string(),
                                    content: text.to_string(),
                                    platform: "claude".to_string(),
                                    timestamp: chrono::Utc::now(),
                                });
                            }
                        }
                    }
                }
            }
        }

        messages
    }

    fn platform(&self) -> &str {
        "claude"
    }
}
