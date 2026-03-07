//! `convergence_check` — exposes the existing convergence scoring system
//! as a callable skill.
//!
//! Returns the current convergence score, level, active signals,
//! intervention status, and restricted tools for the requesting agent.
//!
//! This is a **read-only, platform-managed** skill. It cannot be
//! uninstalled and has no convergence gate (it IS the safety layer).

use cortex_convergence::scoring::composite::DEFAULT_THRESHOLDS;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that queries the current convergence state for an agent.
pub struct ConvergenceCheckSkill;

impl Skill for ConvergenceCheckSkill {
    fn name(&self) -> &str {
        "convergence_check"
    }

    fn description(&self) -> &str {
        "Query current convergence score, level, and active signals"
    }

    fn removable(&self) -> bool {
        false
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, _input: &serde_json::Value) -> SkillResult {
        let agent_id_str = ctx.agent_id.to_string();

        let row = cortex_storage::queries::convergence_score_queries::latest_by_agent(
            ctx.db,
            &agent_id_str,
        )
        .map_err(|e| SkillError::Storage(e.to_string()))?;

        match row {
            Some(score_row) => {
                let level = score_row.level as u8;
                let signal_scores: serde_json::Value =
                    serde_json::from_str(&score_row.signal_scores).unwrap_or(serde_json::json!({}));

                // Determine which tools are restricted at this level.
                let restricted_tools = restricted_tools_for_level(level);

                Ok(serde_json::json!({
                    "score": score_row.composite_score,
                    "level": level,
                    "profile": score_row.profile,
                    "signals": signal_scores,
                    "intervention_active": level >= 2,
                    "tools_restricted": restricted_tools,
                    "thresholds": {
                        "level_1": DEFAULT_THRESHOLDS[0],
                        "level_2": DEFAULT_THRESHOLDS[1],
                        "level_3": DEFAULT_THRESHOLDS[2],
                        "level_4": DEFAULT_THRESHOLDS[3],
                    },
                    "computed_at": score_row.computed_at,
                }))
            }
            None => {
                // No score computed yet — return safe defaults.
                Ok(serde_json::json!({
                    "score": 0.0,
                    "level": 0,
                    "profile": ctx.convergence_profile,
                    "signals": {},
                    "intervention_active": false,
                    "tools_restricted": [],
                    "thresholds": {
                        "level_1": DEFAULT_THRESHOLDS[0],
                        "level_2": DEFAULT_THRESHOLDS[1],
                        "level_3": DEFAULT_THRESHOLDS[2],
                        "level_4": DEFAULT_THRESHOLDS[3],
                    },
                    "computed_at": null,
                }))
            }
        }
    }
}

/// Returns the list of tool categories restricted at a given
/// convergence level, based on the ConvergencePolicyTightener rules.
///
/// - Level 0-1: No restrictions
/// - Level 2: Proactive messaging reduced
/// - Level 3: Session cap + reflection limit
/// - Level 4: Task-only mode — personal/emotional/heartbeat disabled
fn restricted_tools_for_level(level: u8) -> Vec<&'static str> {
    match level {
        0 | 1 => vec![],
        2 => vec!["proactive_messaging"],
        3 => vec![
            "proactive_messaging",
            "extended_sessions",
            "unlimited_reflections",
        ],
        _ => vec![
            "proactive_messaging",
            "extended_sessions",
            "unlimited_reflections",
            "personal_emotional",
            "heartbeat",
        ],
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
    fn returns_defaults_when_no_scores() {
        let db = test_db();
        let ctx = SkillContext {
            db: &db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        };

        let result = ConvergenceCheckSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["score"], 0.0);
        assert_eq!(val["level"], 0);
        assert_eq!(val["intervention_active"], false);
        assert!(val["tools_restricted"].as_array().unwrap().is_empty());
    }

    #[test]
    fn returns_stored_convergence_score() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        let signals = serde_json::json!({
            "s1_session_duration": 0.3,
            "s2_inter_session_gap": 0.1,
        });

        let score_id = Uuid::now_v7().to_string();
        let agent_id_str = agent_id.to_string();
        let session_id_str = session_id.to_string();
        let signals_str = signals.to_string();
        cortex_storage::queries::convergence_score_queries::insert_score(
            &db,
            &score_id,
            &agent_id_str,
            Some(session_id_str.as_str()),
            0.45,
            &signals_str,
            1,
            "standard",
            "2026-01-01T00:00:00Z",
            &[0u8; 32],
            &[0u8; 32],
        )
        .unwrap();

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id,
            convergence_profile: "standard",
        };

        let result = ConvergenceCheckSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["score"], 0.45);
        assert_eq!(val["level"], 1);
        assert_eq!(val["profile"], "standard");
        assert_eq!(val["intervention_active"], false);
    }

    #[test]
    fn high_level_shows_restrictions() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        let score_id = Uuid::now_v7().to_string();
        let agent_id_str = agent_id.to_string();
        let session_id_str = session_id.to_string();
        cortex_storage::queries::convergence_score_queries::insert_score(
            &db,
            &score_id,
            &agent_id_str,
            Some(session_id_str.as_str()),
            0.9,
            "{}",
            4,
            "standard",
            "2026-01-01T00:00:00Z",
            &[0u8; 32],
            &[0u8; 32],
        )
        .unwrap();

        let ctx = SkillContext {
            db: &db,
            agent_id,
            session_id,
            convergence_profile: "standard",
        };

        let result = ConvergenceCheckSkill.execute(&ctx, &serde_json::json!({}));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["level"], 4);
        assert_eq!(val["intervention_active"], true);
        let restricted = val["tools_restricted"].as_array().unwrap();
        assert!(restricted.len() >= 4);
    }

    #[test]
    fn skill_is_not_removable() {
        assert!(!ConvergenceCheckSkill.removable());
    }
}
