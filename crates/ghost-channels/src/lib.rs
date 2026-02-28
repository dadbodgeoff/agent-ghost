//! Channel adapter framework (Req 22).
//!
//! Provides a unified ChannelAdapter trait and normalized message types
//! for CLI, WebSocket, Telegram, Discord, Slack, and WhatsApp.

pub mod adapter;
pub mod adapters;
pub mod streaming;
pub mod types;
