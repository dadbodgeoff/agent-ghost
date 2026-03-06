//! `clipboard_write` — write text to the system clipboard.
//!
//! Medium risk. Convergence max: Level 3. Budget: 100 per session.
//!
//! ## Input
//!
//! | Field  | Type   | Required | Description             |
//! |--------|--------|----------|-------------------------|
//! | `text` | string | yes      | Text to copy to clipboard|
//!
//! ## Output
//!
//! ```json
//! { "length": 42, "status": "ok" }
//! ```

use std::sync::{Arc, Mutex};

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::safety::circuit_breaker::PcControlCircuitBreaker;

pub struct ClipboardWriteSkill {
    circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>,
}

impl ClipboardWriteSkill {
    pub fn new(circuit_breaker: Arc<Mutex<PcControlCircuitBreaker>>) -> Self {
        Self { circuit_breaker }
    }
}

impl Skill for ClipboardWriteSkill {
    fn name(&self) -> &str { "clipboard_write" }
    fn description(&self) -> &str { "Write text to the system clipboard" }
    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let text = input.get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| SkillError::InvalidInput("'text' field is required".into()))?;

        // Circuit breaker check.
        { self.circuit_breaker.lock().unwrap().check("clipboard_write")?; }

        let mut clipboard = arboard::Clipboard::new().map_err(|e| {
            SkillError::Internal(format!("failed to access clipboard: {e}"))
        })?;

        clipboard.set_text(text.to_string()).map_err(|e| {
            self.circuit_breaker.lock().unwrap().record_failure();
            SkillError::Internal(format!("failed to write to clipboard: {e}"))
        })?;

        // Record success.
        { self.circuit_breaker.lock().unwrap().record_success(); }

        let result = serde_json::json!({
            "length": text.len(),
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "clipboard_write", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let text = input.get("text").and_then(|v| v.as_str())?;
        let preview_text = if text.len() > 50 {
            format!("{}...", &text[..50])
        } else {
            text.to_string()
        };
        Some(format!("Copy to clipboard: \"{preview_text}\""))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    fn test_skill() -> ClipboardWriteSkill {
        let cb = Arc::new(Mutex::new(PcControlCircuitBreaker::new(100, 10, Duration::from_secs(30))));
        ClipboardWriteSkill::new(cb)
    }

    #[test]
    fn rejects_missing_text_field() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_null_text() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"text": null}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    #[ignore] // Requires display server / clipboard access
    fn writes_text_to_clipboard() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"text": "hello world"})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["length"], 11);
    }

    #[test]
    fn preview_truncates_long_text() {
        let skill = test_skill();
        let long_text = "a".repeat(100);
        let preview = skill.preview(&serde_json::json!({"text": long_text}));
        assert!(preview.unwrap().contains("..."));
    }

    #[test]
    fn preview_short_text() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({"text": "hi"}));
        assert_eq!(preview, Some("Copy to clipboard: \"hi\"".into()));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "clipboard_write");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
