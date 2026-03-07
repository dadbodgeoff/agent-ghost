//! `launch_app` — launch an application by name or path.
//!
//! Medium risk. Convergence max: Level 2. Budget: 10 per session.
//! App allowlist enforced.
//!
//! ## Input
//!
//! | Field  | Type   | Required | Description               |
//! |--------|--------|----------|---------------------------|
//! | `app`  | string | yes      | Application name or path  |
//! | `args` | array  | no       | Command-line arguments    |

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::window_backend::WindowBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct LaunchAppSkill {
    backend: Arc<dyn WindowBackend>,
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

impl LaunchAppSkill {
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

impl Skill for LaunchAppSkill {
    fn name(&self) -> &str {
        "launch_app"
    }
    fn description(&self) -> &str {
        "Launch an application by name or path"
    }
    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let app = input
            .get("app")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidInput("'app' field is required".into()))?;

        let args: Vec<String> = input
            .get("args")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();

        // Validate against allowlist.
        if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
            audit::log_blocked_action(
                ctx.db,
                ctx.agent_id,
                ctx.session_id,
                "launch_app",
                input,
                &reason,
            );
            return Err(SkillError::PcControlBlocked(reason));
        }

        // Circuit breaker check.
        {
            self.circuit_breaker.lock().unwrap().check("launch_app")?;
        }

        let launch = self.backend.launch_app(app, &args).map_err(|e| {
            self.circuit_breaker.lock().unwrap().record_failure();
            SkillError::Internal(format!("failed to launch app: {e}"))
        })?;

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "pid": launch.pid,
            "app": launch.app,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "launch_app",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let app = input
            .get("app")
            .and_then(|v| v.as_str())
            .unwrap_or("application");
        Some(format!("Launch: {app}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::window_backend::MockWindowBackend;
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

    fn test_skill() -> LaunchAppSkill {
        let backend = Arc::new(MockWindowBackend::empty());
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
        LaunchAppSkill::new(backend, validator, cb)
    }

    #[test]
    fn launches_app() {
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
    fn rejects_missing_app_field() {
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

        let result = skill.execute(&ctx, &serde_json::json!({"app": "Malware"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "launch_app");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
