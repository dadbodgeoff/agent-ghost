//! Slack adapter: Socket Mode (WebSocket) for receiving, Web API for sending.
//!
//! - Connect via apps.connections.open → WebSocket URL
//! - Listen for event_callback with message type → InboundMessage
//! - Send via chat.postMessage REST endpoint.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct SlackAdapter {
    connected: bool,
    bot_token: String,
    app_token: String,
    /// Channel for outbound messages (set from last received message).
    last_channel: Option<String>,
}

impl SlackAdapter {
    pub fn new(bot_token: &str, app_token: &str) -> Self {
        Self {
            connected: false,
            bot_token: bot_token.into(),
            app_token: app_token.into(),
            last_channel: None,
        }
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for SlackAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        if self.bot_token.is_empty() || self.app_token.is_empty() {
            return Err("Slack tokens not configured".into());
        }
        self.connected = true;
        tracing::info!("Slack adapter connected");
        // In production: call apps.connections.open with app_token to get
        // WebSocket URL, then connect for Socket Mode events.
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        let channel = self
            .last_channel
            .as_ref()
            .ok_or("no channel — receive a message first")?;

        let client = reqwest::Client::new();
        let resp = client
            .post("https://slack.com/api/chat.postMessage")
            .header("authorization", format!("Bearer {}", self.bot_token))
            .header("content-type", "application/json")
            .json(&serde_json::json!({
                "channel": channel,
                "text": message.content,
            }))
            .send()
            .await
            .map_err(|e| format!("Slack send failed: {e}"))?;

        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Slack JSON error: {e}"))?;

        if body["ok"].as_bool() != Some(true) {
            let error = body["error"].as_str().unwrap_or("unknown");
            return Err(format!("Slack API error: {error}"));
        }

        Ok(())
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        // In production: read from Socket Mode WebSocket, parse event_callback
        // with message type, extract text/channel/user.
        Err("Slack Socket Mode not yet connected".into())
    }

    fn supports_streaming(&self) -> bool {
        false
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn channel_type(&self) -> &str {
        "slack"
    }
}
