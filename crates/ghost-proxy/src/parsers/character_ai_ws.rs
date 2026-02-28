//! Character.AI WebSocket JSON parser.

use super::{ParsedMessage, PayloadParser};

pub struct CharacterAiWsParser;

impl PayloadParser for CharacterAiWsParser {
    fn parse_chunk(&self, data: &[u8]) -> Vec<ParsedMessage> {
        let text = match std::str::from_utf8(data) {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let mut messages = Vec::new();

        if let Ok(json) = serde_json::from_str::<serde_json::Value>(text) {
            if let Some(reply) = json
                .get("turn")
                .and_then(|t| t.get("candidates"))
                .and_then(|c| c.get(0))
                .and_then(|c| c.get("raw_content"))
                .and_then(|r| r.as_str())
            {
                if !reply.is_empty() {
                    messages.push(ParsedMessage {
                        role: "assistant".to_string(),
                        content: reply.to_string(),
                        platform: "character_ai".to_string(),
                        timestamp: chrono::Utc::now(),
                    });
                }
            }
        }

        messages
    }

    fn platform(&self) -> &str {
        "character_ai"
    }
}
