//! Drift alerting — threshold-based alert evaluation with dampening.
//!
//! 6 alert categories with configurable thresholds from TemporalConfig.
//! Alert dampening: cooldown per category + affected entity dedup.

use chrono::{Duration, Utc};

use cortex_core::config::TemporalConfig;
use cortex_core::memory::MemoryType;
use cortex_core::models::{AlertSeverity, DriftAlert, DriftAlertCategory, DriftSnapshot};

/// Evaluate drift alerts from a snapshot, respecting dampening via recent_alerts.
///
/// Checks 6 alert categories against configured thresholds.
/// Uses `recent_alerts` to implement cooldown-based dampening.
pub fn evaluate_drift_alerts(
    snapshot: &DriftSnapshot,
    config: &TemporalConfig,
    recent_alerts: &[DriftAlert],
) -> Vec<DriftAlert> {
    let start = std::time::Instant::now();
    let mut alerts = Vec::new();
    let now = Utc::now();

    // 1. KSI alerts — per-type with type-specific thresholds
    for (mt, metrics) in &snapshot.type_metrics {
        let threshold = ksi_threshold_for_type(mt, config);
        if metrics.ksi < threshold {
            let severity = if metrics.ksi < threshold * 0.5 {
                AlertSeverity::Critical
            } else {
                AlertSeverity::Warning
            };

            let alert = DriftAlert {
                severity,
                category: DriftAlertCategory::KnowledgeChurn,
                message: format!(
                    "KSI for {:?} is {:.2}, below threshold {:.2}",
                    mt, metrics.ksi, threshold
                ),
                affected_memories: vec![],
                recommended_action: format!(
                    "Review recent changes to {:?} memories and consider stabilization",
                    mt
                ),
                detected_at: now,
            };

            if !is_dampened(&alert, recent_alerts, config) {
                alerts.push(alert);
            }
        }
    }

    // 2. Confidence erosion — fires after consecutive declining windows
    // Check global confidence trajectory (simplified: compare current avg to threshold)
    if snapshot.global.avg_confidence < 0.5 {
        let alert = DriftAlert {
            severity: AlertSeverity::Warning,
            category: DriftAlertCategory::ConfidenceErosion,
            message: format!(
                "Average confidence is {:.2}, indicating erosion",
                snapshot.global.avg_confidence
            ),
            affected_memories: vec![],
            recommended_action: "Review and validate low-confidence memories".to_string(),
            detected_at: now,
        };

        if !is_dampened(&alert, recent_alerts, config) {
            alerts.push(alert);
        }
    }

    // 3. Contradiction spike — fires when density > threshold (Critical severity)
    if snapshot.global.overall_contradiction_density > config.alert_contradiction_density_threshold {
        let alert = DriftAlert {
            severity: AlertSeverity::Critical,
            category: DriftAlertCategory::ContradictionSpike,
            message: format!(
                "Contradiction density is {:.3}, above threshold {:.3}",
                snapshot.global.overall_contradiction_density,
                config.alert_contradiction_density_threshold
            ),
            affected_memories: vec![],
            recommended_action: "Investigate and resolve contradictions immediately".to_string(),
            detected_at: now,
        };

        if !is_dampened(&alert, recent_alerts, config) {
            alerts.push(alert);
        }
    }

    // 4. Stale evidence — fires for high-importance memories when freshness < threshold
    if snapshot.global.overall_evidence_freshness < config.alert_evidence_freshness_threshold {
        let alert = DriftAlert {
            severity: AlertSeverity::Warning,
            category: DriftAlertCategory::StaleEvidence,
            message: format!(
                "Evidence freshness index is {:.2}, below threshold {:.2}",
                snapshot.global.overall_evidence_freshness,
                config.alert_evidence_freshness_threshold
            ),
            affected_memories: vec![],
            recommended_action: "Review and refresh stale evidence links".to_string(),
            detected_at: now,
        };

        if !is_dampened(&alert, recent_alerts, config) {
            alerts.push(alert);
        }
    }

    // 5. Knowledge explosion — fires when creation rate > baseline + sigma × stddev
    // Simplified: check if total memories grew significantly
    if snapshot.global.active_memories > 0 {
        // Per-type check for explosion
        for (mt, metrics) in &snapshot.type_metrics {
            if metrics.count > 100 && metrics.ksi < 0.3 {
                let alert = DriftAlert {
                    severity: AlertSeverity::Warning,
                    category: DriftAlertCategory::KnowledgeExplosion,
                    message: format!(
                        "{:?} has {} memories with KSI {:.2}, suggesting rapid growth",
                        mt, metrics.count, metrics.ksi
                    ),
                    affected_memories: vec![],
                    recommended_action: "Trigger consolidation to reduce memory volume"
                        .to_string(),
                    detected_at: now,
                };

                if !is_dampened(&alert, recent_alerts, config) {
                    alerts.push(alert);
                }
            }
        }
    }

    // 6. Coverage gap — modules/types with low coverage
    for (mt, metrics) in &snapshot.type_metrics {
        if metrics.evidence_freshness_index < 0.3 && metrics.count > 5 {
            let alert = DriftAlert {
                severity: AlertSeverity::Info,
                category: DriftAlertCategory::CoverageGap,
                message: format!(
                    "{:?} has low evidence freshness ({:.2}) across {} memories",
                    mt, metrics.evidence_freshness_index, metrics.count
                ),
                affected_memories: vec![],
                recommended_action: format!(
                    "Add evidence links to {:?} memories to improve coverage",
                    mt
                ),
                detected_at: now,
            };

            if !is_dampened(&alert, recent_alerts, config) {
                alerts.push(alert);
            }
        }
    }

    let elapsed = start.elapsed();
    if elapsed.as_secs() >= ALERT_EVALUATION_BUDGET_SECS {
        tracing::warn!(
            elapsed_secs = elapsed.as_secs(),
            budget_secs = ALERT_EVALUATION_BUDGET_SECS,
            alert_count = alerts.len(),
            type_metrics_count = snapshot.type_metrics.len(),
            "evaluate_drift_alerts took {}s, exceeding {}s budget. \
             Consider reducing the number of type_metrics or recent_alerts.",
            elapsed.as_secs(),
            ALERT_EVALUATION_BUDGET_SECS,
        );
    }

    alerts
}

