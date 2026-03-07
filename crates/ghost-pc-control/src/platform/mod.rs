//! Platform abstraction for input simulation, window management,
//! accessibility, and OCR.
//!
//! Provides traits that wrap platform-specific APIs behind testable
//! interfaces. Production code uses platform backends; tests use mocks.

pub mod accessibility_backend;
pub mod input_backend;
pub mod ocr_backend;
pub mod window_backend;

#[cfg(target_os = "macos")]
pub mod macos_window_backend;

#[cfg(target_os = "macos")]
pub mod macos_accessibility_backend;

#[cfg(target_os = "macos")]
pub mod macos_ocr_backend;
