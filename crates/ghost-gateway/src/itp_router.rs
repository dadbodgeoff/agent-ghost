//! ITP event router: sends to monitor (Healthy) or buffer (Degraded).

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use crate::gateway::GatewayState;
use crate::itp_buffer::ITPBuffer;

/// Routes ITP events based on gateway state.
pub struct ITPEventRouter {
    gateway_state: Arc<AtomicU8>,
    buffer: std::sync::Mutex<ITPBuffer>,
    monitor_address: String,
}

impl ITPEventRouter {
    pub fn new(gateway_state: Arc<AtomicU8>, monitor_address: String) -> Self {
        Self {
            gateway_state,
            buffer: std::sync::Mutex::new(ITPBuffer::new()),
            monitor_address,
        }
    }

    /// Route an ITP event JSON string.
    pub async fn route(&self, event_json: String) {
        let state = GatewayState::from_u8(self.gateway_state.load(Ordering::Acquire));
        match state {
            GatewayState::Healthy | GatewayState::Recovering => {
                self.send_to_monitor(&event_json).await;
            }
            GatewayState::Degraded => {
                if let Ok(mut buf) = self.buffer.lock() {
                    buf.push(event_json);
                }
            }
            _ => {
                // ShuttingDown, FatalError, Initializing — drop
            }
        }
    }

    /// Drain buffered events for replay during recovery.
    pub fn drain_buffer(&self) -> Vec<String> {
        if let Ok(mut buf) = self.buffer.lock() {
            buf.drain_all().into_iter().map(|e| e.json).collect()
        } else {
            Vec::new()
        }
    }

    async fn send_to_monitor(&self, event_json: &str) {
        // Send via HTTP POST to the convergence monitor's event endpoint.
        // Falls back to buffering if the monitor is unreachable.
        let url = format!("{}/events", self.monitor_address);
        let client = reqwest::Client::new();
        match client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(event_json.to_string())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => {
                tracing::warn!(
                    status = %resp.status(),
                    "monitor rejected ITP event — buffering"
                );
                if let Ok(mut buf) = self.buffer.lock() {
                    buf.push(event_json.to_string());
                }
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to send ITP event to monitor — buffering"
                );
                if let Ok(mut buf) = self.buffer.lock() {
                    buf.push(event_json.to_string());
                }
            }
        }
    }
}
