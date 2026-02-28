//! Evolution pattern detectors — crystallization, erosion, explosion, conflict wave.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};

use cortex_core::errors::CortexResult;
use cortex_storage::pool::ReadPool;

/// Crystallization pattern: episodic → semantic → validated → stable confidence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrystallizationPattern {
    /// Memory IDs that have crystallized.
    pub memory_ids: Vec<String>,
    /// Current stage in the lifecycle.
    pub stage: CrystallizationStage,
    /// Time from first episodic to current stage.
    pub time_to_current_stage_hours: f64,
    /// Recommended action.
    pub recommended_action: String,
}

/// Stages in the crystallization lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrystallizationStage {
    Episodic,
    Semantic,
    Validated,
    Stable,
}

/// Erosion pattern: confidence declining over consecutive windows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErosionPattern {
    /// Memory IDs with declining confidence.
    pub affected_memories: Vec<String>,
    /// Number of consecutive declining windows.
    pub declining_windows: usize,
    /// Average confidence decline per window.
    pub avg_decline_per_window: f64,
    /// Recommended action.
    pub recommended_action: String,
}

/// Explosion pattern: memory creation rate spiking above baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplosionPattern {
    /// Current creation rate (memories per day).
    pub current_rate: f64,
    /// Baseline creation rate.
    pub baseline_rate: f64,
    /// Standard deviations above baseline.
    pub sigma_above: f64,
    /// Recommended action.
    pub recommended_action: String,
}

/// Conflict wave pattern: contradiction density spike in a specific area.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictWavePattern {
    /// Hotspot area (memory type or module).
    pub hotspot: String,
    /// Current contradiction density in hotspot.
    pub density: f64,
    /// Baseline density.
    pub baseline_density: f64,
    /// Recommended action.
    pub recommended_action: String,
}

/// Detect crystallization pattern — tracks lifecycle from episodic to stable.
///
/// Returns `None` if no crystallization pattern is detected.
pub fn detect_crystallization(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<Option<CrystallizationPattern>> {
    readers.with_conn(|conn| {
        // Find memories that were reclassified from episodic to semantic in window
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT me.memory_id FROM memory_events me
                 WHERE me.event_type = 'reclassified'
                   AND me.recorded_at >= ?1
                   AND me.recorded_at <= ?2
                   AND me.delta LIKE '%episodic%'
                   AND me.delta LIKE '%semantic%'",
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

        let memory_ids: Vec<String> = stmt
            .query_map(
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if memory_ids.is_empty() {
            return Ok(None);
        }

        let hours = window_end
            .signed_duration_since(window_start)
            .num_hours() as f64;

        Ok(Some(CrystallizationPattern {
            memory_ids,
            stage: CrystallizationStage::Semantic,
            time_to_current_stage_hours: hours,
            recommended_action: "Continue validation to promote to Verified status".to_string(),
        }))
    })
}

/// Detect erosion pattern — confidence declining over consecutive windows.
///
/// Returns `None` if no erosion pattern is detected.
pub fn detect_erosion(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<Option<ErosionPattern>> {
    readers.with_conn(|conn| {
        // Find memories with multiple confidence declines in window
        let mut stmt = conn
            .prepare(
                "SELECT me.memory_id, COUNT(*) as decline_count
                 FROM memory_events me
                 WHERE me.event_type = 'decayed'
                   AND me.recorded_at >= ?1
                   AND me.recorded_at <= ?2
                 GROUP BY me.memory_id
                 HAVING decline_count >= 2
                 ORDER BY decline_count DESC
                 LIMIT 100",
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

        let rows: Vec<(String, i64)> = stmt
            .query_map(
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        if rows.is_empty() {
            return Ok(None);
        }

        let affected: Vec<String> = rows.iter().map(|(id, _)| id.clone()).collect();
        let avg_declines: f64 =
            rows.iter().map(|(_, c)| *c as f64).sum::<f64>() / rows.len() as f64;

        Ok(Some(ErosionPattern {
            affected_memories: affected,
            declining_windows: avg_declines as usize,
            avg_decline_per_window: 0.02, // Approximate per-window decline
            recommended_action: "Review and validate affected memories to halt erosion"
                .to_string(),
        }))
    })
}

/// Detect explosion pattern — memory creation rate spiking above baseline.
///
/// Returns `None` if no explosion is detected.
pub fn detect_explosion(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    sigma_threshold: f64,
) -> CortexResult<Option<ExplosionPattern>> {
    readers.with_conn(|conn| {
        let window_days = window_end
            .signed_duration_since(window_start)
            .num_days()
            .max(1) as f64;

        // Count creations in window
        let window_created: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_events
                 WHERE event_type = 'created'
                   AND recorded_at >= ?1
                   AND recorded_at <= ?2",
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let current_rate = window_created as f64 / window_days;

        // Compute baseline from older data (3× window before window_start)
        let baseline_start = window_start
            - chrono::Duration::seconds(
                window_end
                    .signed_duration_since(window_start)
                    .num_seconds()
                    * 3,
            );

        let baseline_days = window_start
            .signed_duration_since(baseline_start)
            .num_days()
            .max(1) as f64;

        let baseline_created: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_events
                 WHERE event_type = 'created'
                   AND recorded_at >= ?1
                   AND recorded_at < ?2",
                params![baseline_start.to_rfc3339(), window_start.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let baseline_rate = baseline_created as f64 / baseline_days;

        if baseline_rate <= 0.0 {
            return Ok(None);
        }

        // Simple stddev approximation: use baseline_rate * 0.5 as stddev
        let stddev = (baseline_rate * 0.5).max(0.1);
        let sigma_above = (current_rate - baseline_rate) / stddev;

        if sigma_above > sigma_threshold {
            Ok(Some(ExplosionPattern {
                current_rate,
                baseline_rate,
                sigma_above,
                recommended_action: "Trigger consolidation to reduce memory volume".to_string(),
            }))
        } else {
            Ok(None)
        }
    })
}

/// Detect conflict wave — contradiction density spike concentrated in specific area.
///
/// Returns `None` if no conflict wave is detected.
pub fn detect_conflict_wave(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<Option<ConflictWavePattern>> {
    readers.with_conn(|conn| {
        // Find memory types with high contradiction density
        let mut stmt = conn
            .prepare(
                "SELECT m.memory_type, COUNT(*) as contradiction_count
                 FROM memory_events me
                 JOIN memories m ON me.memory_id = m.id
                 WHERE me.event_type = 'relationship_added'
                   AND me.delta LIKE '%contradict%'
                   AND me.recorded_at >= ?1
                   AND me.recorded_at <= ?2
                 GROUP BY m.memory_type
                 ORDER BY contradiction_count DESC
                 LIMIT 1",
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?;

        let hotspot: Option<(String, i64)> = stmt
            .query_map(
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|e| cortex_storage::to_storage_err(e.to_string()))?
            .filter_map(|r| r.ok())
            .next();

        let (hotspot_type, count) = match hotspot {
            Some(h) => h,
            None => return Ok(None),
        };

        // Get total memories of that type
        let total: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE memory_type = ?1 AND archived = 0",
                params![hotspot_type],
                |row| row.get(0),
            )
            .unwrap_or(1);

        let density = count as f64 / total as f64;

        // Only report if density is significant (> 2× a baseline of 0.02)
        let baseline = 0.02;
        if density > baseline * 2.0 {
            Ok(Some(ConflictWavePattern {
                hotspot: hotspot_type,
                density,
                baseline_density: baseline,
                recommended_action: "Run targeted validation on the affected memory type"
                    .to_string(),
            }))
        } else {
            Ok(None)
        }
    })
}
