//! Gemini streaming JSON parser.

use super::{ParsedMessage, PayloadParser};

pub struct GeminiStreamParser;

impl PayloadParser for GeminiStreamParser {
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage> {
        let text = match std::str::from_utf8(data) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut messages = Vec::new();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(candidates) = json.get("candidates").and_then(|c| c.as_array()) {
                for candidate in candidates {
                    if let Some(content) = candidate
                        .get("content")
                        .and_then(|c| c.get("parts"))
                        .and_then(|p| p.as_array())
                    {
                        for part in content {
                            if let Some(text) = part.get("text").and_then(|t| t.as_str()) {
                                if !text.is_empty() {
                                    messages.push(ParsedMessage {
                                        role: "assistant".to_string(),
                                        content: text.to_string(),
                                        platform: "gemini".to_string(),
                                        timestamp: chrono::Utc::now(),
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        messages
    }

    fn platform(&self) -> &str {
        "gemini"
    }
}
