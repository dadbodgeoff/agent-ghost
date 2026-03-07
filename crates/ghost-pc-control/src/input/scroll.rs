//! `scroll` — scroll the mouse wheel vertically or horizontally.
//!
//! Medium risk. Convergence max: Level 2. Budget: total.
//!
//! ## Input
//!
//! | Field        | Type   | Required | Default    | Description                     |
//! |--------------|--------|----------|------------|---------------------------------|
//! | `direction`  | string | no       | "down"     | "up", "down", "left", "right"   |
//! | `amount`     | i32    | no       | 3          | Number of scroll steps           |
//! | `target_app` | string | no       | —          | Target application name          |
//!
//! ## Output
//!
//! ```json
//! { "direction": "down", "amount": 3, "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::input_backend::InputBackend;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;
use crate::safety::input_validator::{InputValidator, ValidationResult};

pub struct ScrollSkill {
    validator: Arc<InputValidator>,
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
    backend: Arc<Mutex<dyn InputBackend>>,
}

impl ScrollSkill {
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

impl Skill for ScrollSkill {
    fn name(&self) -> &str {
        "scroll"
    }

    fn description(&self) -> &str {
        "Scroll the mouse wheel vertically or horizontally"
    }

    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let direction = input
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("down");
        let amount = input.get("amount").and_then(|v| v.as_i64()).unwrap_or(3) as i32;
        let target_app = input.get("target_app").and_then(|v| v.as_str());

        // Validate direction.
        if !matches!(direction, "up" | "down" | "left" | "right") {
            return Err(SkillError::InvalidInput(format!(
                "invalid direction '{direction}', must be: up, down, left, right"
            )));
        }

        // Validate amount is positive.
        if amount <= 0 {
            return Err(SkillError::InvalidInput(format!(
                "amount must be positive, got {amount}"
            )));
        }

        // Safety: validate app if specified.
        if let Some(app) = target_app {
            if let ValidationResult::Denied(reason) = self.validator.validate_app(app) {
                audit::log_blocked_action(
                    ctx.db,
                    ctx.agent_id,
                    ctx.session_id,
                    "scroll",
                    input,
                    &reason,
                );
                return Err(SkillError::PcControlBlocked(reason));
            }
        }

        // Safety: circuit breaker.
        {
            self.circuit_breaker.lock().unwrap().check("scroll")?;
        }

        // Execute: scroll in the requested direction.
        {
            let mut backend = self.backend.lock().unwrap();
            match direction {
                "up" => backend.scroll_y(-amount),
                "down" => backend.scroll_y(amount),
                "left" => backend.scroll_x(-amount),
                "right" => backend.scroll_x(amount),
                _ => unreachable!(), // validated above
            }
        }

        // Record success.
        {
            self.circuit_breaker.lock().unwrap().record_success();
        }

        let result = serde_json::json!({
            "direction": direction,
            "amount": amount,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "scroll",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let direction = input
            .get("direction")
            .and_then(|v| v.as_str())
            .unwrap_or("down");
        let amount = input.get("amount").and_then(|v| v.as_i64()).unwrap_or(3);
        Some(format!("Scroll {direction} {amount} steps"))
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
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    fn test_skill() -> (ScrollSkill, MockInputBackend) {
        let mock = MockInputBackend::new();
        let validator = Arc::new(InputValidator::new(vec!["Firefox".into()], None, vec![]));
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(
            100,
            10,
            std::time::Duration::from_secs(30),
        )));
        let backend: Arc<Mutex<dyn InputBackend>> = Arc::new(Mutex::new(mock.clone()));
        (ScrollSkill::new(validator, cb, backend), mock)
    }

    #[test]
    fn scrolls_down_by_default() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["direction"], "down");
        assert_eq!(result["amount"], 3);

        let actions = mock.actions();
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], RecordedAction::ScrollY(3));
    }

    #[test]
    fn scrolls_up() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"direction": "up", "amount": 5}))
            .unwrap();
        assert_eq!(result["direction"], "up");
        assert_eq!(result["amount"], 5);

        let actions = mock.actions();
        assert_eq!(actions[0], RecordedAction::ScrollY(-5));
    }

    #[test]
    fn scrolls_horizontally() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        skill
            .execute(&ctx, &serde_json::json!({"direction": "left", "amount": 2}))
            .unwrap();
        let actions = mock.actions();
        assert_eq!(actions[0], RecordedAction::ScrollX(-2));

        mock.clear();
        skill
            .execute(
                &ctx,
                &serde_json::json!({"direction": "right", "amount": 4}),
            )
            .unwrap();
        let actions = mock.actions();
        assert_eq!(actions[0], RecordedAction::ScrollX(4));
    }

    #[test]
    fn rejects_invalid_direction() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"direction": "diagonal"}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_zero_amount() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, _) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"amount": 0}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn blocks_disallowed_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let (skill, mock) = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"target_app": "Terminal"}));
        assert!(matches!(result, Err(SkillError::PcControlBlocked(_))));
        assert!(mock.actions().is_empty());
    }

    #[test]
    fn preview_formats_correctly() {
        let (skill, _) = test_skill();
        let preview = skill.preview(&serde_json::json!({"direction": "up", "amount": 10}));
        assert_eq!(preview, Some("Scroll up 10 steps".into()));
    }

    #[test]
    fn skill_metadata() {
        let (skill, _) = test_skill();
        assert_eq!(skill.name(), "scroll");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
