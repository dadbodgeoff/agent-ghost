//! macOS OCR backend using the Vision framework via Swift CLI.
//!
//! Writes the screenshot to a temp PNG file, runs a Swift snippet that
//! uses `VNRecognizeTextRequest` for OCR, then parses the JSON output.
//!
//! Requires macOS 10.15+ (Catalina) for Vision framework support.

#![cfg(target_os = "macos")]

use std::io::Write;
use std::process::Command;

use crate::perception::element::BoundingRect;
use crate::platform::ocr_backend::{OcrBackend, OcrTextRegion};

pub struct MacOsOcrBackend;

impl MacOsOcrBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MacOsOcrBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl OcrBackend for MacOsOcrBackend {
    fn extract_text(
        &self,
        image_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Vec<OcrTextRegion>, String> {
        // Write RGBA data to a temp PNG file.
        let tmp_path = format!("/tmp/ghost-ocr-{}.png", uuid::Uuid::now_v7());
        write_rgba_as_png(image_data, width, height, &tmp_path)?;

        // Run Swift OCR snippet.
        let result = run_swift_ocr(&tmp_path, width, height);

        // Clean up temp file.
        let _ = std::fs::remove_file(&tmp_path);

        result
    }
}

/// Write raw RGBA data as a minimal PNG file.
fn write_rgba_as_png(data: &[u8], width: u32, height: u32, path: &str) -> Result<(), String> {
    // Use sips to convert raw data, or write a minimal PPM and convert.
    // Simplest approach: write raw RGBA to a temp file, then use sips.
    let raw_path = format!("{path}.raw");
    let mut file =
        std::fs::File::create(&raw_path).map_err(|e| format!("failed to create temp file: {e}"))?;
    file.write_all(data)
        .map_err(|e| format!("failed to write image data: {e}"))?;
    drop(file);

    // Use Python to convert raw RGBA to PNG (available on all macOS).
    let script = format!(
        r#"
import struct, zlib
width, height = {width}, {height}
with open('{raw_path}', 'rb') as f:
    rgba = f.read()
# Build minimal PNG
def make_png(w, h, rgba_data):
    def chunk(tag, data):
        c = tag + data
        crc = struct.pack('>I', zlib.crc32(c) & 0xffffffff)
        return struct.pack('>I', len(data)) + c + crc
    sig = b'\x89PNG\r\n\x1a\n'
    ihdr = chunk(b'IHDR', struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0))
    raw_rows = b''
    for y in range(h):
        raw_rows += b'\x00'
        raw_rows += rgba_data[y*w*4:(y+1)*w*4]
    idat = chunk(b'IDAT', zlib.compress(raw_rows))
    iend = chunk(b'IEND', b'')
    return sig + ihdr + idat + iend
png_data = make_png(width, height, rgba)
with open('{path}', 'wb') as f:
    f.write(png_data)
"#
    );

    let status = Command::new("python3")
        .arg("-c")
        .arg(&script)
        .status()
        .map_err(|e| format!("python3 failed: {e}"))?;

    let _ = std::fs::remove_file(&raw_path);

    if !status.success() {
        return Err("failed to convert RGBA to PNG".into());
    }

    Ok(())
}

/// Run OCR on a PNG file using macOS Vision framework via Swift.
fn run_swift_ocr(
    png_path: &str,
    img_width: u32,
    img_height: u32,
) -> Result<Vec<OcrTextRegion>, String> {
    let swift_code = format!(
        r#"
import Foundation
import Vision
import AppKit

let imagePath = "{png_path}"
guard let image = NSImage(contentsOfFile: imagePath),
      let cgImage = image.cgImage(forProposedRect: nil, context: nil, hints: nil) else {{
    print("[]")
    exit(0)
}}

let imgWidth = Double({img_width})
let imgHeight = Double({img_height})

let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.usesLanguageCorrection = true

let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
try? handler.perform([request])

var results: [[String: Any]] = []
if let observations = request.results {{
    for obs in observations {{
        let text = obs.topCandidates(1).first?.string ?? ""
        let confidence = Double(obs.confidence)
        let box = obs.boundingBox
        let x = Int(box.origin.x * imgWidth)
        let y = Int((1.0 - box.origin.y - box.size.height) * imgHeight)
        let w = Int(box.size.width * imgWidth)
        let h = Int(box.size.height * imgHeight)
        results.append(["text": text, "confidence": confidence, "x": x, "y": y, "width": w, "height": h])
    }}
}}

if let jsonData = try? JSONSerialization.data(withJSONObject: results),
   let jsonString = String(data: jsonData, encoding: .utf8) {{
    print(jsonString)
}} else {{
    print("[]")
}}
"#
    );

    let output = Command::new("swift")
        .arg("-e")
        .arg(&swift_code)
        .output()
        .map_err(|e| format!("swift OCR failed to execute: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("swift OCR error: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_ocr_json(&stdout)
}

/// Parse the JSON output from the Swift OCR script.
fn parse_ocr_json(json_str: &str) -> Result<Vec<OcrTextRegion>, String> {
    let arr: Vec<serde_json::Value> =
        serde_json::from_str(json_str).map_err(|e| format!("failed to parse OCR JSON: {e}"))?;

    let mut regions = Vec::new();
    for item in &arr {
        let text = item
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if text.is_empty() {
            continue;
        }

        regions.push(OcrTextRegion {
            text,
            bounds: BoundingRect {
                x: item.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                y: item.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32,
                width: item.get("width").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                height: item.get("height").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            },
            confidence: item
                .get("confidence")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        });
    }

    Ok(regions)
}
