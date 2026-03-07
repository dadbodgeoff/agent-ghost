//! Session boundary proxy: reads caps from shared state file (A34 Gap 9).

use std::time::Duration;

/// Session boundary configuration.
#[derive(Debug, Clone)]
pub struct SessionBoundaryConfig {
    pub max_duration: Duration,
    pub min_gap: Duration,
}

impl Default for SessionBoundaryConfig {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(6 * 3600), // 6 hours
            min_gap: Duration::from_secs(300),           // 5 minutes
        }
    }
}

/// Session boundary proxy that reads from shared state file.
pub struct SessionBoundaryProxy {
    config: SessionBoundaryConfig,
}

impl SessionBoundaryProxy {
    pub fn new(config: SessionBoundaryConfig) -> Self {
        Self { config }
    }

    /// Check if a new session can be created (min_gap enforcement).
    pub fn can_create_session(
        &self,
        last_session_end: Option<chrono::DateTime<chrono::Utc>>,
    ) -> bool {
        match last_session_end {
            None => true,
            Some(end) => {
                let elapsed = chrono::Utc::now() - end;
                elapsed.to_std().unwrap_or(Duration::ZERO) >= self.config.min_gap
            }
        }
    }

    /// Check if a session has exceeded max duration.
    pub fn is_session_expired(&self, session_start: chrono::DateTime<chrono::Utc>) -> bool {
        let elapsed = chrono::Utc::now() - session_start;
        elapsed.to_std().unwrap_or(Duration::ZERO) >= self.config.max_duration
    }

    pub fn config(&self) -> &SessionBoundaryConfig {
        &self.config
    }
}

impl Default for SessionBoundaryProxy {
    fn default() -> Self {
        Self::new(SessionBoundaryConfig::default())
    }
}
