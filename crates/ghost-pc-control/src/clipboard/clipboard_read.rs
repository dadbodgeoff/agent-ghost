//! `clipboard_read` — read the current system clipboard contents.
//!
//! Medium risk. Convergence max: Level 2. No budget limit.
//!
//! ## Input
//!
//! No required fields.
//!
//! ## Output
//!
//! ```json
//! {
//!   "text": "clipboard contents",
//!   "has_image": false,
//!   "char_count": 42,
//!   "status": "ok"
//! }
//! ```

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;

pub struct ClipboardReadSkill;

impl ClipboardReadSkill {
    pub fn new() -> Self { Self }
}

impl Default for ClipboardReadSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for ClipboardReadSkill {
    fn name(&self) -> &str { "clipboard_read" }
    fn description(&self) -> &str { "Read the current system clipboard contents" }
    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let mut clipboard = arboard::Clipboard::new().map_err(|e| {
            SkillError::Internal(format!("failed to access clipboard: {e}"))
        })?;

        let text = clipboard.get_text().ok();
        let has_image = clipboard.get_image().is_ok();
        let char_count = text.as_ref().map(|t| t.len()).unwrap_or(0);

        let result = serde_json::json!({
            "text": text,
            "has_image": has_image,
            "char_count": char_count,
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "clipboard_read", input, &result);

        Ok(result)
    }

    fn preview(&self, _input: &serde_json::Value) -> Option<String> {
        Some("Read clipboard".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext { db, agent_id: Uuid::nil(), session_id: Uuid::nil(), convergence_profile: "standard" }
    }

    #[test]
    #[ignore] // Requires display server / clipboard access
    fn reads_clipboard() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ClipboardReadSkill::new();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert!(result.get("text").is_some());
        assert!(result.get("has_image").is_some());
        assert!(result.get("char_count").is_some());
    }

    #[test]
    fn skill_metadata() {
        let skill = ClipboardReadSkill::new();
        assert_eq!(skill.name(), "clipboard_read");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }

    #[test]
    fn preview_output() {
        let skill = ClipboardReadSkill::new();
        assert_eq!(skill.preview(&serde_json::json!({})), Some("Read clipboard".into()));
    }
}
