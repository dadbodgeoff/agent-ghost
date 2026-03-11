//! `screenshot` — capture a screenshot of the entire screen or a region.
//!
//! Low risk. Convergence max: Level 4. No budget limit.
//! Perception-only: does not mutate any state.
//!
//! ## Input
//!
//! | Field    | Type   | Required | Description                                     |
//! |----------|--------|----------|-------------------------------------------------|
//! | `region` | object | no       | `{ x, y, width, height }` crop region           |
//! | `format` | string | no       | "png" (default) or "jpeg"                        |
//!
//! ## Output
//!
//! ```json
//! {
//!   "width": 1920,
//!   "height": 1080,
//!   "format": "png",
//!   "size_bytes": 2048576,
//!   "path": "/tmp/ghost-screenshot-<uuid>.png",
//!   "status": "ok"
//! }
//! ```
//!
//! Note: The `xcap` screen capture dependency is used in production.
//! This module provides the `ScreenshotSkill` struct and a `ScreenCaptureBackend`
//! trait for testability.

use ghost_skills::registry::SkillSource;
use ghost_skills::skill::{Skill, SkillContext, SkillError, SkillResult};

/// Trait abstracting screen capture for testability.
///
/// Production uses `XcapScreenCapture`; tests use `MockScreenCapture`.
pub trait ScreenCaptureBackend: Send + Sync {
    /// Capture the full screen and return raw RGBA pixel data + dimensions.
    fn capture_full_screen(&self) -> Result<CapturedImage, String>;
}

/// Raw captured image data.
pub struct CapturedImage {
    pub width: u32,
    pub height: u32,
    pub rgba_data: Vec<u8>,
}

/// Production screen capture using `xcap`.
pub struct XcapScreenCapture;

impl XcapScreenCapture {
    pub fn try_new() -> Result<Self, String> {
        // xcap availability is verified at capture time, not construction.
        Ok(Self)
    }
}

impl ScreenCaptureBackend for XcapScreenCapture {
    fn capture_full_screen(&self) -> Result<CapturedImage, String> {
        use xcap::Monitor;

        let monitors = Monitor::all().map_err(|e| format!("failed to list monitors: {e}"))?;
        let monitor = monitors
            .into_iter()
            .next()
            .ok_or_else(|| "no monitors found".to_string())?;

        let image = monitor
            .capture_image()
            .map_err(|e| format!("screenshot capture failed: {e}"))?;

        Ok(CapturedImage {
            width: image.width(),
            height: image.height(),
            rgba_data: image.into_raw(),
        })
    }
}

pub fn primary_screen_dimensions() -> Result<(u32, u32), String> {
    let captured = XcapScreenCapture::try_new()?.capture_full_screen()?;
    Ok((captured.width, captured.height))
}

/// Mock screen capture for tests.
pub struct MockScreenCapture {
    width: u32,
    height: u32,
}

impl MockScreenCapture {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

impl ScreenCaptureBackend for MockScreenCapture {
    fn capture_full_screen(&self) -> Result<CapturedImage, String> {
        // Return a minimal RGBA buffer (all black).
        let size = (self.width * self.height * 4) as usize;
        Ok(CapturedImage {
            width: self.width,
            height: self.height,
            rgba_data: vec![0u8; size],
        })
    }
}

pub struct ScreenshotSkill {
    backend: Box<dyn ScreenCaptureBackend>,
}

impl ScreenshotSkill {
    pub fn new(backend: Box<dyn ScreenCaptureBackend>) -> Self {
        Self { backend }
    }

    /// Attempt to create with the production xcap backend.
    pub fn try_new_xcap() -> Result<Self, String> {
        Ok(Self::new(Box::new(XcapScreenCapture::try_new()?)))
    }
}

impl Skill for ScreenshotSkill {
    fn name(&self) -> &str {
        "screenshot"
    }

    fn description(&self) -> &str {
        "Capture a screenshot of the entire screen or a region"
    }

    fn removable(&self) -> bool {
        true
    }
    fn source(&self) -> SkillSource {
        SkillSource::Bundled
    }

