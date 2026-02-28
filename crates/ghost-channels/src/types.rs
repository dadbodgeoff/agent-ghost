//! Normalized message types across all channels.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Normalized inbound message from any channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboundMessage {
    pub id: Uuid,
    pub channel: String,
    pub sender: String,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub attachments: Vec<Attachment>,
}

/// Normalized outbound message to any channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutboundMessage {
    pub content: String,
    pub reply_to: Option<Uuid>,
    pub attachments: Vec<Attachment>,
}

/// Message attachment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    pub data: Vec<u8>,
}

impl InboundMessage {
    pub fn new(channel: &str, sender: &str, content: &str) -> Self {
        Self {
            id: Uuid::now_v7(),
            channel: channel.into(),
            sender: sender.into(),
            content: content.into(),
            timestamp: Utc::now(),
            attachments: Vec::new(),
        }
    }
}
