//! Input validation layer for PC control skills.
//!
//! Validates every input action (click, drag, hotkey) against:
//! - Screen safe zone (optional rectangular region)
//! - Blocked hotkeys (always-denied key combinations)
//! - Application allowlist (which apps can be targeted)

use serde::{Deserialize, Serialize};

/// Result of an input validation check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    /// The action is allowed.
    Allowed,
    /// The action is denied for the given reason.
    Denied(String),
}

/// A rectangular screen region for safe zone enforcement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl ScreenRegion {
    /// Check whether a point falls within this region.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.width as i32
            && py < self.y + self.height as i32
    }
}

/// Validates PC control actions against safety constraints.
///
/// Thread-safe and immutable after construction. Shared across all input
/// skills via `Arc<InputValidator>`.
pub struct InputValidator {
    app_allowlist: Vec<String>,
    safe_zone: Option<ScreenRegion>,
    blocked_hotkeys: Vec<String>,
}

impl InputValidator {
    /// Create a new validator with the given constraints.
    pub fn new(
        app_allowlist: Vec<String>,
        safe_zone: Option<ScreenRegion>,
        blocked_hotkeys: Vec<String>,
    ) -> Self {
        // Normalize hotkey strings to lowercase for case-insensitive matching.
        let blocked_hotkeys = blocked_hotkeys
            .into_iter()
            .map(|h| h.to_lowercase())
            .collect();

        Self {
            app_allowlist,
            safe_zone,
            blocked_hotkeys,
        }
    }

    /// Validate a click/move at the given coordinates.
    ///
    /// Checks:
    /// 1. Coordinates fall within the safe zone (if configured).
    /// 2. Target app is in the allowlist (if specified).
    pub fn validate_click(
        &self,
        x: i32,
        y: i32,
        target_app: Option<&str>,
    ) -> ValidationResult {
        // Check safe zone.
        if let Some(ref zone) = self.safe_zone {
            if !zone.contains(x, y) {
                return ValidationResult::Denied(format!(
                    "coordinates ({x}, {y}) outside safe zone \
                     (x={}, y={}, w={}, h={})",
                    zone.x, zone.y, zone.width, zone.height,
                ));
            }
        }

        // Check app allowlist.
        if let Some(app) = target_app {
            if let result @ ValidationResult::Denied(_) = self.validate_app(app) {
                return result;
            }
        }

        ValidationResult::Allowed
    }

    /// Validate a drag from one point to another.
    ///
    /// Both the start and end points must be within the safe zone.
    pub fn validate_drag(
        &self,
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
        target_app: Option<&str>,
    ) -> ValidationResult {
        if let Some(ref zone) = self.safe_zone {
            if !zone.contains(from_x, from_y) {
                return ValidationResult::Denied(format!(
                    "drag start ({from_x}, {from_y}) outside safe zone"
                ));
            }
            if !zone.contains(to_x, to_y) {
                return ValidationResult::Denied(format!(
                    "drag end ({to_x}, {to_y}) outside safe zone"
                ));
            }
        }

        if let Some(app) = target_app {
            if let result @ ValidationResult::Denied(_) = self.validate_app(app) {
                return result;
            }
        }

        ValidationResult::Allowed
    }

    /// Validate a hotkey combination (e.g., "Ctrl+C", "Cmd+Q").
    ///
    /// Case-insensitive matching against the blocked hotkeys list.
    pub fn validate_hotkey(&self, keys: &str) -> ValidationResult {
        let normalized = keys.to_lowercase();
        if self.blocked_hotkeys.iter().any(|h| *h == normalized) {
            return ValidationResult::Denied(format!(
                "hotkey '{keys}' blocked by safety policy"
            ));
        }
        ValidationResult::Allowed
    }

