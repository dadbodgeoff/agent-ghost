//! `list_windows` — list all visible windows.
//!
//! Low risk. Convergence max: Level 4. No budget limit.
//! Perception-only: does not mutate any state.
//!
//! ## Input
//!
//! | Field | Type   | Required | Description              |
//! |-------|--------|----------|--------------------------|
//! | `app` | string | no       | Filter by app name       |
//!
//! ## Output
//!
//! ```json
//! {
//!   "windows": [
//!     { "title": "...", "app": "...", "pid": 1234, "bounds": {...} }
//!   ],
//!   "count": 5,
//!   "status": "ok"
//! }
//! ```

use std::sync::Arc;

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::platform::window_backend::WindowBackend;

pub struct ListWindowsSkill {
    backend: Arc<dyn WindowBackend>,
}

impl ListWindowsSkill {
    pub fn new(backend: Arc<dyn WindowBackend>) -> Self {
        Self { backend }
    }
}

impl Skill for ListWindowsSkill {
    fn name(&self) -> &str {
        "list_windows"
    }
    fn description(&self) -> &str {
        "List all visible windows"
    }
    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let app_filter = input.get("app").and_then(|v| v.as_str());

        let windows = self
            .backend
            .list_windows(app_filter)
            .map_err(|e| SkillError::Internal(format!("failed to list windows: {e}")))?;

        let window_json: Vec<serde_json::Value> = windows
            .iter()
            .map(|w| {
                serde_json::json!({
                    "title": w.title,
                    "app": w.app,
                    "pid": w.pid,
                    "bounds": {
                        "x": w.x,
                        "y": w.y,
                        "width": w.width,
                        "height": w.height,
                    }
                })
            })
            .collect();

        let count = window_json.len();
        let result = serde_json::json!({
            "windows": window_json,
            "count": count,
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "list_windows",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        match input.get("app").and_then(|v| v.as_str()) {
            Some(app) => Some(format!("List windows for \"{app}\"")),
            None => Some("List all visible windows".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::window_backend::{MockWindowBackend, WindowInfo};
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

    fn mock_windows() -> Vec<WindowInfo> {
        vec![
            WindowInfo {
                title: "Document".into(),
                app: "Firefox".into(),
                pid: 100,
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            WindowInfo {
                title: "Terminal".into(),
                app: "Terminal".into(),
                pid: 200,
                x: 100,
                y: 100,
                width: 600,
                height: 400,
            },
        ]
    }

    #[test]
    fn lists_all_windows() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListWindowsSkill::new(Arc::new(MockWindowBackend::new(mock_windows())));

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["count"], 2);
        assert_eq!(result["windows"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn filters_by_app() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListWindowsSkill::new(Arc::new(MockWindowBackend::new(mock_windows())));

        let result = skill
            .execute(&ctx, &serde_json::json!({"app": "Firefox"}))
            .unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["windows"][0]["app"], "Firefox");
    }

    #[test]
    fn handles_empty_window_list() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListWindowsSkill::new(Arc::new(MockWindowBackend::empty()));

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["count"], 0);
    }

    #[test]
    fn window_has_expected_fields() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListWindowsSkill::new(Arc::new(MockWindowBackend::new(mock_windows())));

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        let win = &result["windows"][0];
        assert!(win.get("title").is_some());
        assert!(win.get("app").is_some());
        assert!(win.get("pid").is_some());
        assert!(win.get("bounds").is_some());
    }

    #[test]
    fn skill_metadata() {
        let skill = ListWindowsSkill::new(Arc::new(MockWindowBackend::empty()));
        assert_eq!(skill.name(), "list_windows");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
