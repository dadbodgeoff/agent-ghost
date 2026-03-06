//! Perception skills: screen capture, accessibility tree, OCR.
//!
//! The perception stack has three layers, tried in priority order:
//!
//! 1. **Accessibility tree** (fastest, most semantic) — platform-native
//!    accessibility APIs provide structured element data.
//! 2. **OCR** — runs `ocrs` on a screenshot to extract visible text
//!    and bounding boxes.
//! 3. **Vision model** — sends a screenshot to a VLM for element
//!    identification (slowest, most capable).
//!
//! | Skill               | Risk | Autonomy Default   | Convergence Max |
//! |---------------------|------|--------------------|-----------------|
//! | `screenshot`        | Low  | Act Autonomously   | Level 4         |
//! | `accessibility_tree`| Low  | Act Autonomously   | Level 4         |
//! | `ocr_extract`       | Low  | Act Autonomously   | Level 4         |

pub mod accessibility_tree;
pub mod element;
pub mod ocr_extract;
pub mod screenshot;
