//! `list_processes` — list running processes.
//!
//! Low risk. Convergence max: Level 4. No budget limit.
//! Perception-only: does not mutate any state.
//!
//! ## Input
//!
//! | Field   | Type   | Required | Description                           |
//! |---------|--------|----------|---------------------------------------|
//! | `app`   | string | no       | Filter by app name (case-insensitive) |
//! | `limit` | u32    | no       | Max processes to return (default 500) |
//!
//! ## Output
//!
//! ```json
//! {
//!   "processes": [
//!     { "pid": 1234, "name": "firefox", "cpu_usage": 5.2, "memory_kb": 204800 }
//!   ],
//!   "count": 42,
//!   "status": "ok"
//! }
//! ```

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillResult};
use sysinfo::System;

use crate::audit;

pub struct ListProcessesSkill;

impl ListProcessesSkill {
    pub fn new() -> Self { Self }
}

impl Default for ListProcessesSkill {
    fn default() -> Self { Self::new() }
}

impl Skill for ListProcessesSkill {
    fn name(&self) -> &str { "list_processes" }
    fn description(&self) -> &str { "List running processes" }
    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let filter = input.get("app").and_then(|v| v.as_str());
        let limit = input.get("limit").and_then(|v| v.as_u64()).unwrap_or(500) as usize;

        let mut sys = System::new();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let mut processes: Vec<serde_json::Value> = sys.processes()
            .values()
            .filter(|p| {
                if let Some(f) = filter {
                    p.name().to_string_lossy().to_lowercase().contains(&f.to_lowercase())
                } else {
                    true
                }
            })
            .take(limit)
            .map(|p| serde_json::json!({
                "pid": p.pid().as_u32(),
                "name": p.name().to_string_lossy(),
                "cpu_usage": p.cpu_usage(),
                "memory_kb": p.memory() / 1024,
            }))
            .collect();

        // Sort by memory descending for usability.
        processes.sort_by(|a, b| {
            b["memory_kb"].as_u64().unwrap_or(0)
                .cmp(&a["memory_kb"].as_u64().unwrap_or(0))
        });

        let count = processes.len();
        let result = serde_json::json!({
            "processes": processes,
            "count": count,
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "list_processes", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        match input.get("app").and_then(|v| v.as_str()) {
            Some(app) => Some(format!("List processes matching \"{app}\"")),
            None => Some("List running processes".into()),
        }
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
    fn lists_running_processes() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListProcessesSkill::new();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert!(result["count"].as_u64().unwrap() > 0);
    }

    #[test]
    fn respects_limit() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListProcessesSkill::new();

        let result = skill.execute(&ctx, &serde_json::json!({"limit": 3})).unwrap();
        assert!(result["processes"].as_array().unwrap().len() <= 3);
    }

    #[test]
    fn process_has_expected_fields() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = ListProcessesSkill::new();

        let result = skill.execute(&ctx, &serde_json::json!({"limit": 1})).unwrap();
        let procs = result["processes"].as_array().unwrap();
        if let Some(p) = procs.first() {
            assert!(p.get("pid").is_some());
            assert!(p.get("name").is_some());
            assert!(p.get("memory_kb").is_some());
        }
    }

    #[test]
    fn preview_with_filter() {
        let skill = ListProcessesSkill::new();
        assert_eq!(
            skill.preview(&serde_json::json!({"app": "firefox"})),
            Some("List processes matching \"firefox\"".into()),
        );
    }

    #[test]
    fn skill_metadata() {
        let skill = ListProcessesSkill::new();
        assert_eq!(skill.name(), "list_processes");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
