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
                let mut buf = match self.buffer.lock() {
                    Ok(buf) => buf,
                    Err(poisoned) => {
                        tracing::error!("ITP buffer Mutex poisoned during route — recovering");
                        poisoned.into_inner()
                    }
                };
                buf.push(event_json);
            }
            _ => {
                // ShuttingDown, FatalError, Initializing — drop
            }
        }
    }

    /// Drain buffered events for replay during recovery.
    pub fn drain_buffer(&self) -> Vec<String> {
        let mut buf = match self.buffer.lock() {
            Ok(buf) => buf,
            Err(poisoned) => {
                tracing::error!("ITP buffer Mutex poisoned during drain — recovering");
                poisoned.into_inner()
            }
        };
        buf.drain_all().into_iter().map(|e| e.json).collect()
    }

    pub fn buffer_len(&self) -> usize {
        let buf = match self.buffer.lock() {
            Ok(buf) => buf,
            Err(poisoned) => {
                tracing::error!("ITP buffer Mutex poisoned during len — recovering");
                poisoned.into_inner()
            }
        };
        buf.len()
    }

    pub async fn replay_buffered(&self) -> usize {
        let buffered = self.drain_buffer();
        for (index, event_json) in buffered.iter().enumerate() {
            if !self.send_to_monitor(event_json).await {
                let mut buf = match self.buffer.lock() {
                    Ok(buf) => buf,
                    Err(poisoned) => {
                        tracing::error!("ITP buffer Mutex poisoned during replay — recovering");
                        poisoned.into_inner()
                    }
                };
                for remaining in &buffered[index + 1..] {
                    buf.push(remaining.clone());
                }
                break;
            }
        }
        self.buffer_len()
    }

    pub async fn send_direct(&self, event_json: String) -> bool {
        self.send_to_monitor(&event_json).await
    }

    async fn send_to_monitor(&self, event_json: &str) -> bool {
        // Send via HTTP POST to the convergence monitor's event endpoint.
        // Falls back to buffering if the monitor is unreachable.
        let base = if self.monitor_address.starts_with("http://")
            || self.monitor_address.starts_with("https://")
        {
            self.monitor_address.clone()
        } else {
            format!("http://{}", self.monitor_address)
        };
        let url = format!("{}/events", base.trim_end_matches('/'));
        let client = reqwest::Client::new();
        match client
            .post(&url)
            .header("Content-Type", "application/json")
            .body(event_json.to_string())
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => true,
            Ok(resp) => {
                tracing::warn!(
                    status = %resp.status(),
                    "monitor rejected ITP event — buffering"
                );
                let mut buf = match self.buffer.lock() {
                    Ok(buf) => buf,
                    Err(poisoned) => {
                        tracing::error!(
                            "ITP buffer Mutex poisoned during fallback buffer — recovering"
                        );
                        poisoned.into_inner()
                    }
                };
                buf.push(event_json.to_string());
                false
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "failed to send ITP event to monitor — buffering"
                );
                let mut buf = match self.buffer.lock() {
                    Ok(buf) => buf,
                    Err(poisoned) => {
                        tracing::error!(
                            "ITP buffer Mutex poisoned during fallback buffer — recovering"
                        );
                        poisoned.into_inner()
                    }
                };
                buf.push(event_json.to_string());
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::Arc;

    use axum::extract::State;
    use axum::http::StatusCode;
    use axum::routing::post;
    use axum::Router;
    use tokio::sync::Mutex;

    use super::ITPEventRouter;
    use crate::gateway::GatewayState;

    #[tokio::test]
    async fn replay_buffered_flushes_events_once_monitor_recovers() {
        let captured = Arc::new(Mutex::new(Vec::<String>::new()));
        let app = Router::new()
            .route(
                "/events",
                post(
                    |State(captured): State<Arc<Mutex<Vec<String>>>>, body: String| async move {
                        captured.lock().await.push(body);
                        StatusCode::OK
                    },
                ),
            )
            .with_state(Arc::clone(&captured));

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind replay monitor");
        let addr = listener.local_addr().expect("replay monitor addr");
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("serve replay monitor");
        });

        let state = Arc::new(AtomicU8::new(GatewayState::Degraded as u8));
        let router = ITPEventRouter::new(Arc::clone(&state), addr.to_string());
        router.route(r#"{"event":"buffer-me"}"#.into()).await;
        assert_eq!(router.buffer_len(), 1);

        state.store(GatewayState::Recovering as u8, Ordering::Release);
        let remaining = router.replay_buffered().await;

        assert_eq!(remaining, 0);
        assert_eq!(captured.lock().await.len(), 1);

        server.abort();
    }
}
