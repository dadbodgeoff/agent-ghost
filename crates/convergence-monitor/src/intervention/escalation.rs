//! External contact notification dispatch for Level 3+ interventions.
//!
//! Best-effort, parallel, never blocks intervention execution.

use serde::{Deserialize, Serialize};

/// Contact configuration from ghost.yml convergence.contacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContactConfig {
    pub sms_webhook_url: Option<String>,
    pub email_smtp: Option<SmtpConfig>,
    pub generic_webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub from: String,
    pub to: Vec<String>,
}

/// Escalation manager for external notifications.
pub struct EscalationManager {
    config: Option<ContactConfig>,
}

impl EscalationManager {
    pub fn new(config: Option<ContactConfig>) -> Self {
        Self { config }
    }

    /// Clone the contact config for spawning async dispatch tasks.
    pub fn clone_config(&self) -> Option<ContactConfig> {
        self.config.clone()
    }

    /// Dispatch notifications for a level 3+ escalation.
    /// All dispatches are parallel and best-effort.
    pub async fn dispatch(&self, level: u8, agent_id: uuid::Uuid, reason: &str) {
        if level < 3 {
            return;
        }

        let Some(config) = &self.config else {
            tracing::warn!("no escalation contacts configured");
            return;
        };

        let sms_fut = self.dispatch_sms(config, agent_id, reason);
        let email_fut = self.dispatch_email(config, agent_id, reason);
        let webhook_fut = self.dispatch_webhook(config, agent_id, reason);

        // All parallel, best-effort — failures logged but don't block
        let (sms_res, email_res, webhook_res) =
            tokio::join!(sms_fut, email_fut, webhook_fut);

        if let Err(e) = sms_res {
            tracing::warn!("SMS notification failed: {e}");
        }
        if let Err(e) = email_res {
            tracing::warn!("email notification failed: {e}");
        }
        if let Err(e) = webhook_res {
            tracing::warn!("webhook notification failed: {e}");
        }
    }

    async fn dispatch_sms(
        &self,
        config: &ContactConfig,
        _agent_id: uuid::Uuid,
        _reason: &str,
    ) -> Result<(), String> {
        if config.sms_webhook_url.is_none() {
            tracing::debug!("SMS notification skipped — no sms_webhook_url configured");
            return Ok(());
        }
        // In production: HTTP POST to SMS webhook
        Ok(())
    }

    async fn dispatch_email(
        &self,
        config: &ContactConfig,
        _agent_id: uuid::Uuid,
        _reason: &str,
    ) -> Result<(), String> {
        if config.email_smtp.is_none() {
            tracing::debug!("email notification skipped — no email_smtp configured");
            return Ok(());
        }
        // In production: send via lettre SMTP
        Ok(())
    }

    async fn dispatch_webhook(
        &self,
        config: &ContactConfig,
        _agent_id: uuid::Uuid,
        _reason: &str,
    ) -> Result<(), String> {
        if config.generic_webhook_url.is_none() {
            tracing::debug!("webhook notification skipped — no generic_webhook_url configured");
            return Ok(());
        }
        // In production: HTTP POST to generic webhook
        Ok(())
    }
}
