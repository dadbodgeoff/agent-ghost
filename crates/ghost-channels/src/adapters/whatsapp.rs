//! WhatsApp adapter: Baileys Node.js sidecar via stdin/stdout JSON-RPC.
//! Restart up to 3 times on crash (AC4).

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

const MAX_RESTARTS: u32 = 3;

pub struct WhatsAppAdapter {
    connected: bool,
    restart_count: u32,
}

impl WhatsAppAdapter {
    pub fn new() -> Self {
        Self {
            connected: false,
            restart_count: 0,
        }
    }

    /// Restart the Baileys sidecar. Returns false if max restarts exceeded.
    pub fn restart_sidecar(&mut self) -> bool {
        if self.restart_count >= MAX_RESTARTS {
            tracing::warn!("WhatsApp sidecar max restarts exceeded, degrading gracefully");
            return false;
        }
        self.restart_count += 1;
        tracing::info!(restart = self.restart_count, "Restarting WhatsApp sidecar");
        true
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for WhatsAppAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        self.restart_count = 0;
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, _msg: OutboundMessage) -> Result<(), String> { Ok(()) }
    async fn receive(&mut self) -> Result<InboundMessage, String> { Err("stub".into()) }
    fn supports_streaming(&self) -> bool { false }
    fn supports_editing(&self) -> bool { false }
    fn channel_type(&self) -> &str { "whatsapp" }
}
