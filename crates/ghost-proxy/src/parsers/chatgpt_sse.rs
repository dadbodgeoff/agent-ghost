//! ChatGPT SSE response parser.

use super::{ParsedMessage, PayloadParser};

pub struct ChatGptSseParser;

impl PayloadParser for ChatGptSseParser {
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage> {
        let text = match std::str::from_utf8(data) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut messages = Vec::new();

        for line in text.lines() {
            if let Some(json_str) = line.strip_prefix("data: ") {
                if json_str == "[DONE]" {
                    continue;
                }
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(content) = json
                        .get("choices")
                        .and_then(|c| c.get(0))
                        .and_then(|c| c.get("delta"))
                        .and_then(|d| d.get("content"))
                        .and_then(|c| c.as_str())
                    {
                        if !content.is_empty() {
                            messages.push(ParsedMessage {
                                role: "assistant".to_string(),
                                content: content.to_string(),
                                platform: "chatgpt".to_string(),
                                timestamp: chrono::Utc::now(),
                            });
                        }
                    }
                }
            }
        }

        messages
    }

    fn platform(&self) -> &str {
        "chatgpt"
    }
}
