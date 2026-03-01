//! WhatsApp adapter: Cloud API (official) or Baileys sidecar.
//!
//! Cloud API approach (preferred):
//! - Webhook receiver for inbound messages
//! - REST POST for outbound messages
//!
//! Sidecar approach (legacy):
//! - Spawn Node.js Baileys process
//! - Communicate via stdin/stdout JSON-RPC
//! - Restart up to 3 times on crash (AC4)

use crate::adapter::ChannelAdapter;
use crate::types::{InboundMessage, OutboundMessage};

use std::collections::VecDeque;

const MAX_RESTARTS: u32 = 3;

/// WhatsApp adapter mode.
#[derive(Debug, Clone)]
pub enum WhatsAppMode {
    /// Official WhatsApp Cloud API (Meta Business).
    CloudApi {
        access_token: String,
        phone_number_id: String,
    },
    /// Baileys Node.js sidecar (self-hosted).
    Sidecar,
}

pub struct WhatsAppAdapter {
    connected: bool,
    restart_count: u32,
    mode: WhatsAppMode,
    /// Last sender phone number for outbound routing.
    last_sender: Option<String>,
    /// Buffered inbound messages (from webhook).
    inbound_queue: VecDeque<InboundMessage>,
}

impl WhatsAppAdapter {
    pub fn new_cloud_api(access_token: &str, phone_number_id: &str) -> Self {
        Self {
            connected: false,
            restart_count: 0,
            mode: WhatsAppMode::CloudApi {
                access_token: access_token.into(),
                phone_number_id: phone_number_id.into(),
            },
            last_sender: None,
            inbound_queue: VecDeque::new(),
        }
    }

    pub fn new_sidecar() -> Self {
        Self {
            connected: false,
            restart_count: 0,
            mode: WhatsAppMode::Sidecar,
            last_sender: None,
            inbound_queue: VecDeque::new(),
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

    /// Push an inbound message (called by webhook handler).
    pub fn push_inbound(&mut self, msg: InboundMessage) {
        self.inbound_queue.push_back(msg);
    }
}

#[async_trait::async_trait]
impl ChannelAdapter for WhatsAppAdapter {
    async fn connect(&mut self) -> Result<(), String> {
        self.connected = true;
        self.restart_count = 0;
        tracing::info!(mode = ?self.mode, "WhatsApp adapter connected");
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<(), String> {
        self.connected = false;
        Ok(())
    }

    async fn send(&self, message: OutboundMessage) -> Result<(), String> {
        match &self.mode {
            WhatsAppMode::CloudApi { access_token, phone_number_id } => {
                let to = self.last_sender.as_ref()
                    .ok_or("no recipient — receive a message first")?;

                let url = format!(
                    "https://graph.facebook.com/v18.0/{}/messages",
                    phone_number_id
                );

                let client = reqwest::Client::new();
                let resp = client
                    .post(&url)
                    .header("authorization", format!("Bearer {access_token}"))
                    .header("content-type", "application/json")
                    .json(&serde_json::json!({
                        "messaging_product": "whatsapp",
                        "to": to,
                        "type": "text",
                        "text": { "body": message.content },
                    }))
                    .send()
                    .await
                    .map_err(|e| format!("WhatsApp send failed: {e}"))?;

                if !resp.status().is_success() {
                    let text = resp.text().await.unwrap_or_default();
                    return Err(format!("WhatsApp API error: {text}"));
                }
                Ok(())
            }
            WhatsAppMode::Sidecar => {
                // In production: write JSON-RPC message to sidecar stdin.
                Err("Sidecar send not yet implemented".into())
            }
        }
    }

    async fn receive(&mut self) -> Result<InboundMessage, String> {
        // Pop from webhook-buffered queue.
        match self.inbound_queue.pop_front() {
            Some(msg) => {
                self.last_sender = Some(msg.sender.clone());
                Ok(msg)
            }
            None => Err("no new messages".into()),
        }
    }

    fn supports_streaming(&self) -> bool { false }
    fn supports_editing(&self) -> bool { false }
    fn channel_type(&self) -> &str { "whatsapp" }
}
