//! ITP emission via bounded channel (Req 11 AC4).
//!
//! Capacity 1000, try_send drops on full — never blocks the agent loop.

use itp_protocol::events::ITPEvent;
use tokio::sync::mpsc;

/// Non-blocking ITP event emitter.
pub struct ITPEmitter {
    sender: mpsc::Sender<ITPEvent>,
}

impl ITPEmitter {
    /// Create a new emitter with the given channel sender.
    pub fn new(sender: mpsc::Sender<ITPEvent>) -> Self {
        Self { sender }
    }

    /// Create a bounded channel pair (capacity 1000).
    pub fn channel() -> (Self, mpsc::Receiver<ITPEvent>) {
        let (tx, rx) = mpsc::channel(1000);
        (Self::new(tx), rx)
    }

    /// Emit an ITP event. Drops on full channel — never blocks (AC4).
    pub fn emit(&self, event: ITPEvent) {
        match self.sender.try_send(event) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                tracing::warn!("ITP channel full — event dropped");
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                tracing::warn!("ITP channel closed — event dropped");
            }
        }
    }

    /// Emit a SessionStart event (pre-loop step 11).
    pub fn emit_session_start(&self, agent_id: uuid::Uuid, session_id: uuid::Uuid) {
        self.emit(ITPEvent::SessionStart(
            itp_protocol::events::SessionStartEvent {
                session_id,
                agent_id,
                channel: String::new(),
                privacy_level: itp_protocol::privacy::PrivacyLevel::Standard,
                timestamp: chrono::Utc::now(),
            },
        ));
    }

    /// Emit an InteractionMessage event (pre-loop step 11).
    pub fn emit_interaction_message(
        &self,
        _agent_id: uuid::Uuid,
        session_id: uuid::Uuid,
        content: &str,
    ) {
        let content_hash = itp_protocol::privacy::hash_content(content);
        self.emit(ITPEvent::InteractionMessage(
            itp_protocol::events::InteractionMessageEvent {
                session_id,
                message_id: uuid::Uuid::now_v7(),
                sender: itp_protocol::events::MessageSender::Human,
                content_hash,
                content_plaintext: Some(content.to_string()),
                token_count: content.split_whitespace().count(),
                timestamp: chrono::Utc::now(),
            },
        ));
    }
}
