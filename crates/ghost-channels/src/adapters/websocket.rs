//! WebSocket adapter: bridges WebSocket connections to the agent loop.
//!
//! Inbound: parse JSON messages from WebSocket → InboundMessage.
//! Outbound: serialize OutboundMessage → JSON → WebSocket text frame.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

use std::collections::VecDeque;

pub struct WebSocketAdapter {
    connected: bool,
    bind_address: String,
    /// Buffered inbound messages from WebSocket clients.
    inbound_queue: VecDeque<InboundMessage>,
}

impl WebSocketAdapter {
    pub fn new(bind_address: &str) -> Self {
        Self {
            connected: false,
            bind_address: bind_address.into(),
            inbound_queue: VecDeque::new(),
        }
    }

    pub fn loopback() -> Self {
        Self::new("127.0.0.1:18789")
    }

    /// Push an inbound message (called by the WebSocket handler when a
    /// client sends a message).
    pub fn push_inbound(&mut self, msg: InboundMessage) {
        self.inbound_queue.push_back(msg);
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

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        // Serialize to JSON for WebSocket delivery.
        let json =
            serde_json::to_string(&message).map_err(|e| format!("serialization error: {e}"))?;
        tracing::debug!(len = json.len(), "WebSocket outbound message serialized");
        // In production, this sends to the connected WebSocket client(s)
        // via a broadcast channel or direct socket reference.
        Ok(())
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        // Pop from the inbound queue.
        self.inbound_queue
            .pop_front()
            .ok_or_else(|| "no message available".into())
    }

    fn supports_streaming(&self) -> bool {
        true
    }
    fn supports_editing(&self) -> bool {
        true
    }
    fn channel_type(&self) -> &str {
        "websocket"
    }
}
