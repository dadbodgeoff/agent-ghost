//! `accessibility_tree` — query the platform accessibility tree.
//!
//! Low risk. Convergence max: Level 4. No budget limit.
//! Perception-only: does not mutate any state.
//!
//! ## Input
//!
//! | Field       | Type   | Required | Description                              |
//! |-------------|--------|----------|------------------------------------------|
//! | `query`     | string | no       | Search text or element name              |
//! | `role`      | string | no       | Filter by role (e.g. "button", "link")   |
//! | `window`    | string | no       | Limit to a specific window title         |
//! | `max_depth` | u32    | no       | Maximum tree traversal depth (default 5) |
//!
//! ## Output
//!
//! ```json
//! {
//!   "elements": [
//!     { "bounds": {...}, "role": "button", "name": "Submit", "confidence": 1.0 }
//!   ],
//!   "count": 1,
//!   "layer": "AccessibilityTree",
//!   "status": "ok"
//! }
//! ```

use std::sync::Arc;

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::perception::element::{BoundingRect, ResolvedElement};
use crate::platform::accessibility_backend::AccessibilityBackend;

pub struct AccessibilityTreeSkill {
    backend: Arc<dyn AccessibilityBackend>,
}

impl AccessibilityTreeSkill {
    pub fn new(backend: Arc<dyn AccessibilityBackend>) -> Self {
        Self { backend }
    }
}

impl Skill for AccessibilityTreeSkill {
    fn name(&self) -> &str {
        "accessibility_tree"
    }

    fn description(&self) -> &str {
        "Query the platform accessibility tree for UI elements"
    }

    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let query = input.get("query").and_then(|v| v.as_str());
        let role = input.get("role").and_then(|v| v.as_str());
        let window = input.get("window").and_then(|v| v.as_str());
        let max_depth = input.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(5) as u32;

        let nodes = self
            .backend
            .query(window, role, query, max_depth)
            .map_err(|e| SkillError::Internal(format!("accessibility tree query failed: {e}")))?;

        // Convert to ResolvedElements.
        let elements: Vec<serde_json::Value> = nodes
            .iter()
            .map(|n| {
                let elem = ResolvedElement::from_accessibility(
                    BoundingRect {
                        x: n.x,
                        y: n.y,
                        width: n.width,
                        height: n.height,
                    },
                    &n.role,
                    n.name.clone(),
                    n.title.clone().or_else(|| n.value.clone()),
                );
                serde_json::json!({
                    "bounds": {
                        "x": elem.bounds.x,
                        "y": elem.bounds.y,
                        "width": elem.bounds.width,
                        "height": elem.bounds.height,
                    },
                    "role": elem.role,
                    "name": elem.name,
                    "text": elem.text,
                    "confidence": elem.confidence,
                    "layer": "AccessibilityTree",
                })
            })
            .collect();

        let count = elements.len();
        let result = serde_json::json!({
            "elements": elements,
            "count": count,
            "layer": "AccessibilityTree",
            "status": "ok",
        });

        audit::log_pc_action(
            ctx.db,
            ctx.agent_id,
            ctx.session_id,
            "accessibility_tree",
            input,
            &result,
        );

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let query = input.get("query").and_then(|v| v.as_str());
        let role = input.get("role").and_then(|v| v.as_str());
        match (query, role) {
            (Some(q), Some(r)) => Some(format!("Query accessibility tree: {r} matching \"{q}\"")),
            (Some(q), None) => Some(format!("Query accessibility tree: \"{q}\"")),
            (None, Some(r)) => Some(format!("Query accessibility tree: all {r} elements")),
            (None, None) => Some("Query accessibility tree".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::accessibility_backend::{AccessibilityNode, MockAccessibilityBackend};
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

    fn mock_nodes() -> Vec<AccessibilityNode> {
        vec![
            AccessibilityNode {
                role: "AXButton".into(),
                name: Some("Submit".into()),
                title: None,
                value: None,
                x: 100,
                y: 200,
                width: 80,
                height: 30,
                enabled: true,
            },
            AccessibilityNode {
                role: "AXTextField".into(),
                name: Some("Username".into()),
                title: None,
                value: Some("admin".into()),
                x: 100,
                y: 100,
                width: 200,
                height: 25,
                enabled: true,
            },
            AccessibilityNode {
                role: "AXLink".into(),
                name: Some("Help".into()),
                title: Some("Help Center".into()),
                value: None,
                x: 300,
                y: 400,
                width: 50,
                height: 20,
                enabled: true,
            },
        ]
    }

    #[test]
    fn queries_all_elements() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let backend = Arc::new(MockAccessibilityBackend::new(mock_nodes()));
        let skill = AccessibilityTreeSkill::new(backend);

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["count"], 3);
        assert_eq!(result["layer"], "AccessibilityTree");
    }

    #[test]
    fn queries_by_role() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let backend = Arc::new(MockAccessibilityBackend::new(mock_nodes()));
        let skill = AccessibilityTreeSkill::new(backend);

        let result = skill
            .execute(&ctx, &serde_json::json!({"role": "AXButton"}))
            .unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["elements"][0]["name"], "Submit");
    }

    #[test]
    fn queries_by_name() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let backend = Arc::new(MockAccessibilityBackend::new(mock_nodes()));
        let skill = AccessibilityTreeSkill::new(backend);

        let result = skill
            .execute(&ctx, &serde_json::json!({"query": "Help"}))
            .unwrap();
        assert_eq!(result["count"], 1);
    }

    #[test]
    fn elements_have_confidence_one() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let backend = Arc::new(MockAccessibilityBackend::new(mock_nodes()));
        let skill = AccessibilityTreeSkill::new(backend);

        let result = skill
            .execute(&ctx, &serde_json::json!({"role": "AXButton"}))
            .unwrap();
        assert_eq!(result["elements"][0]["confidence"], 1.0);
    }

    #[test]
    fn handles_empty_tree() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let backend = Arc::new(MockAccessibilityBackend::new(vec![]));
        let skill = AccessibilityTreeSkill::new(backend);

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["count"], 0);
    }

    #[test]
    fn preview_with_query_and_role() {
        let backend = Arc::new(MockAccessibilityBackend::new(vec![]));
        let skill = AccessibilityTreeSkill::new(backend);
        let preview = skill.preview(&serde_json::json!({"query": "Submit", "role": "button"}));
        assert_eq!(
            preview,
            Some("Query accessibility tree: button matching \"Submit\"".into())
        );
    }

    #[test]
    fn preview_query_only() {
        let backend = Arc::new(MockAccessibilityBackend::new(vec![]));
        let skill = AccessibilityTreeSkill::new(backend);
        let preview = skill.preview(&serde_json::json!({"query": "Login"}));
        assert_eq!(preview, Some("Query accessibility tree: \"Login\"".into()));
    }

    #[test]
    fn preview_no_params() {
        let backend = Arc::new(MockAccessibilityBackend::new(vec![]));
        let skill = AccessibilityTreeSkill::new(backend);
        let preview = skill.preview(&serde_json::json!({}));
        assert_eq!(preview, Some("Query accessibility tree".into()));
    }

    #[test]
    fn skill_metadata() {
        let backend = Arc::new(MockAccessibilityBackend::new(vec![]));
        let skill = AccessibilityTreeSkill::new(backend);
        assert_eq!(skill.name(), "accessibility_tree");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
