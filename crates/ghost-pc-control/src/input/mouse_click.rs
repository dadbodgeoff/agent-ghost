//! `mouse_click` — click the mouse at the current or specified position.
//!
//! Medium risk. Convergence max: Level 2. Budget: 200 per session.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Default | Description                      |
//! |--------------|--------|----------|---------|----------------------------------|
//! | `x`          | i32    | no       | current | Target X (moves mouse if given)  |
//! | `y`          | i32    | no       | current | Target Y (moves mouse if given)  |
//! | `button`     | string | no       | "left"  | "left", "right", or "middle"     |
//! | `click_type` | string | no       | "single"| "single" or "double"             |
//! | `target_app` | string | no       | —       | Target application name          |

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::{InputBackend, MouseButton};
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct MouseClickSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl MouseClickSkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
        backend: Arc<Mutex<dyn InputBackend>>,
    ) -> Self {
        Self { validator, circuit_breaker, backend }
    }
}

impl Skill for MouseClickSkill {
    fn name(&self) -> &str { "mouse_click" }

    fn description(&self) -> &str {
        "Click the mouse at a position with configurable button and click type"
    }

    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        let button = match input.get("button").and_then(|v| v.as_str()).unwrap_or("left") {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            other => {
                return Err(SkillError::InvalidInput(format!(
                    "invalid button '{other}', must be: left, right, middle"
                )));
            }
        };

        let click_type = input.get("click_type").and_then(|v| v.as_str()).unwrap_or("single");
        if !matches!(click_type, "single" | "double") {
            return Err(SkillError::InvalidInput(format!(
                "invalid click_type '{click_type}', must be: single, double"
            )));
        }

        // If x,y are provided, move first and validate.
        let has_coords = input.get("x").is_some() && input.get("y").is_some();
        if has_coords {
            let x = input["x"].as_i64().ok_or_else(|| {
                SkillError::InvalidInput("'x' must be an integer".into())
            })? as i32;
            let y = input["y"].as_i64().ok_or_else(|| {
                SkillError::InvalidInput("'y' must be an integer".into())
            })? as i32;

            if let ValidationResult::Denied(reason) = self.validator.validate_click(x, y, target_app) {
                audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "mouse_click", input, &reason);
                return Err(SkillError::PcControlBlocked(reason));
            }

            // Circuit breaker check.
            { self.circuit_breaker.lock().unwrap().check("mouse_click")?; }

            // Move + click.
            {
                let mut backend = self.backend.lock().unwrap();
                backend.mouse_move_to(x, y);
                match click_type {
                    "double" => backend.mouse_double_click(button),
                    _ => backend.mouse_click(button),
                }
            }
        } else {
            // Click at current position. Validate app if provided.
            if let Some(app) = target_app {
                if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
                    audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "mouse_click", input, &reason);
                    return Err(SkillError::PcControlBlocked(reason));
                }
            }

            { self.circuit_breaker.lock().unwrap().check("mouse_click")?; }

            {
                let mut backend = self.backend.lock().unwrap();
                match click_type {
                    "double" => backend.mouse_double_click(button),
                    _ => backend.mouse_click(button),
                }
            }
        }

        // Record success.
        { self.circuit_breaker.lock().unwrap().record_success(); }

        let result = serde_json::json!({
            "status": "ok",
            "button": format!("{button:?}").to_lowercase(),
            "click_type": click_type,
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "mouse_click", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let button = input.get("button").and_then(|v| v.as_str()).unwrap_or("left");
        let click_type = input.get("click_type").and_then(|v| v.as_str()).unwrap_or("single");
        let pos = match (input.get("x").and_then(|v| v.as_i64()), input.get("y").and_then(|v| v.as_i64())) {
            (Some(x), Some(y)) => format!(" at ({x}, {y})"),
            _ => " at current position".into(),
        };
        Some(format!("{click_type} {button} click{pos}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::input_backend::{MockInputBackend, RecordedAction};
    use crate::safety::circuit_breaker::PcControlCircuitBreaker;
    use crate::safety::input_validator::ScreenRegion;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext { db, agent_id: Uuid::nil(), session_id: Uuid::nil(), convergence_profile: "standard" }
    }

    fn test_skill() -> (MouseClickSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into()],
            Some(ScreenRegion { x: 0, y: 0, width: 1920, height: 1080 }),
            vec![],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, std::time::Duration::from_secs(30))));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (MouseClickSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn left_click_at_position() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"x": 100, "y": 200})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["button"], "left");

        let actions = mock.actions();
        assert_eq!(actions.len(), 2); // move + click
        assert_eq!(actions[0], RecordedAction::MouseMoveTo(100, 200));
        assert_eq!(actions[1], RecordedAction::MouseClick(MouseButton::Left));
    }

    #[test]
    fn right_double_click() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({
            "x": 50, "y": 50, "button": "right", "click_type": "double"
        })).unwrap();
        assert_eq!(result["click_type"], "double");

        let actions = mock.actions();
        assert_eq!(actions[1], RecordedAction::MouseDoubleClick(MouseButton::Right));
    }

    #[test]
    fn click_without_coords_at_current_position() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");

        let actions = mock.actions();
        assert_eq!(actions.len(), 1); // just click, no move
        assert_eq!(actions[0], RecordedAction::MouseClick(MouseButton::Left));
    }

    #[test]
    fn invalid_button_rejected() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"button": "turbo"}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_outside_safe_zone() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"x": 9999, "y": 9999}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "mouse_click");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
