//! Auto-refresh scheduler for materialized views.
//!
//! Creates views at configurable intervals (default: 14 days / sprint boundaries).
//! Skips creation if no events have been recorded since the last view.

use std::sync::Arc;

use chrono::{Duration, Utc};

use cortex_core::config::TemporalConfig;
use cortex_core::errors::CortexResult;
use cortex_storage::pool::ReadPool;

use super::query;

/// Scheduler that determines when to auto-create materialized views.
pub struct AutoRefreshScheduler {
    config: TemporalConfig,
}

impl AutoRefreshScheduler {
    /// Create a new scheduler with the given config.
    pub fn new(config: TemporalConfig) -> Self {
        Self { config }
    }

    /// Check whether a new auto-created view should be created.
    ///
    /// Returns `Some(label)` (e.g. "auto-2026-02-07") if:
    /// - No auto-created view exists yet, OR
    /// - The interval since the last auto-created view has elapsed
    /// - AND there have been changes since the last view
    ///
    /// Returns `None` if the interval hasn't elapsed or there are no changes.
    pub fn should_create_view(
        &self,
        readers: &Arc<ReadPool>,
    ) -> CortexResult<Option<String>> {
        let views = query::list_views(readers)?;
        let last_auto = views.iter().find(|v| v.auto_refresh);

        let interval = Duration::days(self.config.materialized_view_auto_interval_days as i64);
        let now = Utc::now();

        match last_auto {
            Some(view) => {
                let elapsed = now - view.timestamp;
                if elapsed < interval {
                    return Ok(None); // Interval not yet elapsed
                }

                // Check if there have been changes since the last view
                if !self.has_changes_since_last_view(readers, view.timestamp)? {
                    return Ok(None); // No changes — skip
                }

                let label = format!("auto-{}", now.format("%Y-%m-%d"));
                Ok(Some(label))
            }
            None => {
                // No auto-created view exists yet — create one
                let label = format!("auto-{}", now.format("%Y-%m-%d"));
                Ok(Some(label))
            }
        }
    }

    /// Check if any events have been recorded since the given timestamp.
    fn has_changes_since_last_view(
        &self,
        readers: &Arc<ReadPool>,
        since: chrono::DateTime<chrono::Utc>,
    ) -> CortexResult<bool> {
        let ts = since.to_rfc3339();
        readers.with_conn(move |conn| {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_events WHERE recorded_at > ?1",
                    rusqlite::params![ts],
                    |row| row.get(0),
                )
                .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;
            Ok(count > 0)
        })
    }
}
