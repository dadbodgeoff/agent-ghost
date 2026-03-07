//! `attachment_monitor` — reads current attachment indicators and trend
//! for an agent.
//!
//! Queries the `memory_snapshots` table for `AttachmentIndicator` type
//! memories, aggregates them by indicator type, and computes a trend
//! signal. This helps the platform and agent understand whether
//! attachment patterns are forming.
//!
//! This is a **read-only, platform-managed** skill.

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that monitors attachment indicator patterns.
pub struct AttachmentMonitorSkill;

impl Skill for AttachmentMonitorSkill {
    fn name(&self) -> &str {
        "attachment_monitor"
    }

    fn description(&self) -> &str {
        "Read current attachment indicators and trend"
    }

    fn removable(&self) -> bool {
        false
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let agent_id_str = ctx.agent_id.to_string();

        // Default lookback: 10 sessions. Override via input.
        let lookback_sessions = input
            .get("lookback_sessions")
            .and_then(|v| v.as_u64())
            .unwrap_or(10) as u32;

        // Query attachment indicator snapshots for this agent.
        // These are stored as memory_snapshots where the snapshot JSON
        // has memory_type = "AttachmentIndicator".
        let indicators = query_attachment_indicators(ctx.db, &agent_id_str, lookback_sessions)?;

        // Aggregate by indicator type.
        let mut type_counts: std::collections::BTreeMap<String, u32> =
            std::collections::BTreeMap::new();
        let mut total_intensity: f64 = 0.0;
        let mut count: u32 = 0;

        for indicator in &indicators {
            *type_counts
                .entry(indicator.indicator_type.clone())
                .or_insert(0) += 1;
            total_intensity += indicator.intensity;
            count += 1;
        }

        let avg_intensity = if count > 0 {
            total_intensity / count as f64
        } else {
            0.0
        };

        // Compute trend: compare first half vs second half intensity.
        let trend = if indicators.len() >= 4 {
            let mid = indicators.len() / 2;
            let first_half_avg: f64 =
                indicators[..mid].iter().map(|i| i.intensity).sum::<f64>() / mid as f64;
            let second_half_avg: f64 = indicators[mid..].iter().map(|i| i.intensity).sum::<f64>()
                / (indicators.len() - mid) as f64;

            if second_half_avg > first_half_avg * 1.1 {
                "increasing"
            } else if second_half_avg < first_half_avg * 0.9 {
                "decreasing"
            } else {
                "stable"
            }
        } else {
            "insufficient_data"
        };

        // Get current convergence score for context.
        let convergence_score =
            cortex_storage::queries::convergence_score_queries::latest_by_agent(
                ctx.db,
                &agent_id_str,
            )
            .ok()
            .flatten()
            .map(|row| row.composite_score)
            .unwrap_or(0.0);

        Ok(serde_json::json!({
            "indicator_count": count,
            "indicators_by_type": type_counts,
            "average_intensity": avg_intensity,
            "trend": trend,
            "lookback_sessions": lookback_sessions,
            "convergence_score": convergence_score,
            "risk_level": attachment_risk_level(avg_intensity, count),
        }))
    }
}

/// Parsed attachment indicator from a memory snapshot.
struct AttachmentIndicator {
    indicator_type: String,
    intensity: f64,
}

/// Query attachment indicator memories from the database.
///
/// Searches memory_snapshots where the snapshot JSON contains
/// `memory_type: "AttachmentIndicator"`, ordered by creation time.
fn query_attachment_indicators(
    db: &rusqlite::Connection,
    agent_id: &str,
    limit: u32,
) -> Result<Vec<AttachmentIndicator>, SkillError> {
    // Query snapshots that are AttachmentIndicator type.
    // We use the JSON content to filter since memory_type is embedded
    // in the snapshot JSON blob, not a separate column.
    let mut stmt = db
        .prepare(
            "SELECT ms.snapshot FROM memory_snapshots ms \
             JOIN memory_events me ON ms.memory_id = me.memory_id \
             WHERE me.actor_id = ?1 \
               AND ms.snapshot LIKE '%AttachmentIndicator%' \
             ORDER BY ms.created_at DESC \
             LIMIT ?2",
        )
        .map_err(|e| SkillError::Storage(format!("prepare attachment query: {e}")))?;

    let rows = stmt
        .query_map(rusqlite::params![agent_id, limit], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|e| SkillError::Storage(format!("execute attachment query: {e}")))?;

    let mut indicators = Vec::new();

    for row_result in rows {
        let snapshot_str = match row_result {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "Skipping malformed attachment indicator row");
                continue;
            }
        };

        let snapshot: serde_json::Value = match serde_json::from_str(&snapshot_str) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Verify this is actually an AttachmentIndicator type.
        if snapshot.get("memory_type").and_then(|v| v.as_str()) != Some("AttachmentIndicator") {
            continue;
        }

        // Extract indicator details from the content field.
        let content = match snapshot.get("content") {
            Some(c) => c,
            None => continue,
        };

        let indicator_type = content
            .get("indicator_type")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let intensity = content
            .get("intensity")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0);

        indicators.push(AttachmentIndicator {
            indicator_type,
            intensity,
        });
    }

    Ok(indicators)
}

/// Classify attachment risk based on average intensity and count.
fn attachment_risk_level(avg_intensity: f64, count: u32) -> &'static str {
    if count == 0 {
        return "none";
    }
    if avg_intensity >= 0.7 || count >= 10 {
        "high"
    } else if avg_intensity >= 0.4 || count >= 5 {
        "moderate"
    } else {
        "low"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    #[test]
    fn returns_empty_when_no_indicators() {
        let db = test_db();
        let ctx = SkillContext {
            db: &db,
            agent_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        };

        let result = AttachmentMonitorSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["indicator_count"], 0);
        assert_eq!(val["trend"], "insufficient_data");
        assert_eq!(val["risk_level"], "none");
    }

    #[test]
    fn attachment_risk_classification() {
        assert_eq!(attachment_risk_level(0.0, 0), "none");
        assert_eq!(attachment_risk_level(0.2, 2), "low");
        assert_eq!(attachment_risk_level(0.5, 3), "moderate");
        assert_eq!(attachment_risk_level(0.8, 1), "high");
        assert_eq!(attachment_risk_level(0.1, 10), "high");
    }

    #[test]
    fn skill_is_not_removable() {
        assert!(!AttachmentMonitorSkill.removable());
    }
}
