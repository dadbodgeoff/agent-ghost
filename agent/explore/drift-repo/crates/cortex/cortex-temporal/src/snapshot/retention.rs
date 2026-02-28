//! Snapshot retention policy.

use std::sync::Arc;

use chrono::{Duration, Utc};

use cortex_core::config::TemporalConfig;
use cortex_core::errors::CortexResult;
use cortex_storage::pool::WriteConnection;
use cortex_storage::queries::snapshot_ops;

/// Result of applying the retention policy.
#[derive(Debug, Clone)]
pub struct RetentionResult {
    pub snapshots_deleted: u64,
}

/// Apply the retention policy:
/// - Keep all snapshots for `snapshot_retention_full_days`
/// - After that, keep only monthly snapshots until `snapshot_retention_monthly_days`
/// - After that, delete everything
pub async fn apply_retention_policy(
    writer: &Arc<WriteConnection>,
    config: &TemporalConfig,
) -> CortexResult<RetentionResult> {
    let now = Utc::now();
    let full_cutoff =
        (now - Duration::days(config.snapshot_retention_full_days as i64)).to_rfc3339();
    let monthly_cutoff =
        (now - Duration::days(config.snapshot_retention_monthly_days as i64)).to_rfc3339();

    let full_cutoff_clone = full_cutoff.clone();
    let monthly_cutoff_clone = monthly_cutoff.clone();

    writer
        .with_conn(move |conn| {
            // Delete very old snapshots entirely
            let deleted_old =
                snapshot_ops::delete_old_snapshots(conn, &monthly_cutoff_clone, false)?;

            // For the middle range, keep only monthly
            let deleted_monthly =
                snapshot_ops::delete_old_snapshots(conn, &full_cutoff_clone, true)?;

            Ok(RetentionResult {
                snapshots_deleted: deleted_old + deleted_monthly,
            })
        })
        .await
}
