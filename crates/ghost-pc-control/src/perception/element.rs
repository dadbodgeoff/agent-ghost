//! Core element types shared across the perception stack.
//!
//! All three perception layers (accessibility tree, OCR, vision model)
//! produce `ResolvedElement` values. Downstream input skills can target
//! elements by clicking their center coordinates or typing into focused
//! elements.

use serde::{Deserialize, Serialize};

/// A rectangular bounding box in screen coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundingRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl BoundingRect {
    /// Center point of this rectangle.
    pub fn center(&self) -> (i32, i32) {
        (
            self.x + (self.width as i32) / 2,
            self.y + (self.height as i32) / 2,
        )
    }

    /// Check whether a point falls within this rectangle.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x + self.width as i32
            && py < self.y + self.height as i32
    }
}

/// Which perception layer resolved this element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerceptionLayer {
    /// Platform accessibility API (highest priority, most semantic).
    AccessibilityTree,
    /// OCR text extraction from screenshot.
    Ocr,
    /// Vision model element identification (slowest, most capable).
    VisionModel,
}

/// A UI element resolved by the perception stack.
///
/// Contains enough information for input skills to target it (click,
/// type into, etc.) and for the agent to understand what it is.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedElement {
    /// Bounding rectangle in screen coordinates.
    pub bounds: BoundingRect,

    /// Element role (e.g., "button", "text_field", "link", "menu_item").
    /// From accessibility tree or vision model classification.
    pub role: Option<String>,

    /// Visible text or label.
    pub text: Option<String>,

    /// Accessibility name (from the accessibility tree).
    pub name: Option<String>,

    /// Confidence score (0.0–1.0). Set by OCR and vision model layers.
    /// Accessibility tree elements are always 1.0.
    pub confidence: f64,

    /// Which perception layer produced this element.
    pub layer: PerceptionLayer,
}

impl ResolvedElement {
    /// Create an element from an accessibility tree node.
    pub fn from_accessibility(
        bounds: BoundingRect,
        role: impl Into<String>,
        name: Option<String>,
        text: Option<String>,
    ) -> Self {
        Self {
            bounds,
            role: Some(role.into()),
            text,
            name,
            confidence: 1.0,
            layer: PerceptionLayer::AccessibilityTree,
        }
    }

    /// Create an element from OCR results.
    pub fn from_ocr(bounds: BoundingRect, text: String, confidence: f64) -> Self {
        Self {
            bounds,
            role: None,
            text: Some(text),
            name: None,
            confidence,
            layer: PerceptionLayer::Ocr,
        }
    }

    /// Create an element from vision model results.
    pub fn from_vision(
        bounds: BoundingRect,
        role: String,
        text: Option<String>,
        confidence: f64,
    ) -> Self {
        Self {
            bounds,
            role: Some(role),
            text,
            name: None,
            confidence,
            layer: PerceptionLayer::VisionModel,
        }
    }

    /// Center point of this element's bounding rectangle.
    pub fn center(&self) -> (i32, i32) {
        self.bounds.center()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounding_rect_center() {
        let rect = BoundingRect {
            x: 100,
            y: 200,
            width: 50,
            height: 30,
        };
        assert_eq!(rect.center(), (125, 215));
    }

    #[test]
    fn bounding_rect_center_zero_origin() {
        let rect = BoundingRect {
            x: 0,
            y: 0,
            width: 100,
            height: 100,
        };
        assert_eq!(rect.center(), (50, 50));
    }

    #[test]
    fn bounding_rect_contains() {
        let rect = BoundingRect {
            x: 10,
            y: 20,
            width: 100,
            height: 50,
        };
        assert!(rect.contains(10, 20)); // top-left
        assert!(rect.contains(109, 69)); // bottom-right edge
        assert!(!rect.contains(110, 20)); // just outside
        assert!(!rect.contains(10, 70)); // just outside
        assert!(!rect.contains(9, 20)); // just left
    }

    #[test]
    fn resolved_element_from_accessibility() {
        let elem = ResolvedElement::from_accessibility(
            BoundingRect {
                x: 0,
                y: 0,
                width: 80,
                height: 30,
            },
            "button",
            Some("Submit".into()),
            Some("Submit".into()),
        );
        assert_eq!(elem.confidence, 1.0);
        assert_eq!(elem.layer, PerceptionLayer::AccessibilityTree);
        assert_eq!(elem.role.as_deref(), Some("button"));
        assert_eq!(elem.center(), (40, 15));
    }

    #[test]
    fn resolved_element_from_ocr() {
        let elem = ResolvedElement::from_ocr(
            BoundingRect {
                x: 50,
                y: 100,
                width: 200,
                height: 20,
            },
            "Hello World".into(),
            0.95,
        );
        assert_eq!(elem.confidence, 0.95);
        assert_eq!(elem.layer, PerceptionLayer::Ocr);
        assert_eq!(elem.text.as_deref(), Some("Hello World"));
        assert!(elem.role.is_none());
    }

    #[test]
    fn resolved_element_from_vision() {
        let elem = ResolvedElement::from_vision(
            BoundingRect {
                x: 300,
                y: 400,
                width: 60,
                height: 25,
            },
            "link".into(),
            Some("Click here".into()),
            0.87,
        );
        assert_eq!(elem.confidence, 0.87);
        assert_eq!(elem.layer, PerceptionLayer::VisionModel);
        assert_eq!(elem.role.as_deref(), Some("link"));
    }

    #[test]
    fn perception_layer_serializes() {
        let json = serde_json::to_string(&PerceptionLayer::AccessibilityTree).unwrap();
        assert_eq!(json, "\"AccessibilityTree\"");

        let restored: PerceptionLayer = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, PerceptionLayer::AccessibilityTree);
    }
}
