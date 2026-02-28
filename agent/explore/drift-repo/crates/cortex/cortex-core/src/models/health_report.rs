use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Comprehensive health report for all subsystems.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HealthReport {
    pub overall_status: HealthStatus,
    pub subsystems: Vec<SubsystemHealth>,
    pub metrics: HealthMetrics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SubsystemHealth {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct HealthMetrics {
    pub total_memories: usize,
    pub active_memories: usize,
    pub archived_memories: usize,
    pub average_confidence: f64,
    pub db_size_bytes: u64,
    pub embedding_cache_hit_rate: f64,
}
