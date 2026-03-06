//! Abstraction over OCR engines for testability.
//!
//! The `OcrBackend` trait provides a uniform interface for text extraction
//! from screenshot images. Platform-specific backends implement this trait.

use crate::perception::element::BoundingRect;

/// A text region detected by OCR.
#[derive(Debug, Clone)]
pub struct OcrTextRegion {
    pub text: String,
    pub bounds: BoundingRect,
    pub confidence: f64,
}

/// Trait abstracting OCR engines for testability.
pub trait OcrBackend: Send + Sync {
    /// Run OCR on the given RGBA image data and return detected text regions.
    fn extract_text(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<OcrTextRegion>, String>;
}

/// Stub backend for unsupported platforms. Returns an error.
pub struct StubOcrBackend;

impl OcrBackend for StubOcrBackend {
    fn extract_text(
        &self,
        _image_data: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<Vec<OcrTextRegion>, String> {
        Err("OCR not supported on this platform".into())
    }
}

/// Mock OCR backend for tests.
///
/// Returns pre-configured text regions regardless of input image.
pub struct MockOcrBackend {
    regions: Vec<OcrTextRegion>,
}

impl MockOcrBackend {
    pub fn new(regions: Vec<OcrTextRegion>) -> Self {
        Self { regions }
    }

    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
}

impl OcrBackend for MockOcrBackend {
    fn extract_text(
        &self,
        _image_data: &[u8],
        _width: u32,
        _height: u32,
    ) -> Result<Vec<OcrTextRegion>, String> {
        Ok(self.regions.clone())
    }
}
