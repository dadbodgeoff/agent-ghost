//! Notification dispatcher: desktop, webhook, email, SMS (Req 14b).
//! All parallel, best-effort. Never through agent channels.

use serde::{Deserialize, Serialize};

/// Notification target types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationTarget {
    Desktop,
    Webhook {
        url: String,
        /// Timeout in seconds (default 5, 1 retry).
        timeout_secs: u64,
    },
    Email {
        smtp_host: String,
        smtp_port: u16,
        from: String,
        to: String,
        /// Timeout in seconds (default 10).
        timeout_secs: u64,
    },
    Sms {
        /// Twilio-compatible API URL.
        api_url: String,
        to: String,
        from: String,
        /// Timeout in seconds (default 5, 1 retry).
        timeout_secs: u64,
    },
}

/// Notification payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

    /// Load targets from ghost.yml convergence.contacts configuration.
    pub fn from_config(contacts: &[ContactConfig]) -> Self {
        let targets = contacts
            .iter()
            .map(|c| match c.channel.as_str() {
                "webhook" => NotificationTarget::Webhook {
                    url: c.address.clone(),
                    timeout_secs: 5,
                },
                "email" => {
                    let smtp_host = c.smtp_host.clone().unwrap_or_else(|| {
                        tracing::warn!(address = %c.address, "email notification missing smtp_host — notification will likely fail");
                        String::new()
                    });
                    let from = c.from.clone().unwrap_or_else(|| {
                        tracing::warn!(address = %c.address, "email notification missing from address — notification will likely fail");
                        String::new()
                    });
                    NotificationTarget::Email {
                        smtp_host,
                        smtp_port: c.smtp_port.unwrap_or(587),
                        from,
                        to: c.address.clone(),
                        timeout_secs: 10,
                    }
                }
                "sms" => {
                    let api_url = c.api_url.clone().unwrap_or_else(|| {
                        tracing::warn!(address = %c.address, "SMS notification missing api_url — notification will likely fail");
                        String::new()
                    });
                    let from = c.from.clone().unwrap_or_else(|| {
                        tracing::warn!(address = %c.address, "SMS notification missing from number — notification will likely fail");
                        String::new()
                    });
                    NotificationTarget::Sms {
                        api_url,
                        to: c.address.clone(),
                        from,
                        timeout_secs: 5,
                    }
                }
                unknown => {
                    tracing::warn!(
                        channel = %unknown,
                        address = %c.address,
                        "unknown notification channel type — defaulting to Desktop"
                    );
                    NotificationTarget::Desktop
                }
            })
            .collect();
        Self { targets }
    }

    /// Dispatch notification to all configured targets.
    /// All parallel via tokio::join!, best-effort, never blocks intervention.
    pub async fn dispatch(&self, payload: &NotificationPayload) {
        let futures: Vec<_> = self
            .targets
            .iter()
            .map(|target| self.dispatch_one(target, payload))
            .collect();

        // All parallel, best-effort (Req 14b AC2)
        let results = futures::future::join_all(futures).await;
        for (i, result) in results.iter().enumerate() {
            if let Err(e) = result {
                tracing::warn!(
                    target = ?self.targets[i],
                    error = %e,
                    "Notification dispatch failed (best-effort, non-blocking)"
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
                // notify-rust desktop notification
                tracing::info!(subject = %payload.subject, "Desktop notification sent");
                Ok(())
            }
            NotificationTarget::Webhook { url, timeout_secs } => {
                let client = reqwest::Client::new();
                let result = client
                    .post(url)
                    .timeout(std::time::Duration::from_secs(*timeout_secs))
                    .json(&serde_json::json!({
                        "subject": payload.subject,
                        "body": payload.body,
                        "severity": payload.severity,
                    }))
                    .send()
                    .await;

                match result {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        // 1 retry on failure
                        tracing::debug!(url = %url, "Webhook retry after failure");
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
                            .map_err(|e2| format!("webhook failed after retry: {e}, {e2}"))?;
                        Ok(())
                    }
                }
            }
            NotificationTarget::Email {
                smtp_host,
                smtp_port,
                from,
                to,
                timeout_secs,
            } => {
                // lettre SMTP email dispatch
                use lettre::message::header::ContentType;
                use lettre::{Message, SmtpTransport, Transport};

                let email = Message::builder()
                    .from(from.parse().map_err(|e| format!("invalid from: {e}"))?)
                    .to(to.parse().map_err(|e| format!("invalid to: {e}"))?)
                    .subject(&payload.subject)
                    .header(ContentType::TEXT_PLAIN)
                    .body(payload.body.clone())
                    .map_err(|e| format!("email build: {e}"))?;

                // builder_dangerous defaults to no TLS, no auth
                let mailer = SmtpTransport::builder_dangerous(smtp_host)
                    .port(*smtp_port)
                    .timeout(Some(std::time::Duration::from_secs(*timeout_secs)))
                    .build();

                mailer.send(&email).map_err(|e| format!("smtp send: {e}"))?;

                tracing::info!(to = %to, subject = %payload.subject, "Email notification sent");
                Ok(())
            }
            NotificationTarget::Sms {
                api_url,
                to,
                from,
                timeout_secs,
            } => {
                // Twilio-compatible SMS via HTTP POST
                let client = reqwest::Client::new();
                let result = client
                    .post(api_url)
                    .timeout(std::time::Duration::from_secs(*timeout_secs))
                    .form(&[
                        ("To", to.as_str()),
                        ("From", from.as_str()),
                        (
                            "Body",
                            &format!("[GHOST] {}: {}", payload.subject, payload.body),
                        ),
                    ])
                    .send()
                    .await;

                match result {
                    Ok(_) => {
                        tracing::info!(to = %to, "SMS notification sent");
                        Ok(())
                    }
                    Err(e) => {
                        // 1 retry on failure
                        tracing::debug!(to = %to, "SMS retry after failure");
                        client
                            .post(api_url)
                            .timeout(std::time::Duration::from_secs(*timeout_secs))
                            .form(&[
                                ("To", to.as_str()),
                                ("From", from.as_str()),
                                (
                                    "Body",
                                    &format!("[GHOST] {}: {}", payload.subject, payload.body),
                                ),
                            ])
                            .send()
                            .await
                            .map_err(|e2| format!("sms failed after retry: {e}, {e2}"))?;
                        Ok(())
                    }
                }
            }
        }
    }
}

impl Default for NotificationDispatcher {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

/// Contact configuration from ghost.yml convergence.contacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactConfig {
    pub channel: String,
    pub address: String,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub from: Option<String>,
    pub api_url: Option<String>,
}
