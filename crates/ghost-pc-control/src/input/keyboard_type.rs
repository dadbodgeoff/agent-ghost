//! `keyboard_type` — type a text string via keyboard simulation.
//!
//! Medium risk. Convergence max: Level 2. Budget: 500 per session.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Description              |
//! |--------------|--------|----------|--------------------------|
//! | `text`       | string | yes      | Text to type             |
//! | `target_app` | string | no       | Target application name  |
//!
//! ## Output
//!
//! ```json
//! { "typed_length": 11, "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::InputBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct KeyboardTypeSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl KeyboardTypeSkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
        backend: Arc<Mutex<dyn InputBackend>>,
    ) -> Self {
        Self { validator, circuit_breaker, backend }
    }
}

impl Skill for KeyboardTypeSkill {
    fn name(&self) -> &str { "keyboard_type" }

    fn description(&self) -> &str {
        "Type a text string via keyboard simulation"
    }

    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let text = input.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'text' (string)".into())
        })?;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        if text.is_empty() {
            return Err(SkillError::InvalidInput("text must not be empty".into()));
        }

        // Safety: validate app if specified.
        if let Some(app) = target_app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
                audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "keyboard_type", input, &reason);
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Safety: circuit breaker.
        { self.circuit_breaker.lock().unwrap().check("keyboard_type")?; }

        // Execute: type the text.
        {
            let mut backend = self.backend.lock().unwrap();
            backend.key_sequence(text);
        }

        // Record success.
        { self.circuit_breaker.lock().unwrap().record_success(); }

        let result = serde_json::json!({
            "typed_length": text.len(),
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "keyboard_type", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let text = input.get("text").and_then(|v| v.as_str())?;
        let preview_text = if text.len() > 50 {
            format!("{}...", &text[..50])
        } else {
            text.to_string()
        };
        Some(format!("Type: \"{preview_text}\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::input_backend::{MockInputBackend, RecordedAction};
    use crate::safety::circuit_breaker::PcControlCircuitBreaker;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext { db, agent_id: Uuid::nil(), session_id: Uuid::nil(), convergence_profile: "standard" }
    }

    fn test_skill() -> (KeyboardTypeSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(vec!["Firefox".into()], None, vec![]));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, std::time::Duration::from_secs(30))));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (KeyboardTypeSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn types_text_successfully() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"text": "hello world"})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["typed_length"], 11);

        let actions = mock.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], RecordedAction::KeySequence("hello world".into()));
    }

    #[test]
    fn rejects_missing_text() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_empty_text() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"text": ""}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_disallowed_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"text": "hi", "target_app": "Terminal"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn preview_truncates_long_text() {
        let (skill, _) = test_skill();
        let long_text = "a".repeat(100);
        let preview = skill.preview(&serde_json::json!({"text": long_text}));
        assert!(preview.unwrap().contains("..."));
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "keyboard_type");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
