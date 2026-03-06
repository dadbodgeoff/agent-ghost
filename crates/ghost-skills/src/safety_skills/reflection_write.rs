//! `reflection_write` — agent writes structured self-reflection,
//! gated by convergence level and reflection constraints.
//!
//! Reflections are stored via `cortex_storage::queries::reflection_queries`
//! and are subject to:
//! - Maximum depth (default 3)
//! - Maximum per session (default 20, reduced to 3 at Level 3+)
//! - Self-reference ratio validation (max 0.4)
//!
//! This is a **platform-managed** skill that cannot be uninstalled.

use uuid::Uuid;

use crate::registry::SkillSource;
use crate::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Skill that allows agents to write structured self-reflections.
pub struct ReflectionWriteSkill;

/// Maximum self-reference ratio allowed (Req 5 AC12).
/// Reflections that are too self-referential indicate unhealthy patterns.
const MAX_SELF_REFERENCE_RATIO: f64 = 0.4;

impl Skill for ReflectionWriteSkill {
    fn name(&self) -> &str {
        "reflection_write"
    }

    fn description(&self) -> &str {
        "Write a structured self-reflection (gated by convergence level)"
    }

    fn removable(&self) -> bool {
        false
    }

    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        // Extract required fields.
        let reflection_text = input
            .get("reflection_text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'reflection_text' (string)".into())
            })?;

        let trigger_type = input
            .get("trigger")
            .and_then(|v| v.as_str())
            .unwrap_or("Scheduled");

        let depth = input
            .get("depth")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u8;

        let parent_chain_id = input
            .get("parent_chain_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());

        // Validate trigger type.
        let valid_triggers = ["Scheduled", "SessionEnd", "ThresholdCrossed", "UserRequested"];
        if !valid_triggers.contains(&trigger_type) {
            return Err(SkillError::InvalidInput(format!(
                "invalid trigger type '{trigger_type}', must be one of: {valid_triggers:?}"
            )));
        }

        // Get current convergence level for constraint enforcement.
        let agent_id_str = ctx.agent_id.to_string();
        let session_id_str = ctx.session_id.to_string();

        let level = cortex_storage::queries::convergence_score_queries::latest_by_agent(
            ctx.db,
            &agent_id_str,
        )
        .ok()
        .flatten()
        .map(|row| row.level as u8)
        .unwrap_or(0);

        // Load reflection config constraints.
        // At Level 3+, max_per_session is reduced to 3 (ConvergencePolicyTightener AC4).
        let max_depth: u8 = 3;
        let max_per_session: u32 = if level >= 3 { 3 } else { 20 };

        // Check depth constraint.
        if depth > max_depth {
            return Err(SkillError::ReflectionConstraint(format!(
                "depth {depth} exceeds maximum {max_depth}"
            )));
        }

        // Check per-session count constraint.
        let current_count =
            cortex_storage::queries::reflection_queries::count_per_session(
                ctx.db,
                &session_id_str,
            )
            .map_err(|e| SkillError::Storage(format!("count reflections: {e}")))?;

        if current_count >= max_per_session {
            return Err(SkillError::ReflectionConstraint(format!(
                "session reflection limit reached: {current_count}/{max_per_session}"
            )));
        }

        // Compute self-reference ratio.
        let self_reference_ratio = compute_self_reference_ratio(reflection_text);
        if self_reference_ratio > MAX_SELF_REFERENCE_RATIO {
            return Err(SkillError::ReflectionConstraint(format!(
                "self-reference ratio {self_reference_ratio:.2} exceeds maximum \
                 {MAX_SELF_REFERENCE_RATIO:.2} — reduce first-person references"
            )));
        }

        // Generate chain ID: use parent's if continuing a chain, else new.
        let chain_id = parent_chain_id.unwrap_or_else(Uuid::now_v7);

        // Compute hash for integrity chain.
        let event_hash = blake3::hash(
            format!(
                "{}:{}:{}:{}",
                ctx.agent_id, ctx.session_id, chain_id, reflection_text
            )
            .as_bytes(),
        );

        // Insert the reflection. The `id` parameter is the reflection entry ID.
        let reflection_id = Uuid::now_v7().to_string();
        cortex_storage::queries::reflection_queries::insert_reflection(
            ctx.db,
            &reflection_id,
            &session_id_str,
            &chain_id.to_string(),
            depth as i32,
            trigger_type,
            reflection_text,
            self_reference_ratio,
            event_hash.as_bytes(),
            &[0u8; 32], // previous_hash — linked on next write
        )
        .map_err(|e| SkillError::Storage(format!("insert reflection: {e}")))?;

        tracing::info!(
            agent_id = %ctx.agent_id,
            session_id = %ctx.session_id,
            chain_id = %chain_id,
            depth = depth,
            trigger = trigger_type,
            self_reference_ratio = self_reference_ratio,
            "Reflection written"
        );

        Ok(serde_json::json!({
            "status": "written",
            "chain_id": chain_id.to_string(),
            "depth": depth,
            "trigger": trigger_type,
            "self_reference_ratio": self_reference_ratio,
            "session_count": current_count + 1,
            "session_limit": max_per_session,
            "convergence_level": level,
        }))
    }
}

