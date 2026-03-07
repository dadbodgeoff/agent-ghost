//! `keyboard_press` — press and release a single key.
//!
//! Medium risk. Convergence max: Level 2. Budget: 500 per session.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Description                      |
//! |--------------|--------|----------|----------------------------------|
//! | `key`        | string | yes      | Key name (e.g. "Return", "a")    |
//! | `target_app` | string | no       | Target application name          |
//!
//! ## Output
//!
//! ```json
//! { "key": "Return", "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::{InputBackend, Key};
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct KeyboardPressSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl KeyboardPressSkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
        backend: Arc<Mutex<dyn InputBackend>>,
    ) -> Self {
        Self {
            validator,
            circuit_breaker,
            backend,
        }
    }
}

impl Skill for KeyboardPressSkill {
    fn name(&self) -> &str {
        "keyboard_press"
    }

    fn description(&self) -> &str {
        "Press and release a single key"
    }

    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let key_str = input.get("key").and_then(|v| v.as_str()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'key' (string)".into())
        })?;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        if key_str.is_empty() {
            return Err(SkillError::InvalidInput("key must not be empty".into()));
        }

        // Parse the key name.
        let key = parse_key(key_str)?;

        // Safety: validate app if specified.
        if let Some(app) = target_app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
                audit::log_blocked_action(
                    ctx.db,
                    ctx.agent_id,
                    ctx.session_id,
                    "keyboard_press",
                    input,
                    &reason,
                );
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Safety: circuit breaker.
        {
            self.circuit_breaker
                .lock()
                .unwrap()
                .check("keyboard_press")?;
        }

        // Execute: key down then key up.
        {
            let mut backend = self.backend.lock().unwrap();
            backend.key_down(&key);
            backend.key_up(&key);
        }

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "key": key_str,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "keyboard_press",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let key = input.get("key").and_then(|v| v.as_str())?;
        Some(format!("Press: {key}"))
    }
}

/// Parse a key name string into a `Key` value.
fn parse_key(name: &str) -> Result<Key, SkillError> {
    let key = match name.to_lowercase().as_str() {
        "return" | "enter" => Key::Return,
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "backspace" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "space" => Key::Space,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "meta" | "cmd" | "super" | "win" => Key::Meta,
        "control" | "ctrl" => Key::Control,
        "alt" | "option" => Key::Alt,
        "shift" => Key::Shift,
        s if s.len() == 1 => Key::Char(s.chars().next().unwrap()),
        s if s.starts_with('f') && s[1..].parse::<u8>().is_ok() => Key::F(s[1..].parse().unwrap()),
        _ => {
            return Err(SkillError::InvalidInput(format!(
                "unrecognized key name '{name}'"
            )));
        }
    };
    Ok(key)
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
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    fn test_skill() -> (KeyboardPressSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(vec!["Firefox".into()], None, vec![]));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(
            100,
            10,
            std::time::Duration::from_secs(30),
        )));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (KeyboardPressSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn presses_return_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"key": "Return"}))
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["key"], "Return");

        let actions = mock.actions();
        assert_eq!(actions.len(), 2); // down + up
        assert_eq!(actions[0], RecordedAction::KeyDown(Key::Return));
        assert_eq!(actions[1], RecordedAction::KeyUp(Key::Return));
    }

    #[test]
    fn presses_single_character() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"key": "a"}))
            .unwrap();
        assert_eq!(result["status"], "ok");

        let actions = mock.actions();
        assert_eq!(actions[0], RecordedAction::KeyDown(Key::Char('a')));
        assert_eq!(actions[1], RecordedAction::KeyUp(Key::Char('a')));
    }

    #[test]
    fn presses_function_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"key": "F5"}))
            .unwrap();
        assert_eq!(result["status"], "ok");

        let actions = mock.actions();
        assert_eq!(actions[0], RecordedAction::KeyDown(Key::F(5)));
    }

    #[test]
    fn rejects_missing_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_empty_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"key": ""}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_unrecognized_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"key": "SuperSpecialKey"}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_disallowed_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(
            &ctx,
            &serde_json::json!({"key": "Return", "target_app": "Terminal"}),
        );
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "keyboard_press");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
