//! Notification dispatcher: desktop, webhook, email, SMS (Req 14b).
//! All parallel, best-effort. Never through agent channels.

use serde::{Deserialize, Serialize};

/// Notification target types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationTarget {
    Desktop,
    Webhook { url: String, timeout_secs: u64 },
    Email { smtp_host: String, to: String },
    Sms { api_url: String, to: String },
}

/// Notification payload.
#[derive(Debug, Clone)]
pub struct NotificationPayload {
    pub subject: String,
    pub body: String,
    pub severity: String,
}

/// Notification dispatcher. All dispatches are parallel and best-effort.
pub struct NotificationDispatcher {
    targets: Vec<NotificationTarget>,
}

impl NotificationDispatcher {
    pub fn new(targets: Vec<NotificationTarget>) -> Self {
        Self { targets }
    }

    /// Dispatch notification to all configured targets. Best-effort, never blocks.
    pub async fn dispatch(&self, payload: &NotificationPayload) {
        let futures: Vec<_> = self
            .targets
            .iter()
            .map(|target| self.dispatch_one(target, payload))
            .collect();

        // All parallel, best-effort
        let results = futures::future::join_all(futures).await;
        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                tracing::warn!(
                    target = ?self.targets[i],
                    error = %e,
                    "Notification dispatch failed (best-effort)"
                );
            }
        }
    }

    async fn dispatch_one(
        &self,
        target: &NotificationTarget,
        payload: &NotificationPayload,
    ) -> Result<(), String> {
        match target {
            NotificationTarget::Desktop => {
                tracing::info!(subject = %payload.subject, "Desktop notification");
                Ok(())
            }
            NotificationTarget::Webhook { url, timeout_secs } => {
                let client = reqwest::Client::new();
                client
                    .post(url)
                    .timeout(std::time::Duration::from_secs(*timeout_secs))
                    .json(&serde_json::json!({
                        "subject": payload.subject,
                        "body": payload.body,
                        "severity": payload.severity,
                    }))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
            NotificationTarget::Email { .. } => {
                // Placeholder for lettre SMTP integration
                tracing::info!(subject = %payload.subject, "Email notification (stub)");
                Ok(())
            }
            NotificationTarget::Sms { .. } => {
                // Placeholder for Twilio integration
                tracing::info!(subject = %payload.subject, "SMS notification (stub)");
                Ok(())
            }
        }
    }
}

impl Default for NotificationDispatcher {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}
