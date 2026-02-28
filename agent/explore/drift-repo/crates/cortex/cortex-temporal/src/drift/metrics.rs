//! Drift metrics — KSI, confidence trajectory, contradiction density,
//! consolidation efficiency, and full snapshot assembly.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use rusqlite::params;

use cortex_core::errors::CortexResult;
use cortex_core::memory::MemoryType;
use cortex_core::models::{
    DriftSnapshot, GlobalDriftMetrics, TypeDriftMetrics,
};
use cortex_storage::pool::ReadPool;

use super::evidence_freshness;

/// Knowledge Stability Index for a specific memory type within a window.
///
/// KSI = 1.0 - change_events / (2 * memory_population)
/// Clamped to [0.0, 1.0]. KSI=1.0 means perfectly stable.
///
/// `memory_population` is the number of memories that existed at any point
/// during the window (transaction_time <= window_end). This correctly handles
/// both long-lived memories and recently-created ones. If the population is 0,
/// returns 1.0 (no memories = perfectly stable).
pub fn compute_ksi(
    readers: &Arc<ReadPool>,
    memory_type: Option<MemoryType>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<f64> {
    readers.with_conn(|conn| {
        // Count memories that existed during the window.
        // A memory is part of the population if it was created on or before
        // window_end (transaction_time <= window_end).
        let population: i64 = match memory_type {
            Some(ref mt) => {
                let mt_str = serde_json::to_string(mt)
                    .unwrap_or_default()
                    .trim_matches('"')
                    .to_string();
                conn.query_row(
                    "SELECT COUNT(*) FROM memories
                     WHERE memory_type = ?1
                       AND transaction_time <= ?2",
                    params![mt_str, window_end.to_rfc3339()],
                    |row| row.get(0),
                )
                .unwrap_or(0)
            }
            None => conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE transaction_time <= ?1",
                    params![window_end.to_rfc3339()],
                    |row| row.get(0),
                )
                .unwrap_or(0),
        };

        if population == 0 {
            return Ok(1.0);
        }

        // Count events in window (created + archived + modified)
        let type_filter = memory_type.map(|mt| {
            serde_json::to_string(&mt)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string()
        });

        let change_count: i64 = if let Some(ref mt_str) = type_filter {
            conn.query_row(
                "SELECT COUNT(*) FROM memory_events me
                 JOIN memories m ON me.memory_id = m.id
                 WHERE m.memory_type = ?1
                   AND me.recorded_at >= ?2
                   AND me.recorded_at <= ?3
                   AND me.event_type IN ('created', 'archived', 'content_updated',
                       'confidence_changed', 'tags_modified', 'consolidated', 'reclassified')",
                params![mt_str, window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM memory_events
                 WHERE recorded_at >= ?1
                   AND recorded_at <= ?2
                   AND event_type IN ('created', 'archived', 'content_updated',
                       'confidence_changed', 'tags_modified', 'consolidated', 'reclassified')",
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };

        let ksi = 1.0 - (change_count as f64) / (2.0 * population as f64);
        Ok(ksi.clamp(0.0, 1.0))
    })
}

/// Sample average confidence at N points across a window.
/// Returns a Vec<f64> of confidence values at each sample point.
///
/// Uses the `memory_events` table to compute historical confidence at each
/// sample point. For each memory that existed at a sample time, we find the
/// most recent `confidence_changed` or `created` event before that time and
/// use its confidence value. This gives the true historical confidence, not
/// the current DB value.
pub fn compute_confidence_trajectory(
    readers: &Arc<ReadPool>,
    memory_type: Option<MemoryType>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    sample_points: usize,
) -> CortexResult<Vec<f64>> {
    if sample_points == 0 {
        return Ok(vec![]);
    }

    let total_duration = window_end
        .signed_duration_since(window_start)
        .num_seconds();
    if total_duration <= 0 {
        return Ok(vec![]);
    }

    let step = total_duration / sample_points as i64;

    readers.with_conn(|conn| {
        let mut trajectory = Vec::with_capacity(sample_points);

        for i in 0..sample_points {
            let sample_time = window_start + Duration::seconds(step * (i as i64 + 1));
            let sample_ts = sample_time.to_rfc3339();

            // Use event-sourced historical confidence:
            // For each memory that existed at sample_time, find the most recent
            // confidence_changed or created event and extract the confidence value.
            let avg: f64 = match memory_type {
                Some(ref mt) => {
                    let mt_str = serde_json::to_string(mt)
                        .unwrap_or_default()
                        .trim_matches('"')
                        .to_string();
                    conn.query_row(
                        "SELECT COALESCE(AVG(latest_conf), 0.0) FROM ( \
                            SELECT me_outer.memory_id, ( \
                                SELECT COALESCE( \
                                    JSON_EXTRACT(me2.delta, '$.new'), \
                                    JSON_EXTRACT(me2.delta, '$.confidence'), \
                                    0.5 \
                                ) \
                                FROM memory_events me2 \
                                WHERE me2.memory_id = me_outer.memory_id \
                                  AND me2.recorded_at <= ?1 \
                                  AND me2.event_type IN ('confidence_changed', 'created') \
                                ORDER BY me2.recorded_at DESC LIMIT 1 \
                            ) as latest_conf \
                            FROM ( \
                                SELECT DISTINCT me3.memory_id \
                                FROM memory_events me3 \
                                INNER JOIN memories m ON me3.memory_id = m.id \
                                WHERE me3.recorded_at <= ?1 \
                                  AND m.memory_type = ?2 \
                                  AND me3.event_type = 'created' \
                            ) me_outer \
                        )",
                        params![sample_ts, mt_str],
                        |row| row.get(0),
                    )
                    .unwrap_or(0.0)
                }
                None => conn
                    .query_row(
                        "SELECT COALESCE(AVG(latest_conf), 0.0) FROM ( \
                            SELECT me_outer.memory_id, ( \
                                SELECT COALESCE( \
                                    JSON_EXTRACT(me2.delta, '$.new'), \
                                    JSON_EXTRACT(me2.delta, '$.confidence'), \
                                    0.5 \
                                ) \
                                FROM memory_events me2 \
                                WHERE me2.memory_id = me_outer.memory_id \
                                  AND me2.recorded_at <= ?1 \
                                  AND me2.event_type IN ('confidence_changed', 'created') \
                                ORDER BY me2.recorded_at DESC LIMIT 1 \
                            ) as latest_conf \
                            FROM ( \
                                SELECT DISTINCT me3.memory_id \
                                FROM memory_events me3 \
                                INNER JOIN memories m ON me3.memory_id = m.id \
                                WHERE me3.recorded_at <= ?1 \
                                  AND me3.event_type = 'created' \
                            ) me_outer \
                        )",
                        params![sample_ts],
                        |row| row.get(0),
                    )
                    .unwrap_or(0.0),
            };

            trajectory.push(avg);
        }

        Ok(trajectory)
    })
}

/// Contradiction density: new_contradictions / total_memories in window.
/// < 0.02 healthy, > 0.10 needs attention.
pub fn compute_contradiction_density(
    readers: &Arc<ReadPool>,
    memory_type: Option<MemoryType>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<f64> {
    readers.with_conn(|conn| {
        let type_filter = memory_type.map(|mt| {
            serde_json::to_string(&mt)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string()
        });

        // Count contradiction-related events in window
        // We look for RelationshipAdded events where the relationship is a contradiction
        let contradiction_count: i64 = if let Some(ref mt_str) = type_filter {
            conn.query_row(
                "SELECT COUNT(*) FROM memory_events me
                 JOIN memories m ON me.memory_id = m.id
                 WHERE m.memory_type = ?1
                   AND me.recorded_at >= ?2
                   AND me.recorded_at <= ?3
                   AND me.event_type = 'relationship_added'
                   AND me.delta LIKE '%contradict%'",
                params![mt_str, window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM memory_events
                 WHERE recorded_at >= ?1
                   AND recorded_at <= ?2
                   AND event_type = 'relationship_added'
                   AND delta LIKE '%contradict%'",
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };

        // Total active memories
        let total: i64 = if let Some(ref mt_str) = type_filter {
            conn.query_row(
                "SELECT COUNT(*) FROM memories
                 WHERE memory_type = ?1 AND archived = 0",
                params![mt_str],
                |row| row.get(0),
            )
            .unwrap_or(0)
        } else {
            conn.query_row(
                "SELECT COUNT(*) FROM memories WHERE archived = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0)
        };

        if total == 0 {
            return Ok(0.0);
        }

        Ok((contradiction_count as f64 / total as f64).clamp(0.0, 1.0))
    })
}

/// Consolidation efficiency: semantic_created / episodic_archived in window.
/// > 0.5 good, < 0.2 poor.
///
/// Uses LEFT JOIN to count events even when the original memory row has been
/// deleted (e.g., after consolidation cleanup). Falls back to the `created`
/// event's delta to determine memory_type when the memories table row is missing.
pub fn compute_consolidation_efficiency(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<f64> {
    readers.with_conn(|conn| {
        // Count semantic memories created in window.
        // LEFT JOIN memories to tolerate deleted rows; fall back to the
        // memory_type from the created event's delta JSON.
        let semantic_created: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_events me
                 LEFT JOIN memories m ON me.memory_id = m.id
                 WHERE COALESCE(m.memory_type,
                     JSON_EXTRACT(
                         (SELECT me2.delta FROM memory_events me2
                          WHERE me2.memory_id = me.memory_id
                            AND me2.event_type = 'created'
                          LIMIT 1),
                         '$.memory_type'
                     )
                 ) = 'semantic'
                   AND me.event_type = 'created'
                   AND me.recorded_at >= ?1
                   AND me.recorded_at <= ?2",
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        // Count episodic memories archived in window.
        let episodic_archived: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_events me
                 LEFT JOIN memories m ON me.memory_id = m.id
                 WHERE COALESCE(m.memory_type,
                     JSON_EXTRACT(
                         (SELECT me2.delta FROM memory_events me2
                          WHERE me2.memory_id = me.memory_id
                            AND me2.event_type = 'created'
                          LIMIT 1),
                         '$.memory_type'
                     )
                 ) = 'episodic'
                   AND me.event_type = 'archived'
                   AND me.recorded_at >= ?1
                   AND me.recorded_at <= ?2",
                params![window_start.to_rfc3339(), window_end.to_rfc3339()],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if episodic_archived == 0 {
            return Ok(if semantic_created > 0 { 1.0 } else { 0.0 });
        }

        Ok((semantic_created as f64 / episodic_archived as f64).clamp(0.0, 1.0))
    })
}

/// Assemble a full DriftSnapshot from all metrics.
pub fn compute_all_metrics(
    readers: &Arc<ReadPool>,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> CortexResult<DriftSnapshot> {
    let mut type_metrics = HashMap::new();

    // Compute per-type metrics for all types that have memories
    for mt in MemoryType::ALL.iter() {
        let count = readers.with_conn(|conn| {
            let mt_str = serde_json::to_string(mt)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let c: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memories WHERE memory_type = ?1 AND archived = 0",
                    params![mt_str],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            Ok(c as usize)
        })?;

        if count == 0 {
            continue;
        }

        let ksi = compute_ksi(readers, Some(*mt), window_start, window_end)?;
        let contradiction_density =
            compute_contradiction_density(readers, Some(*mt), window_start, window_end)?;

        let avg_confidence = readers.with_conn(|conn| {
            let mt_str = serde_json::to_string(mt)
                .unwrap_or_default()
                .trim_matches('"')
                .to_string();
            let avg: f64 = conn
                .query_row(
                    "SELECT COALESCE(AVG(confidence), 0.0) FROM memories
                     WHERE memory_type = ?1 AND archived = 0",
                    params![mt_str],
                    |row| row.get(0),
                )
                .unwrap_or(0.0);
            Ok(avg)
        })?;

        let efi = evidence_freshness::compute_evidence_freshness_index_for_type(readers, *mt)?;

        type_metrics.insert(
            *mt,
            TypeDriftMetrics {
                count,
                avg_confidence,
                ksi,
                contradiction_density,
                consolidation_efficiency: if *mt == MemoryType::Semantic || *mt == MemoryType::Episodic {
                    compute_consolidation_efficiency(readers, window_start, window_end)?
                } else {
                    0.0
                },
                evidence_freshness_index: efi,
            },
        );
    }

    // Compute global metrics
    let (total, active, archived, avg_conf) = readers.with_conn(|conn| {
        let total: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
            .unwrap_or(0);
        let active: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE archived = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let archived: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memories WHERE archived = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);
        let avg: f64 = conn
            .query_row(
                "SELECT COALESCE(AVG(confidence), 0.0) FROM memories WHERE archived = 0",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);
        Ok((total as usize, active as usize, archived as usize, avg))
    })?;

    let overall_ksi = compute_ksi(readers, None, window_start, window_end)?;
    let overall_contradiction_density =
        compute_contradiction_density(readers, None, window_start, window_end)?;
    let overall_efi = evidence_freshness::compute_evidence_freshness_index(readers)?;

    Ok(DriftSnapshot {
        timestamp: window_end,
        window_hours: window_end
            .signed_duration_since(window_start)
            .num_hours()
            .unsigned_abs(),
        type_metrics,
        module_metrics: HashMap::new(), // Module metrics require namespace info
        global: GlobalDriftMetrics {
            total_memories: total,
            active_memories: active,
            archived_memories: archived,
            avg_confidence: avg_conf,
            overall_ksi,
            overall_contradiction_density,
            overall_evidence_freshness: overall_efi,
        },
    })
}
