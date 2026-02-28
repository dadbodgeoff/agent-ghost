//! Session boundary enforcement.

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
            max_duration: Duration::from_secs(360 * 60), // 6 hours
            min_gap: Duration::from_secs(30 * 60),       // 30 minutes
        }
    }
}
