//! Discord adapter: REST API for sending, Gateway WebSocket for receiving.
//!
//! - Connect to wss://gateway.discord.gg/?v=10&encoding=json
//! - Handle HELLO → IDENTIFY → READY handshake
//! - Listen for MESSAGE_CREATE events → InboundMessage
//! - Send via POST /channels/{id}/messages
//! - Mention-based activation: only respond when bot is @mentioned.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct DiscordAdapter {
    connected: bool,
    bot_token: String,
    /// Channel ID for outbound messages (set from last received message).
    last_channel_id: Option<String>,
    /// Bot's own user ID (set after READY event).
    bot_user_id: Option<String>,
}

impl DiscordAdapter {
    pub fn new(bot_token: &str) -> Self {
        Self {
            connected: false,
            bot_token: bot_token.into(),
            last_channel_id: None,
            bot_user_id: None,
        }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for DiscordAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        if self.bot_token.is_empty() {
            return Err("Discord bot token not configured".into());
        }
        self.connected = true;
        tracing::info!("Discord adapter connected");
        // In production: connect to Gateway WebSocket, perform IDENTIFY handshake,
        // extract bot_user_id from READY event.
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        let channel_id = self.last_channel_id.as_ref()
            .ok_or("no channel_id — receive a message first")?;

        let url = format!(
            "https://discord.com/api/v10/channels/{}/messages",
            channel_id
        );

        let client = reqwest::Client::new();
        let resp = client
            .post(&url)
            .header("authorization", format!("Bot {}", self.bot_token))
            .header("content-type", "application/json")
            .json(&serde_json::json!({ "content": message.content }))
            .send()
            .await
            .map_err(|e| format!("Discord send failed: {e}"))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Discord API error: {text}"));
        }

        Ok(())
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        // In production: read from Gateway WebSocket, filter MESSAGE_CREATE events,
        // check for bot mention, extract content.
        // Stub: returns error until WebSocket gateway connection is implemented.
        Err("Discord Gateway WebSocket not yet connected".into())
    }

    fn supports_streaming(&self) -> bool { false }
    fn supports_editing(&self) -> bool { true }
    fn channel_type(&self) -> &str { "discord" }
}
