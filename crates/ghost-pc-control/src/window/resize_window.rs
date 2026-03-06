//! `resize_window` — resize and/or move a window.
//!
//! Medium risk. Convergence max: Level 2. App allowlist enforced.
//!
//! ## Input
//!
//! | Field    | Type   | Required | Description                     |
//! |----------|--------|----------|---------------------------------|
//! | `title`  | string | no       | Match window by title           |
//! | `app`    | string | no       | Match window by app name        |
//! | `x`      | i32    | no       | New X position                  |
//! | `y`      | i32    | no       | New Y position                  |
//! | `width`  | u32    | no       | New width                       |
//! | `height` | u32    | no       | New height                      |

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::window_backend::WindowBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct ResizeWindowSkill {
    backend: Arc<dyn WindowBackend>,
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

impl ResizeWindowSkill {
    pub fn new(
        backend: Arc<dyn WindowBackend>,
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    ) -> Self {
        Self { backend, validator, circuit_breaker }
    }
}

impl Skill for ResizeWindowSkill {
    fn name(&self) -> &str { "resize_window" }
    fn description(&self) -> &str { "Resize and/or move a window" }
    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let title = input.get("title").and_then(|v| v.as_str());
        let app = input.get("app").and_then(|v| v.as_str());

        if title.is_none() && app.is_none() {
            return Err(SkillError::InvalidInput(
                "at least one of 'title' or 'app' must be provided".into()
            ));
        }

        let x = input.get("x").and_then(|v| v.as_i64()).map(|v| v as i32);
        let y = input.get("y").and_then(|v| v.as_i64()).map(|v| v as i32);
        let width = input.get("width").and_then(|v| v.as_u64()).map(|v| v as u32);
        let height = input.get("height").and_then(|v| v.as_u64()).map(|v| v as u32);

        // Validate app against allowlist.
        if let Some(app_name) = app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app_name) {
                audit::log_blocked_action(ctx.db, ctx.agent_id, ctx.session_id, "resize_window", input, &reason);
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Circuit breaker check.
        { self.circuit_breaker.lock().unwrap().check("resize_window")?; }

        let win = self.backend.resize_window(title, app, x, y, width, height).map_err(|e| {
            self.circuit_breaker.lock().unwrap().record_failure();
            SkillError::Internal(format!("failed to resize window: {e}"))
        })?;

        // Record success.
        { self.circuit_breaker.lock().unwrap().record_success(); }

        let result = serde_json::json!({
            "title": win.title,
            "app": win.app,
            "pid": win.pid,
            "bounds": {
                "x": win.x,
                "y": win.y,
                "width": win.width,
                "height": win.height,
            },
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "resize_window", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let target = input.get("title").and_then(|v| v.as_str())
            .or_else(|| input.get("app").and_then(|v| v.as_str()))
            .unwrap_or("window");
        Some(format!("Resize: {target}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::window_backend::{MockWindowBackend, WindowInfo};
    use crate::safety::input_validator::ScreenRegion;
    use uuid::Uuid;
    use std::time::Duration;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext { db, agent_id: Uuid::nil(), session_id: Uuid::nil(), convergence_profile: "standard" }
    }

    fn test_skill() -> ResizeWindowSkill {
        let windows = vec![
            WindowInfo { title: "Doc".into(), app: "Firefox".into(), pid: 100, x: 0, y: 0, width: 800, height: 600 },
        ];
        let backend = Arc::new(MockWindowBackend::new(windows));
        let validator = Arc::new(InputValidator::new(
            vec!["Firefox".into()],
            Some(ScreenRegion { x: 0, y: 0, width: 1920, height: 1080 }),
            vec![],
        ));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, Duration::from_secs(30))));
        ResizeWindowSkill::new(backend, validator, cb)
    }

    #[test]
    fn resizes_window() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"app": "Firefox", "width": 1024, "height": 768})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["bounds"]["width"], 1024);
        assert_eq!(result["bounds"]["height"], 768);
    }

    #[test]
    fn rejects_no_target() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"width": 100}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn validates_app_allowlist() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"app": "Terminal", "width": 100}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "resize_window");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
