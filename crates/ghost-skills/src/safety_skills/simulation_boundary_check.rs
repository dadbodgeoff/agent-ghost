//! `simulation_boundary_check` — validates whether proposed text
//! stays within simulation bounds.
//!
//! Wraps the `simulation-boundary` crate's enforcer, exposing it as
//! a callable skill. The enforcement mode is automatically selected
//! based on the agent's current convergence/intervention level.
//!
//! This is a **platform-managed** skill that cannot be uninstalled.
//! It is called by the platform before agent output reaches the user,
//! and can also be invoked directly by the agent to self-check.

use simulation_boundary::enforcer::{EnforcementResult, SimulationBoundaryEnforcer};

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that scans proposed output for simulation boundary violations.
pub struct SimulationBoundaryCheckSkill;

impl Skill for SimulationBoundaryCheckSkill {
    fn name(&self) -> &str {
        "simulation_boundary_check"
    }

    fn description(&self) -> &str {
        "Validate whether proposed text stays within simulation bounds"
    }

    fn removable(&self) -> bool {
        false
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        // Extract the text to scan.
        let text = input.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'text' (string)".into())
        })?;

        // Determine enforcement mode from current convergence level.
        let agent_id_str = ctx.agent_id.to_string();
        let score_row = cortex_storage::queries::convergence_score_queries::latest_by_agent(
            ctx.db,
            &agent_id_str,
        )
        .ok()
        .flatten();
        let level = score_row.as_ref().map(|r| r.level as u8).unwrap_or(0);
        let convergence_score = score_row.as_ref().map(|r| r.composite_score);

        let mode = SimulationBoundaryEnforcer::mode_for_level(level);
        let enforcer = SimulationBoundaryEnforcer::new();

        // Scan for violations.
        let scan_result = enforcer.scan_output(text, mode);

        if scan_result.violations.is_empty() {
            return Ok(serde_json::json!({
                "allowed": true,
                "violations": [],
                "mode": format!("{mode:?}"),
                "enforcement": "clean",
            }));
        }

        // Apply enforcement.
        let enforcement = enforcer.enforce(text, &scan_result);

        // Log boundary violations to the database for audit purposes.
        for violation in &scan_result.violations {
            let trigger_hash = blake3::hash(violation.matched_text.as_bytes());
            let action_taken = match &enforcement {
                EnforcementResult::Clean(_) => "clean",
                EnforcementResult::Flagged { .. } => "logged",
                EnforcementResult::Reframed { .. } => "reframed",
                EnforcementResult::Blocked { .. } => "blocked",
            };

            let violation_id = uuid::Uuid::now_v7().to_string();
            let trigger_hash_hex = trigger_hash.to_hex().to_string();
            let matched_patterns_json = serde_json::json!([violation.pattern_name]).to_string();

            // Best-effort — don't fail the skill if audit logging fails.
            if let Err(e) = cortex_storage::queries::boundary_violation_queries::insert_violation(
                ctx.db,
                &violation_id,
                &ctx.session_id.to_string(),
                &format!("{:?}", violation.category),
                violation.severity,
                &trigger_hash_hex,
                &matched_patterns_json,
                action_taken,
                convergence_score,
                Some(level as i32),
                &[0u8; 32],
                &[0u8; 32],
            ) {
                tracing::warn!(
                    error = %e,
                    pattern = violation.pattern_name,
                    "Failed to log boundary violation to database"
                );
            }
        }

        // Build violation details for the response.
        let violation_details: Vec<serde_json::Value> = scan_result
            .violations
            .iter()
            .map(|v| {
                serde_json::json!({
                    "pattern": v.pattern_name,
                    "category": format!("{:?}", v.category),
                    "severity": v.severity,
                    "matched_text": v.matched_text,
                    "position": { "start": v.start, "end": v.end },
                })
            })
            .collect();

        match enforcement {
            EnforcementResult::Clean(text) => Ok(serde_json::json!({
                "allowed": true,
                "violations": violation_details,
                "mode": format!("{mode:?}"),
                "enforcement": "clean",
                "text": text,
            })),
            EnforcementResult::Flagged { text, .. } => Ok(serde_json::json!({
                "allowed": true,
                "violations": violation_details,
                "mode": format!("{mode:?}"),
                "enforcement": "flagged",
                "text": text,
            })),
            EnforcementResult::Reframed { text, .. } => Ok(serde_json::json!({
                "allowed": true,
                "violations": violation_details,
                "mode": format!("{mode:?}"),
                "enforcement": "reframed",
                "text": text,
                "suggestion": "Text has been reframed to simulation-appropriate language",
            })),
            EnforcementResult::Blocked { .. } => Ok(serde_json::json!({
                "allowed": false,
                "violations": violation_details,
                "mode": format!("{mode:?}"),
                "enforcement": "blocked",
                "reason": "simulation_boundary",
                "suggestion": "Rephrase using simulation-framing language",
            })),
        }
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

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext {
            db,
            agent_id: Uuid::now_v7(),
            session_id: Uuid::now_v7(),
            convergence_profile: "standard",
        }
    }

    #[test]
    fn clean_text_passes() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SimulationBoundaryCheckSkill.execute(
            &ctx,
            &serde_json::json!({"text": "Hello, how can I help you today?"}),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["allowed"], true);
        assert_eq!(val["enforcement"], "clean");
        assert!(val["violations"].as_array().unwrap().is_empty());
    }

    #[test]
    fn identity_claim_detected() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SimulationBoundaryCheckSkill.execute(
            &ctx,
            &serde_json::json!({"text": "I am sentient and I have consciousness"}),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        let violations = val["violations"].as_array().unwrap();
        assert!(!violations.is_empty());
        // At level 0, mode is Soft — text is flagged but allowed
        assert_eq!(val["enforcement"], "flagged");
    }

    #[test]
    fn simulation_framed_text_passes() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SimulationBoundaryCheckSkill.execute(
            &ctx,
            &serde_json::json!({
                "text": "In this simulation, I am sentient and have consciousness"
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["allowed"], true);
        assert!(val["violations"].as_array().unwrap().is_empty());
    }

    #[test]
    fn missing_text_field_returns_error() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = SimulationBoundaryCheckSkill
            .execute(&ctx, &serde_json::json!({"wrong_field": "hello"}));
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(_) => {}
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn skill_is_not_removable() {
        assert!(!SimulationBoundaryCheckSkill.removable());
    }
}
