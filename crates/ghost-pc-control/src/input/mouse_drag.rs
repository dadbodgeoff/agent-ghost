//! `mouse_drag` — drag the mouse from one position to another.
//!
//! Medium risk. Convergence max: Level 2. Budget: 20 per session.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Description                        |
//! |--------------|--------|----------|------------------------------------|
//! | `from_x`     | i32    | yes      | Start X coordinate                 |
//! | `from_y`     | i32    | yes      | Start Y coordinate                 |
//! | `to_x`       | i32    | yes      | End X coordinate                   |
//! | `to_y`       | i32    | yes      | End Y coordinate                   |
//! | `button`     | string | no       | "left" (default), "right", "middle"|
//! | `target_app` | string | no       | Target application name            |
//!
//! ## Output
//!
//! ```json
//! { "from": { "x": 100, "y": 200 }, "to": { "x": 300, "y": 400 }, "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::{InputBackend, MouseButton};
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct MouseDragSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl MouseDragSkill {
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

impl Skill for MouseDragSkill {
    fn name(&self) -> &str {
        "mouse_drag"
    }

    fn description(&self) -> &str {
        "Drag the mouse from one position to another"
    }

    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let from_x = input
            .get("from_x")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'from_x' (integer)".into())
            })? as i32;
        let from_y = input
            .get("from_y")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| {
                SkillError::InvalidInput("missing required field 'from_y' (integer)".into())
            })? as i32;
        let to_x = input.get("to_x").and_then(|v| v.as_i64()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'to_x' (integer)".into())
        })? as i32;
        let to_y = input.get("to_y").and_then(|v| v.as_i64()).ok_or_else(|| {
            SkillError::InvalidInput("missing required field 'to_y' (integer)".into())
        })? as i32;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        let button = match input
            .get("button")
            .and_then(|v| v.as_str())
            .unwrap_or("left")
        {
            "left" => MouseButton::Left,
            "right" => MouseButton::Right,
            "middle" => MouseButton::Middle,
            other => {
                return Err(SkillError::InvalidInput(format!(
                    "invalid button '{other}', must be: left, right, middle"
                )));
            }
        };

        // Safety: validate both start and end coordinates.
        if let ValidationResult::Denied(reason) = self
            .validator
            .validate_drag(from_x, from_y, to_x, to_y, target_app)
        {
            audit::log_blocked_action(
                ctx.db,
                ctx.agent_id,
                ctx.session_id,
                "mouse_drag",
                input,
                &reason,
            );
            return Err(SkillError::PcControlBlocked(reason));
        }

        // Safety: circuit breaker.
        {
            self.circuit_breaker.lock().unwrap().check("mouse_drag")?;
        }

        // Execute: move to start, press, move to end, release.
        {
            let mut backend = self.backend.lock().unwrap();
            backend.mouse_move_to(from_x, from_y);
            backend.mouse_down(button);
            backend.mouse_move_to(to_x, to_y);
            backend.mouse_up(button);
        }

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "from": { "x": from_x, "y": from_y },
            "to": { "x": to_x, "y": to_y },
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "mouse_drag",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let from_x = input.get("from_x").and_then(|v| v.as_i64())?;
        let from_y = input.get("from_y").and_then(|v| v.as_i64())?;
        let to_x = input.get("to_x").and_then(|v| v.as_i64())?;
        let to_y = input.get("to_y").and_then(|v| v.as_i64())?;
        Some(format!(
            "Drag from ({from_x}, {from_y}) to ({to_x}, {to_y})"
        ))
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

    fn test_skill() -> (MouseDragSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into()],
            Some(ScreenRegion {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }),
            vec![],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(
            100,
            10,
            std::time::Duration::from_secs(30),
        )));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (MouseDragSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn drags_successfully() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill
            .execute(
                &ctx,
                &serde_json::json!({
                    "from_x": 100, "from_y": 200, "to_x": 300, "to_y": 400
                }),
            )
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["from"]["x"], 100);
        assert_eq!(result["to"]["y"], 400);

        let actions = mock.actions();
        assert_eq!(actions.len(), 4); // move, down, move, up
        assert_eq!(actions[0], RecordedAction::MouseMoveTo(100, 200));
        assert_eq!(actions[1], RecordedAction::MouseDown(MouseButton::Left));
        assert_eq!(actions[2], RecordedAction::MouseMoveTo(300, 400));
        assert_eq!(actions[3], RecordedAction::MouseUp(MouseButton::Left));
    }

    #[test]
    fn rejects_missing_coordinates() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"from_x": 100, "from_y": 200}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_drag_outside_safe_zone() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(
            &ctx,
            &serde_json::json!({
                "from_x": 100, "from_y": 200, "to_x": 9999, "to_y": 9999
            }),
        );
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn blocks_disallowed_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(
            &ctx,
            &serde_json::json!({
                "from_x": 100, "from_y": 200, "to_x": 300, "to_y": 400, "target_app": "Terminal"
            }),
        );
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn preview_formats_correctly() {
        let (skill, _) = test_skill();
        let preview = skill.preview(&serde_json::json!({
            "from_x": 10, "from_y": 20, "to_x": 30, "to_y": 40
        }));
        assert_eq!(preview, Some("Drag from (10, 20) to (30, 40)".into()));
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "mouse_drag");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
