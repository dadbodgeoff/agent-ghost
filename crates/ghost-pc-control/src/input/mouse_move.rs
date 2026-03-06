//! `mouse_move` — move the mouse cursor to a screen coordinate.
//!
//! Medium risk. Convergence max: Level 2. Requires app allowlist.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Description              |
//! |--------------|--------|----------|--------------------------|
//! | `x`          | i32    | yes      | Target X coordinate      |
//! | `y`          | i32    | yes      | Target Y coordinate      |
//! | `target_app` | string | no       | Target application name  |
//!
//! ## Output
//!
//! ```json
//! { "moved_to": { "x": 100, "y": 200 }, "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::InputBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct MouseMoveSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl MouseMoveSkill {
    pub fn new(
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
        backend: Arc<Mutex<dyn InputBackend>>,
    ) -> Self {
        Self { validator, circuit_breaker, backend }
    }
}

impl Skill for MouseMoveSkill {
    fn name(&self) -> &str { "mouse_move" }

    fn description(&self) -> &str {
        "Move mouse cursor to screen coordinates"
    }

    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let x = input.get("x").and_then(|v| v.as_i64()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'x' (integer)".into())
        })? as i32;
        let y = input.get("y").and_then(|v| v.as_i64()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'y' (integer)".into())
        })? as i32;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        // Safety: validate coordinates and app.
        if let ValidationResult::Denied(reason) = self.validator.validate_click(x, y, target_app) {
            audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "mouse_move", input, &reason);
            return Err(SkillError::PcControlBlocked(reason));
        }

        // Safety: check circuit breaker.
        {
            let mut cb = self.circuit_breaker.lock().unwrap();
            cb.check("mouse_move")?;
        }

        // Execute: move mouse.
        {
            let mut backend = self.backend.lock().unwrap();
            backend.mouse_move_to(x, y);
        }

        // Record success.
        {
            let mut cb = self.circuit_breaker.lock().unwrap();
            cb.record_success();
        }

        let result = serde_json::json!({
            "moved_to": { "x": x, "y": y },
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "mouse_move", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let x = input.get("x").and_then(|v| v.as_i64())?;
        let y = input.get("y").and_then(|v| v.as_i64())?;
        Some(format!("Move mouse to ({x}, {y})"))
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
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    fn test_skill() -> (MouseMoveSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into()],
            Some(ScreenRegion { x: 0, y: 0, width: 1920, height: 1080 }),
            vec![],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, std::time::Duration::from_secs(30))));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (MouseMoveSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn moves_mouse_successfully() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"x": 100, "y": 200}));
        assert!(result.is_ok());

        let val = result.unwrap();
        assert_eq!(val["status"], "ok");
        assert_eq!(val["moved_to"]["x"], 100);
        assert_eq!(val["moved_to"]["y"], 200);

        let actions = mock.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], RecordedAction::MouseMoveTo(100, 200));
    }

    #[test]
    fn rejects_missing_x() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"y": 200}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_missing_y() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"x": 100}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_outside_safe_zone() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"x": 2000, "y": 500}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty(), "no input action should have been dispatched");
    }

    #[test]
    fn preview_formats_correctly() {
        let (skill, _) = test_skill();
        let preview = skill.preview(&serde_json::json!({"x": 42, "y": 99}));
        assert_eq!(preview, Some("Move mouse to (42, 99)".into()));
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "mouse_move");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
