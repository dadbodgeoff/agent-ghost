//! Drift snapshot storage — store, retrieve, and query drift snapshots.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use cortex_core::errors::CortexResult;
use cortex_core::models::DriftSnapshot;
use cortex_storage::pool::{ReadPool, WriteConnection};
use cortex_storage::queries::drift_ops;

/// Store a drift snapshot to the database.
///
/// Uses `with_conn` (async-safe) to avoid blocking the tokio runtime.
pub async fn store_drift_snapshot(
    writer: &Arc<WriteConnection>,
    snapshot: &DriftSnapshot,
) -> CortexResult<u64> {
    let ts = snapshot.timestamp.to_rfc3339();
    let window_seconds = (snapshot.window_hours * 3600) as i64;
    let json = serde_json::to_string(snapshot)
        .map_err(cortex_core::CortexError::SerializationError)?;

    writer
        .with_conn(move |conn| drift_ops::insert_drift_snapshot(conn, &ts, window_seconds, &json))
        .await
}

/// Get drift snapshots within a time range.
pub fn get_drift_snapshots(
    readers: &Arc<ReadPool>,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> CortexResult<Vec<DriftSnapshot>> {
    readers.with_conn(|conn| {
        let raw = drift_ops::get_drift_snapshots(conn, &from.to_rfc3339(), &to.to_rfc3339())?;
        let mut snapshots = Vec::with_capacity(raw.len());
        for r in raw {
            let snapshot: DriftSnapshot = serde_json::from_str(&r.metrics)
                .map_err(cortex_core::CortexError::SerializationError)?;
            snapshots.push(snapshot);
        }
        Ok(snapshots)
    })
}

/// Get the most recent drift snapshot.
pub fn get_latest_drift_snapshot(
    readers: &Arc<ReadPool>,
) -> CortexResult<Option<DriftSnapshot>> {
    readers.with_conn(|conn| {
        let raw = drift_ops::get_latest_drift_snapshot(conn)?;
        match raw {
            Some(r) => {
                let snapshot: DriftSnapshot = serde_json::from_str(&r.metrics)
                    .map_err(cortex_core::CortexError::SerializationError)?;
                Ok(Some(snapshot))
            }
            None => Ok(None),
        }
    })
}
