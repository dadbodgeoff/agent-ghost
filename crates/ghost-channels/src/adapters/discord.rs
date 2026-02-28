//! Discord adapter: serenity-rs, slash commands.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct DiscordAdapter { connected: bool }

impl DiscordAdapter {
    pub fn new() -> Self { Self { connected: false } }
}

#[async_trait::async_trait]
impl ChannelAdapter for DiscordAdapter {
    async fn connect(&mut self) -> Result<(), String> { self.connected = true; Ok(()) }
    async fn disconnect(&mut self) -> Result<(), String> { self.connected = false; Ok(()) }
    async fn send(&self, _msg: OutboundMessage) -> Result<(), String> { Ok(()) }
    async fn receive(&mut self) -> Result<InboundMessage, String> { Err("stub".into()) }
    fn supports_streaming(&self) -> bool { false }
    fn supports_editing(&self) -> bool { true }
    fn channel_type(&self) -> &str { "discord" }
}
