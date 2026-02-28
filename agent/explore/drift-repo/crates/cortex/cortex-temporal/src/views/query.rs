//! View lookup and diff between views.

use std::sync::Arc;

use cortex_core::errors::{CortexResult, TemporalError};
use cortex_core::models::{
    DiffScope, EventActor, MaterializedTemporalView, TemporalDiff, TemporalDiffQuery,
};
use cortex_core::CortexError;
use cortex_storage::pool::ReadPool;
use cortex_storage::queries::view_ops;

/// Get a materialized view by label.
pub fn get_view(
    readers: &Arc<ReadPool>,
    label: &str,
) -> CortexResult<Option<MaterializedTemporalView>> {
    let lbl = label.to_string();
    readers.with_conn(move |conn| {
        let raw = view_ops::get_view_by_label(conn, &lbl)?;
        match raw {
            Some(r) => Ok(Some(raw_to_view(r)?)),
            None => Ok(None),
        }
    })
}

/// List all materialized views.
pub fn list_views(readers: &Arc<ReadPool>) -> CortexResult<Vec<MaterializedTemporalView>> {
    readers.with_conn(|conn| {
        let raw_views = view_ops::list_views(conn)?;
        let mut views = Vec::with_capacity(raw_views.len());
        for r in raw_views {
            views.push(raw_to_view(r)?);
        }
        Ok(views)
    })
}

/// Diff between two materialized views by label.
///
/// This is cheap — it delegates to the existing diff engine with the two
/// view timestamps. The views just provide convenient named time points.
pub fn diff_views(
    readers: &Arc<ReadPool>,
    label_a: &str,
    label_b: &str,
) -> CortexResult<TemporalDiff> {
    let view_a = get_view(readers, label_a)?.ok_or_else(|| {
        CortexError::TemporalError(TemporalError::QueryFailed(format!(
            "view not found: {}",
            label_a
        )))
    })?;
    let view_b = get_view(readers, label_b)?.ok_or_else(|| {
        CortexError::TemporalError(TemporalError::QueryFailed(format!(
            "view not found: {}",
            label_b
        )))
    })?;

    let query = TemporalDiffQuery {
        time_a: view_a.timestamp,
        time_b: view_b.timestamp,
        scope: DiffScope::All,
    };

    readers.with_conn(|conn| crate::query::diff::execute_diff(conn, &query))
}

/// Convert a raw database row to a MaterializedTemporalView.
fn raw_to_view(
    raw: view_ops::RawView,
) -> CortexResult<MaterializedTemporalView> {
    let timestamp = chrono::DateTime::parse_from_rfc3339(&raw.timestamp)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .map_err(|e| {
            CortexError::TemporalError(TemporalError::QueryFailed(format!(
                "invalid timestamp: {}",
                e
            )))
        })?;

    let snapshot_ids: Vec<u64> = serde_json::from_str(&raw.snapshot_ids)
        .map_err(CortexError::SerializationError)?;

    let created_by: EventActor = serde_json::from_str(&raw.created_by)
        .unwrap_or(EventActor::System("unknown".to_string()));

    Ok(MaterializedTemporalView {
        view_id: raw.view_id,
        label: raw.label,
        timestamp,
        memory_count: raw.memory_count,
        snapshot_ids,
        drift_snapshot_id: raw.drift_snapshot_id,
        created_by,
        auto_refresh: raw.auto_refresh,
    })
}