    fn execute(&self, _ctx: &SkillContext<'_>, input: &serde_json::Value) -> SkillResult {
        let format = input
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("png");
        if !matches!(format, "png" | "jpeg") {
            return Err(SkillError::InvalidInput(format!(
                "invalid format '{format}', must be: png, jpeg"
            )));
        }

        // Capture the full screen.
        let captured = self
            .backend
            .capture_full_screen()
            .map_err(|e| SkillError::Internal(format!("screen capture failed: {e}")))?;

        let mut width = captured.width;
        let mut height = captured.height;

        // If a crop region is specified, validate it.
        if let Some(region) = input.get("region") {
            let rx = region.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let ry = region.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as u32;
            let rw = region
                .get("width")
                .and_then(|v| v.as_u64())
                .unwrap_or(width as u64) as u32;
            let rh = region
                .get("height")
                .and_then(|v| v.as_u64())
                .unwrap_or(height as u64) as u32;

            if rx + rw > captured.width || ry + rh > captured.height {
                return Err(SkillError::InvalidInput(format!(
                    "crop region ({rx},{ry},{rw},{rh}) exceeds screen bounds ({},{})",
                    captured.width, captured.height,
                )));
            }
            width = rw;
            height = rh;
        }

        // In a full implementation, we would save to a temp file and return the path.
        // For now, return metadata about what would be captured.
        let size_bytes = captured.rgba_data.len();

        let result = serde_json::json!({
            "width": width,
            "height": height,
            "format": format,
            "size_bytes": size_bytes,
            "status": "ok",
        });

        Ok(result)
    }

    fn preview(&self, input: &serde_json::Value) -> Option<String> {
        if input.get("region").is_some() {
            Some("Capture screenshot (cropped region)".into())
        } else {
            Some("Capture full screenshot".into())
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
        SkillContext {
            db,
            agent_id: Uuid::nil(),
            session_id: Uuid::nil(),
            convergence_profile: "standard",
        }
    }

    fn test_skill() -> ScreenshotSkill {
        ScreenshotSkill::new(Box::new(MockScreenCapture::new(1920, 1080)))
    }

    #[test]
    fn captures_full_screen() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({})).unwrap();
        assert_eq!(result["status"], "ok");
        assert_eq!(result["width"], 1920);
        assert_eq!(result["height"], 1080);
        assert_eq!(result["format"], "png");
    }

    #[test]
    fn captures_with_crop_region() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill
            .execute(
                &ctx,
                &serde_json::json!({
                    "region": { "x": 100, "y": 200, "width": 800, "height": 600 }
                }),
            )
            .unwrap();
        assert_eq!(result["width"], 800);
        assert_eq!(result["height"], 600);
    }

    #[test]
    fn rejects_crop_exceeding_bounds() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(
            &ctx,
            &serde_json::json!({
                "region": { "x": 1900, "y": 0, "width": 100, "height": 100 }
            }),
        );
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn rejects_invalid_format() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill.execute(&ctx, &serde_json::json!({"format": "bmp"}));
        assert!(matches!(result, Err(SkillError::InvalidInput(_))));
    }

    #[test]
    fn jpeg_format_accepted() {
        let db = test_db();
        let ctx = test_ctx(&db);
        let skill = test_skill();

        let result = skill
            .execute(&ctx, &serde_json::json!({"format": "jpeg"}))
            .unwrap();
        assert_eq!(result["format"], "jpeg");
    }

    #[test]
    fn preview_full_screen() {
        let skill = test_skill();
        let preview = skill.preview(&serde_json::json!({}));
        assert_eq!(preview, Some("Capture full screenshot".into()));
    }

    #[test]
    fn preview_cropped() {
        let skill = test_skill();
        let preview = skill
            .preview(&serde_json::json!({"region": {"x": 0, "y": 0, "width": 100, "height": 100}}));
        assert_eq!(preview, Some("Capture screenshot (cropped region)".into()));
    }

    #[test]
    fn skill_metadata() {
        let skill = test_skill();
        assert_eq!(skill.name(), "screenshot");
        assert!(skill.removable());
        assert_eq!(skill.source(), SkillSource::Bundled);
    }
}
