//! `keyboard_hotkey` — send a hotkey combination (e.g. Ctrl+C, Cmd+V).
//!
//! Medium risk. Convergence max: Level 2. Budget: 50 per session.
//!
//! ## Input
//!
//! | Field        | Type     | Required | Description                          |
//! |--------------|----------|----------|--------------------------------------|
//! | `keys`       | string   | yes      | Hotkey combo, e.g. "Ctrl+C"          |
//! | `target_app` | string   | no       | Target application name              |
//!
//! ## Output
//!
//! ```json
//! { "keys": "Ctrl+C", "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::{InputBackend, Key};
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct KeyboardHotkeySkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl KeyboardHotkeySkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
        backend: Arc<Mutex<dyn InputBackend>>,
    ) -> Self {
        Self { validator, circuit_breaker, backend }
    }
}

impl Skill for KeyboardHotkeySkill {
    fn name(&self) -> &str { "keyboard_hotkey" }

    fn description(&self) -> &str {
        "Send a hotkey combination (e.g. Ctrl+C, Cmd+V)"
    }

    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let keys = input.get("keys").and_then(|v| v.as_str()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'keys' (string)".into())
        })?;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        if keys.is_empty() {
            return Err(SkillError::InvalidInput("keys must not be empty".into()));
        }

        // Safety: validate hotkey against blocklist.
        if let ValidationResult::Denied(reason) = self.validator.validate_hotkey(keys) {
            audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "keyboard_hotkey", input, &reason);
            return Err(SkillError::PcControlBlocked(reason));
        }

        // Safety: validate app if specified.
        if let Some(app) = target_app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
                audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "keyboard_hotkey", input, &reason);
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Safety: circuit breaker.
        { self.circuit_breaker.lock().unwrap().check("keyboard_hotkey")?; }

        // Parse and execute the hotkey.
        let parsed_keys = parse_hotkey(keys)?;
        {
            let mut backend = self.backend.lock().unwrap();
            // Press all keys in order, then release in reverse order.
            for key in &parsed_keys {
                backend.key_down(key);
            }
            for key in parsed_keys.iter().rev() {
                backend.key_up(key);
            }
        }

        // Record success.
        { self.circuit_breaker.lock().unwrap().record_success(); }

        let result = serde_json::json!({
            "keys": keys,
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "keyboard_hotkey", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let keys = input.get("keys").and_then(|v| v.as_str())?;
        Some(format!("Hotkey: {keys}"))
    }
}

/// Parse a hotkey string like "Ctrl+Shift+C" into a sequence of `Key` values.
fn parse_hotkey(keys: &str) -> Result<Vec<Key>, SkillError> {
    let parts: Vec<&str> = keys.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() || parts.iter().any(|p| p.is_empty()) {
        return Err(SkillError::InvalidInput(format!(
            "invalid hotkey format: '{keys}'"
        )));
    }

    let mut result = Vec::with_capacity(parts.len());
    for part in &parts {
        let key = match part.to_lowercase().as_str() {
            "ctrl" | "control" => Key::Control,
            "alt" | "option" => Key::Alt,
            "shift" => Key::Shift,
            "cmd" | "meta" | "super" | "win" => Key::Meta,
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
            s if s.len() == 1 => Key::Char(s.chars().next().unwrap()),
            s if s.starts_with('f') && s[1..].parse::<u8>().is_ok() => {
                Key::F(s[1..].parse().unwrap())
            }
            _ => {
                return Err(SkillError::InvalidInput(format!(
                    "unrecognized key '{part}' in hotkey '{keys}'"
                )));
            }
        };
        result.push(key);
    }

    Ok(result)
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

    fn test_skill() -> (KeyboardHotkeySkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into()],
            None,
            vec!["Ctrl+Alt+Delete".into(), "Cmd+Q".into(), "Alt+F4".into()],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, std::time::Duration::from_secs(30))));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (KeyboardHotkeySkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn sends_ctrl_c_successfully() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": "Ctrl+C"})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["keys"], "Ctrl+C");

        let actions = mock.actions();
        assert_eq!(actions.len(), 4); // down, down, up, up (reverse)
        assert_eq!(actions[0], RecordedAction::KeyDown(Key::Control));
        assert_eq!(actions[1], RecordedAction::KeyDown(Key::Char('c')));
        assert_eq!(actions[2], RecordedAction::KeyUp(Key::Char('c')));
        assert_eq!(actions[3], RecordedAction::KeyUp(Key::Control));
    }

    #[test]
    fn blocks_dangerous_hotkey() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": "Ctrl+Alt+Delete"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn blocks_cmd_q() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": "Cmd+Q"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
    }

    #[test]
    fn rejects_missing_keys() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_empty_keys() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": ""}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_unrecognized_key() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": "Ctrl+FooBar"}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn parse_hotkey_handles_modifiers() {
        let keys = parse_hotkey("Ctrl+Shift+A").unwrap();
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0], Key::Control);
        assert_eq!(keys[1], Key::Shift);
        assert_eq!(keys[2], Key::Char('a'));
    }

    #[test]
    fn parse_hotkey_handles_function_keys() {
        let keys = parse_hotkey("Alt+F4").unwrap();
        assert_eq!(keys.len(), 2);
        assert_eq!(keys[0], Key::Alt);
        assert_eq!(keys[1], Key::F(4));
    }

    #[test]
    fn blocks_disallowed_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"keys": "Ctrl+C", "target_app": "Terminal"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "keyboard_hotkey");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
