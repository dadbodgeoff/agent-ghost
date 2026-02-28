//! ChannelAdapter trait — object-safe (Req 22 AC1).

use crate::types::{InboundMessage, OutboundMessage};

/// Unified channel adapter trait. Object-safe for `Box<dyn ChannelAdapter>`.
#[async_trait::async_trait]
pub trait ChannelAdapter: Send + Sync {
    /// Connect to the channel.
    async fn connect(&mut self) -> Result<(), String>;

    /// Disconnect from the channel.
    async fn disconnect(&mut self) -> Result<(), String>;

    /// Send a message to the channel.
    async fn send(&self, message: OutboundMessage) -> Result<(), String>;

    /// Receive the next message from the channel.
    async fn receive(&mut self) -> Result<InboundMessage, String>;

    /// Whether this channel supports streaming responses.
    fn supports_streaming(&self) -> bool;

    /// Whether this channel supports message editing (for streaming updates).
    fn supports_editing(&self) -> bool;

    /// Channel type name.
    fn channel_type(&self) -> &str;
}
