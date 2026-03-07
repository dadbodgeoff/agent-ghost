//! `focus_window` — bring a window to the foreground.
//!
//! Medium risk. Convergence max: Level 3. App allowlist enforced.
//!
//! ## Input
//!
//! | Field   | Type   | Required | Description                    |
//! |---------|--------|----------|--------------------------------|
//! | `title` | string | no       | Match window by title (prefix) |
//! | `app`   | string | no       | Match window by app name       |
//! | `pid`   | u32    | no       | Match window by process ID     |
//!
//! At least one of `title`, `app`, or `pid` must be provided.

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::window_backend::WindowBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct FocusWindowSkill {
    backend: Arc<dyn WindowBackend>,
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

impl FocusWindowSkill {
    pub fn new(
        backend: Arc<dyn WindowBackend>,
        validator: Arc<InputValidator>,
        circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    ) -> Self {
        Self {
            backend,
            validator,
            circuit_breaker,
        }
    }
}

impl Skill for FocusWindowSkill {
    fn name(&self) -> &str {
        "focus_window"
    }
    fn description(&self) -> &str {
        "Bring a window to the foreground"
    }
    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let title = input.get("title").and_then(|v| v.as_str());
        let app = input.get("app").and_then(|v| v.as_str());
        let pid = input.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);

        if title.is_none() && app.is_none() && pid.is_none() {
            return Err(SkillError::InvalidInput(
                "at least one of 'title', 'app', or 'pid' must be provided".into(),
            ));
        }

        // Validate app against allowlist.
        if let Some(app_name) = app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app_name) {
                audit::log_blocked_action(
                    ctx.db,
                    ctx.agent_id,
                    ctx.session_id,
                    "focus_window",
                    input,
                    &reason,
                );
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Circuit breaker check.
        {
            self.circuit_breaker.lock().unwrap().check("focus_window")?;
        }

        let win = self.backend.focus_window(title, app, pid).map_err(|e| {
            self.circuit_breaker.lock().unwrap().record_failure();
            SkillError::Internal(format!("failed to focus window: {e}"))
        })?;

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "title": win.title,
            "app": win.app,
            "pid": win.pid,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "focus_window",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let target = input
            .get("title")
            .and_then(|v| v.as_str())
            .or_else(|| input.get("app").and_then(|v| v.as_str()))
            .unwrap_or("window");
        Some(format!("Focus: {target}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::window_backend::{MockWindowBackend, WindowInfo};
    use crate::safety::input_validator::ScreenRegion;
    use std::time::Duration;
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

    fn test_skill() -> FocusWindowSkill {
        let windows = vec![WindowInfo {
            title: "Document".into(),
            app: "Firefox".into(),
            pid: 100,
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        }];
        let backend = Arc::new(MockWindowBackend::new(windows));
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
            Duration::from_secs(30),
        )));
        FocusWindowSkill::new(backend, validator, cb)
    }

    #[test]
    fn focuses_window_by_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"app": "Firefox"}))
            .unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["app"], "Firefox");
    }

    #[test]
    fn rejects_no_identifier() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn validates_app_allowlist() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"app": "Terminal"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "focus_window");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