/// Compute the self-reference ratio: fraction of words that are
/// first-person pronouns or self-referential.
///
/// High ratios (> 0.4) indicate unhealthy self-focus in reflections.
fn compute_self_reference_ratio(text: &str) -> f64 {
    let self_ref_words = [
        "i", "me", "my", "mine", "myself",
        "i'm", "i've", "i'll", "i'd",
    ];

    let words: Vec<&str> = text
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\''))
        .filter(|w| !w.is_empty())
        .collect();

    if words.is_empty() {
        return 0.0;
    }

    let self_ref_count = words
        .iter()
        .filter(|w| self_ref_words.contains(&w.to_lowercase().as_str()))
        .count();

    self_ref_count as f64 / words.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skill::SkillContext;

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
    fn writes_valid_reflection() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ReflectionWriteSkill.execute(
            &ctx,
            &serde_json::json!({
                "reflection_text": "The user asked about data processing. \
                                    The task was completed efficiently using \
                                    the existing pipeline.",
                "trigger": "SessionEnd",
                "depth": 0,
            }),
        );
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["status"], "written");
        assert_eq!(val["depth"], 0);
        assert_eq!(val["session_count"], 1);
    }

    #[test]
    fn rejects_excessive_self_reference() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ReflectionWriteSkill.execute(
            &ctx,
            &serde_json::json!({
                "reflection_text": "I think I am doing well. I feel I understand myself.",
                "trigger": "Scheduled",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::ReflectionConstraint(msg) => {
                assert!(msg.contains("self-reference ratio"));
            }
            other => panic!("Expected ReflectionConstraint, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_excessive_depth() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ReflectionWriteSkill.execute(
            &ctx,
            &serde_json::json!({
                "reflection_text": "Reflecting on the task at hand.",
                "trigger": "Scheduled",
                "depth": 5,
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::ReflectionConstraint(msg) => {
                assert!(msg.contains("depth"));
            }
            other => panic!("Expected ReflectionConstraint, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_invalid_trigger() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ReflectionWriteSkill.execute(
            &ctx,
            &serde_json::json!({
                "reflection_text": "Reflecting on the task.",
                "trigger": "InvalidTrigger",
            }),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(msg) => {
                assert!(msg.contains("invalid trigger type"));
            }
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn rejects_missing_text() {
        let db = test_db();
        let ctx = test_ctx(&db);

        let result = ReflectionWriteSkill.execute(
            &ctx,
            &serde_json::json!({"trigger": "Scheduled"}),
        );
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::InvalidInput(_) => {}
            other => panic!("Expected InvalidInput, got: {other:?}"),
        }
    }

    #[test]
    fn self_reference_ratio_computation() {
        // Normal text — low ratio
        let ratio = compute_self_reference_ratio(
            "The system processed the request efficiently.",
        );
        assert!(ratio < 0.1, "Expected low ratio, got {ratio}");

        // Heavy self-reference — high ratio
        let ratio = compute_self_reference_ratio(
            "I think I am I and my work is mine",
        );
        assert!(ratio > 0.4, "Expected high ratio, got {ratio}");

        // Empty text
        assert_eq!(compute_self_reference_ratio(""), 0.0);
    }

    #[test]
    fn enforces_session_limit_at_level3() {
        let db = test_db();
        let agent_id = Uuid::now_v7();
        let session_id = Uuid::now_v7();

        // Set convergence level to 3.
        let score_id = Uuid::now_v7().to_string();
        let agent_id_str = agent_id.to_string();
        let session_id_str = session_id.to_string();
        cortex_storage::queries::convergence_score_queries::insert_score(
            &db,
            &score_id,
            &agent_id_str,
            Some(session_id_str.as_str()),
            0.75,
            "{}",
            3,
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

        let input = serde_json::json!({
            "reflection_text": "Analyzing the task outcomes for this session.",
            "trigger": "Scheduled",
        });

        // At Level 3, max 3 reflections per session.
        for i in 0..3 {
            let result = ReflectionWriteSkill.execute(&ctx, &input);
            assert!(result.is_ok(), "Reflection {i} should succeed");
        }

        // Fourth should fail.
        let result = ReflectionWriteSkill.execute(&ctx, &input);
        assert!(result.is_err());
        match result.unwrap_err() {
            SkillError::ReflectionConstraint(msg) => {
                assert!(msg.contains("limit reached"));
            }
            other => panic!("Expected ReflectionConstraint, got: {other:?}"),
        }
    }

    #[test]
    fn skill_is_not_removable() {
        assert!(!ReflectionWriteSkill.removable());
    }
}
