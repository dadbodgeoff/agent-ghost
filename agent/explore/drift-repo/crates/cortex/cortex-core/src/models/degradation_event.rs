use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A degradation event when a subsystem falls back to a lower-quality mode.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct DegradationEvent {
    pub component: String,
    pub failure: String,
    pub fallback_used: String,
    pub timestamp: DateTime<Utc>,
}