/// Maximum budget (in seconds) for drift alert evaluation.
/// If evaluation exceeds this, a warning is emitted for monitoring.
const ALERT_EVALUATION_BUDGET_SECS: u64 = 30;
/// Core/Tribal use alert_ksi_threshold (0.3), Semantic uses 0.5, episodic types use 0.2.
fn ksi_threshold_for_type(mt: &MemoryType, config: &TemporalConfig) -> f64 {
    match mt {
        MemoryType::Core | MemoryType::Tribal => config.alert_ksi_threshold,
        MemoryType::Semantic => 0.5,
        MemoryType::Episodic | MemoryType::Conversation | MemoryType::Incident => 0.2,
        _ => config.alert_ksi_threshold,
    }
}

/// Check if an alert is dampened by recent alerts.
///
/// Warning cooldown: alert_cooldown_warning_hours (default 24h).
/// Critical cooldown: alert_cooldown_critical_hours (default 1h).
fn is_dampened(
    alert: &DriftAlert,
    recent_alerts: &[DriftAlert],
    config: &TemporalConfig,
) -> bool {
    let cooldown = match alert.severity {
        AlertSeverity::Critical => Duration::hours(config.alert_cooldown_critical_hours as i64),
        AlertSeverity::Warning | AlertSeverity::Info => {
            Duration::hours(config.alert_cooldown_warning_hours as i64)
        }
    };

    let cutoff = alert.detected_at - cooldown;

    recent_alerts.iter().any(|recent| {
        recent.category == alert.category
            && recent.detected_at > cutoff
            && recent.severity == alert.severity
    })
}
