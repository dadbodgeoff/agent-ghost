//! `ocr_extract` — extract text from the screen using OCR.
//!
//! Low risk. Convergence max: Level 4. No budget limit.
//! Perception-only: does not mutate any state.
//!
//! ## Input
//!
//! | Field    | Type   | Required | Description                              |
//! |----------|--------|----------|------------------------------------------|
//! | `region` | object | no       | `{ x, y, width, height }` to restrict OCR|
//! | `query`  | string | no       | Text to search for in OCR results        |
//!
//! ## Output
//!
//! ```json
//! {
//!   "elements": [
//!     { "bounds": {...}, "text": "Submit", "confidence": 0.97 }
//!   ],
//!   "count": 1,
//!   "layer": "Ocr",
//!   "status": "ok"
//! }
//! ```

use std::sync::Arc;

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

use crate::audit;
use crate::perception::element::ResolvedElement;
use crate::perception::screenshot::ScreenCaptureBackend;
use crate::platform::ocr_backend::OcrBackend;

pub struct OcrExtractSkill {
    screen_backend: Box<dyn ScreenCaptureBackend>,
    ocr_backend: Arc<dyn OcrBackend>,
}

impl OcrExtractSkill {
    pub fn new(
        screen_backend: Box<dyn ScreenCaptureBackend>,
        ocr_backend: Arc<dyn OcrBackend>,
    ) -> Self {
        Self { screen_backend, ocr_backend }
    }
}

impl Skill for OcrExtractSkill {
    fn name(&self) -> &str { "ocr_extract" }

    fn description(&self) -> &str {
        "Extract text from the screen using OCR"
    }

    fn removable(&self) -> bool { true }
    fn source(&self) -> SkillSource { SkillSource::Bundled }

    fn execute(&self, ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let query = input.get("query").and_then(|v| v.as_str());

        // Capture a screenshot.
        let captured = self.screen_backend.capture_full_screen().map_err(|e| {
            SkillError::Internal(format!("screen capture failed: {e}"))
        })?;

        // Run OCR on the captured image.
        let regions = self.ocr_backend.extract_text(
            &captured.rgba_data,
            captured.width,
            captured.height,
        ).map_err(|e| {
            SkillError::Internal(format!("OCR extraction failed: {e}"))
        })?;

        // Filter by query text if provided.
        let filtered: Vec<_> = if let Some(q) = query {
            let q_lower = q.to_lowercase();
            regions.into_iter()
                .filter(|r| r.text.to_lowercase().contains(&q_lower))
                .collect()
        } else {
            regions
        };

        // Convert to ResolvedElements.
        let elements: Vec<serde_json::Value> = filtered.iter().map(|r| {
            let elem = ResolvedElement::from_ocr(r.bounds.clone(), r.text.clone(), r.confidence);
            serde_json::json!({
                "bounds": {
                    "x": elem.bounds.x,
                    "y": elem.bounds.y,
                    "width": elem.bounds.width,
                    "height": elem.bounds.height,
                },
                "text": elem.text,
                "confidence": elem.confidence,
                "layer": "Ocr",
            })
        }).collect();

        let count = elements.len();
        let result = serde_json::json!({
            "elements": elements,
            "count": count,
            "layer": "Ocr",
            "status": "ok",
        });

        audit::log_pc_action(ctx.db, ctx.agent_id, ctx.session_id, "ocr_extract", input, &result);

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        let query = input.get("query").and_then(|v| v.as_str());
        match query {
            Some(q) => Some(format!("OCR: search for \"{q}\"")),
            None => Some("OCR: extract all visible text".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perception::element::BoundingRect;
    use crate::perception::screenshot::MockScreenCapture;
    use crate::platform::ocr_backend::{MockOcrBackend, OcrTextRegion};
    use uuid::Uuid;

    fn test_db() -> rusqlite::Connection {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        cortex_storage::migrations::run_migrations(&db).unwrap();
        db
    }

    fn test_ctx(db: &rusqlite::Connection) -> SkillContext<'_> {
        SkillContext { db, agent_id: Uuid::nil(), session_id: Uuid::nil(), convergence_profile: "standard" }
    }

    fn mock_regions() -> Vec<OcrTextRegion> {
        vec![
            OcrTextRegion {
                text: "Submit".into(),
                bounds: BoundingRect { x: 100, y: 200, width: 80, height: 30 },
                confidence: 0.97,
            },
            OcrTextRegion {
                text: "Cancel".into(),
                bounds: BoundingRect { x: 200, y: 200, width: 80, height: 30 },
                confidence: 0.95,
            },
            OcrTextRegion {
                text: "Hello World".into(),
                bounds: BoundingRect { x: 50, y: 50, width: 200, height: 20 },
                confidence: 0.99,
            },
        ]
    }

    fn test_skill() -> OcrExtractSkill {
        OcrExtractSkill::new(
            Box::new(MockScreenCapture::new(1920, 1080)),
            Arc::new(MockOcrBackend::new(mock_regions())),
        )
    }

    #[test]
    fn extracts_all_text() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["count"], 3);
        assert_eq!(result["layer"], "Ocr");
    }

    #[test]
    fn filters_by_query() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"query": "Submit"})).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["elements"][0]["text"], "Submit");
    }

    #[test]
    fn query_is_case_insensitive() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"query": "hello"})).unwrap();
        assert_eq!(result["count"], 1);
        assert_eq!(result["elements"][0]["text"], "Hello World");
    }

    #[test]
    fn handles_no_results() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = OcrExtractSkill::new(
            Box::new(MockScreenCapture::new(1920, 1080)),
            Arc::new(MockOcrBackend::empty()),
        );

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["count"], 0);
    }

    #[test]
    fn elements_have_ocr_layer() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        for elem in result["elements"].as_array().unwrap() {
            assert_eq!(elem["layer"], "Ocr");
        }
    }

    #[test]
    fn preview_with_query() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({"query": "Submit"}));
        assert_eq!(preview, Some("OCR: search for \"Submit\"".into()));
    }

    #[test]
    fn preview_no_query() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({}));
        assert_eq!(preview, Some("OCR: extract all visible text".into()));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "ocr_extract");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