    /// Validate that an application is in the allowlist.
    pub fn validate_app(&self, app_name: &str) -> ValidationResult {
        if self.app_allowlist.is_empty() {
            // Empty allowlist means no apps are allowed.
            return ValidationResult::Denied(
                "no apps in allowlist — configure pc_control.allowed_apps".into(),
            );
        }
        if !self.app_allowlist.iter().any(|a| a == app_name) {
            return ValidationResult::Denied(format!(
                "app '{app_name}' not in allowlist: {:?}",
                self.app_allowlist,
            ));
        }
        ValidationResult::Allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_validator() -> InputValidator {
        InputValidator::new(
            vec!["Firefox".into(), "VS Code".into()],
            Some(ScreenRegion {
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
            }),
            vec!["Ctrl+Alt+Delete".into(), "Cmd+Q".into(), "Alt+F4".into()],
        )
    }

    // ── Safe zone tests ──────────────────────────────────────────

    #[test]
    fn click_within_safe_zone_allowed() {
        let v = default_validator();
        assert_eq!(
            v.validate_click(100, 200, Some("Firefox")),
            ValidationResult::Allowed,
        );
    }

    #[test]
    fn click_at_safe_zone_boundary_allowed() {
        let v = default_validator();
        // Top-left corner.
        assert_eq!(v.validate_click(0, 0, None), ValidationResult::Allowed);
        // Bottom-right edge (1919, 1079 is the last valid pixel).
        assert_eq!(
            v.validate_click(1919, 1079, None),
            ValidationResult::Allowed,
        );
    }

    #[test]
    fn click_outside_safe_zone_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_click(1920, 500, None),
            ValidationResult::Denied(ref msg) if msg.contains("outside safe zone")
        ));
        assert!(matches!(
            v.validate_click(100, 1080, None),
            ValidationResult::Denied(ref msg) if msg.contains("outside safe zone")
        ));
        assert!(matches!(
            v.validate_click(-1, 0, None),
            ValidationResult::Denied(ref msg) if msg.contains("outside safe zone")
        ));
    }

    #[test]
    fn click_with_no_safe_zone_allowed() {
        let v = InputValidator::new(
            vec!["Firefox".into()],
            None,
            vec![],
        );
        assert_eq!(
            v.validate_click(99999, 99999, Some("Firefox")),
            ValidationResult::Allowed,
        );
    }

    // ── App allowlist tests ──────────────────────────────────────

    #[test]
    fn app_in_allowlist_allowed() {
        let v = default_validator();
        assert_eq!(v.validate_app("Firefox"), ValidationResult::Allowed);
        assert_eq!(v.validate_app("VS Code"), ValidationResult::Allowed);
    }

    #[test]
    fn app_not_in_allowlist_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_app("Terminal"),
            ValidationResult::Denied(ref msg) if msg.contains("not in allowlist")
        ));
    }

    #[test]
    fn empty_allowlist_denies_all() {
        let v = InputValidator::new(vec![], None, vec![]);
        assert!(matches!(
            v.validate_app("Firefox"),
            ValidationResult::Denied(ref msg) if msg.contains("no apps in allowlist")
        ));
    }

    #[test]
    fn click_without_target_app_skips_allowlist() {
        let v = default_validator();
        assert_eq!(
            v.validate_click(100, 100, None),
            ValidationResult::Allowed,
        );
    }

    // ── Hotkey tests ─────────────────────────────────────────────

    #[test]
    fn blocked_hotkey_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_hotkey("Ctrl+Alt+Delete"),
            ValidationResult::Denied(ref msg) if msg.contains("blocked by safety policy")
        ));
        assert!(matches!(
            v.validate_hotkey("Cmd+Q"),
            ValidationResult::Denied(_)
        ));
        assert!(matches!(
            v.validate_hotkey("Alt+F4"),
            ValidationResult::Denied(_)
        ));
    }

    #[test]
    fn allowed_hotkey_passes() {
        let v = default_validator();
        assert_eq!(v.validate_hotkey("Ctrl+C"), ValidationResult::Allowed);
        assert_eq!(v.validate_hotkey("Ctrl+V"), ValidationResult::Allowed);
    }

    #[test]
    fn hotkey_matching_is_case_insensitive() {
        let v = default_validator();
        assert!(matches!(
            v.validate_hotkey("ctrl+alt+delete"),
            ValidationResult::Denied(_)
        ));
        assert!(matches!(
            v.validate_hotkey("CMD+Q"),
            ValidationResult::Denied(_)
        ));
    }

    // ── Drag tests ───────────────────────────────────────────────

    #[test]
    fn drag_within_safe_zone_allowed() {
        let v = default_validator();
        assert_eq!(
            v.validate_drag(100, 100, 500, 500, Some("Firefox")),
            ValidationResult::Allowed,
        );
    }

    #[test]
    fn drag_start_outside_safe_zone_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_drag(-10, 100, 500, 500, None),
            ValidationResult::Denied(ref msg) if msg.contains("drag start")
        ));
    }

    #[test]
    fn drag_end_outside_safe_zone_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_drag(100, 100, 2000, 500, None),
            ValidationResult::Denied(ref msg) if msg.contains("drag end")
        ));
    }

    #[test]
    fn drag_to_disallowed_app_denied() {
        let v = default_validator();
        assert!(matches!(
            v.validate_drag(100, 100, 200, 200, Some("Terminal")),
            ValidationResult::Denied(ref msg) if msg.contains("not in allowlist")
        ));
    }
}
