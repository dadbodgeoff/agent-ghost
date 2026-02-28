//! WebSocket adapter: loopback-only default.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct WebSocketAdapter {
    connected: bool,
    bind_address: String,
}

impl WebSocketAdapter {
    pub fn new(bind_address: &str) -> Self {
        Self {
            connected: false,
            bind_address: bind_address.into(),
        }
    }

    pub fn loopback() -> Self {
        Self::new("127.0.0.1:18789")
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for WebSocketAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        tracing::info!(address = %self.bind_address, "WebSocket adapter connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, _message: OutboundMessage) -> Result<(), String> {
        Ok(()) // Placeholder
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        // Placeholder — in production, reads from WebSocket connection
        Err("No message available".into())
    }

    fn supports_streaming(&self) -> bool { true }
    fn supports_editing(&self) -> bool { true }
    fn channel_type(&self) -> &str { "websocket" }
}
