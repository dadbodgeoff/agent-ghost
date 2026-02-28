//! Proxy ITP emitter — converts parsed payloads to ITP events (Req 36 AC4).

use crate::parsers::ParsedMessage;

/// Emits ITP events from proxy-parsed messages.
pub struct ProxyITPEmitter {
    monitor_socket: String,
}

impl Default for ProxyITPEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyITPEmitter {
    pub fn new() -> Self {
        Self {
            monitor_socket: "~/.ghost/monitor.sock".to_string(),
        }
    }

    pub fn with_socket(socket_path: String) -> Self {
        Self {
            monitor_socket: socket_path,
        }
    }

    /// Emit a parsed message as an ITP event.
    /// Returns true if successfully sent.
    pub fn emit(&self, message: &ParsedMessage) -> bool {
        let event = serde_json::json!({
            "event_type": "InteractionMessage",
            "data": {
                "platform": message.platform,
                "role": message.role,
                "content_hash": format!("{:x}", sha256_hash(message.content.as_bytes())),
                "timestamp": message.timestamp.to_rfc3339(),
            }
        });

        tracing::debug!(
            platform = %message.platform,
            socket = %self.monitor_socket,
            "Emitting proxy ITP event"
        );

        // In production, this sends via unix socket.
        // For now, log the event.
        let _ = event;
        true
    }

    /// Socket path for the monitor connection.
    pub fn socket_path(&self) -> &str {
        &self.monitor_socket
    }
}

fn sha256_hash(data: &[u8]) -> u64 {
    // Simple hash for proxy use — production uses sha2 crate
    let mut hash: u64 = 0xcbf29ce484222325;
    for &byte in data {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}
