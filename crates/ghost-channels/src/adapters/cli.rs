//! CLI adapter: stdin/stdout with ANSI formatting.

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

pub struct CliAdapter {
    connected: bool,
}

impl CliAdapter {
    pub fn new() -> Self {
        Self { connected: false }
    }
}

impl Default for CliAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for CliAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        println!("{}", message.content);
        Ok(())
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        let mut input = String::new();
        std::io::stdin()
            .read_line(&mut input)
            .map_err(|e| e.to_string())?;
        Ok(InboundMessage::new("cli", "user", input.trim()))
    }

    fn supports_streaming(&self) -> bool { false }
    fn supports_editing(&self) -> bool { false }
    fn channel_type(&self) -> &str { "cli" }
}
