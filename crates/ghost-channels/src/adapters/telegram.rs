//! Telegram adapter: Bot API via HTTPS, long polling for inbound.
//!
//! - Long polling: GET /getUpdates?offset={last_update_id+1}
//! - Send: POST /sendMessage with chat_id + text
//! - Supports reply_to_message_id for threaded conversations.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct TelegramAdapter {
    connected: bool,
    bot_token: String,
    last_update_id: i64,
    /// Chat ID for outbound messages (set from last received message).
    last_chat_id: Option<i64>,
}

impl TelegramAdapter {
    pub fn new(bot_token: &str) -> Self {
        Self {
            connected: false,
            bot_token: bot_token.into(),
            last_update_id: 0,
            last_chat_id: None,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.bot_token, method)
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for TelegramAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        if self.bot_token.is_empty() {
            return Err("Telegram bot token not configured".into());
        }
        self.connected = true;
        tracing::info!("Telegram adapter connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        let chat_id = self
            .last_chat_id
            .ok_or("no chat_id available — receive a message first")?;

        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": message.content,
        });
        if let Some(reply_to) = message.reply_to {
            body["reply_to_message_id"] = serde_json::json!(reply_to.as_u128() as i64);
        }

        let client = reqwest::Client::new();
        let resp = client
            .post(&self.api_url("sendMessage"))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Telegram send failed: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Telegram API error: {text}"));
        }

        Ok(())
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        let url = format!(
            "{}?offset={}&timeout=30",
            self.api_url("getUpdates"),
            self.last_update_id + 1
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(35))
            .build()
            .map_err(|e| format!("HTTP client error: {e}"))?;

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("Telegram poll failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Telegram JSON error: {e}"))?;

        let updates = body["result"]
            .as_array()
            .ok_or("invalid Telegram response")?;

        for update in updates {
            let update_id = update["update_id"].as_i64().unwrap_or(0);
            if update_id > self.last_update_id {
                self.last_update_id = update_id;
            }

            if let Some(message) = update.get("message") {
                let text = message["text"].as_str().unwrap_or("");
                let chat_id = message["chat"]["id"].as_i64().unwrap_or(0);
                let sender = message["from"]["username"]
                    .as_str()
                    .or_else(|| message["from"]["first_name"].as_str())
                    .unwrap_or("unknown");

                self.last_chat_id = Some(chat_id);

                return Ok(InboundMessage::new("telegram", sender, text));
            }
        }

        Err("no new messages".into())
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn channel_type(&self) -> &str {
        "telegram"
    }
}
