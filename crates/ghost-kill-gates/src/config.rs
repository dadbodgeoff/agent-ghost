//! Kill gate configuration.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Configuration for distributed kill gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillGateConfig {
    /// Whether distributed kill gates are enabled.
    pub enabled: bool,
    /// Maximum time to wait for propagation acks before fail-closed.
    pub max_propagation: Duration,
    /// Quorum size for distributed resume. `None` = auto (ceil(n/2) + 1).
    pub quorum_size: Option<usize>,
    /// Heartbeat interval for liveness detection.
    pub heartbeat_interval: Duration,
    /// Time without heartbeat before declaring a node partitioned.
    pub partition_timeout: Duration,
    /// Whether to verify hash chains on node sync/rejoin.
    pub chain_verify_on_sync: bool,
    /// Whether authenticated cluster membership is configured for resume quorum.
    pub authenticated_cluster_membership: bool,
}

impl Default for KillGateConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_propagation: Duration::from_millis(500),
            quorum_size: None,
            heartbeat_interval: Duration::from_millis(1000),
            partition_timeout: Duration::from_millis(3000),
            chain_verify_on_sync: true,
            authenticated_cluster_membership: false,
        }
    }
}

impl KillGateConfig {
    /// Compute effective quorum size for a given cluster size.
    /// Auto: ceil(n/2) + 1, minimum 1.
    pub fn effective_quorum(&self, cluster_size: usize) -> usize {
        if let Some(q) = self.quorum_size {
            return q.min(cluster_size).max(1);
        }
        if cluster_size == 0 {
            return 1;
        }
        (cluster_size / 2) + 1
    }
}
