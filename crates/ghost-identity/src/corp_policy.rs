//! CorpPolicyLoader — loads CORP_POLICY.md with Ed25519 signature verification (Req 24 AC3).

use std::path::Path;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CorpPolicyError {
    #[error("CORP_POLICY.md not found at {path}")]
    NotFound { path: String },
    #[error("failed to read CORP_POLICY.md: {0}")]
    ReadError(String),
    #[error("signature missing from CORP_POLICY.md")]
    SignatureMissing,
    #[error("signature verification failed")]
    SignatureInvalid,
}

/// A loaded and verified corporate policy document.
#[derive(Debug, Clone)]
pub struct CorpPolicyDocument {
    pub content: String,
    pub signature_verified: bool,
}

/// Loads and verifies CORP_POLICY.md.
pub struct CorpPolicyLoader;

impl CorpPolicyLoader {
    /// Load CORP_POLICY.md and verify its Ed25519 signature.
    ///
    /// The signature is expected as a `<!-- SIGNATURE: <hex> -->` comment
    /// at the end of the file. The verifying key is the platform key.
    pub fn load(
        path: &Path,
        verifying_key: &ghost_signing::VerifyingKey,
    ) -> Result<CorpPolicyDocument, CorpPolicyError> {
        if !path.exists() {
            return Err(CorpPolicyError::NotFound {
                path: path.display().to_string(),
            });
        }

        let raw = std::fs::read_to_string(path)
            .map_err(|e| CorpPolicyError::ReadError(e.to_string()))?;

        // Extract signature from trailing comment
        let (content, signature_hex) = extract_signature(&raw)?;

        // Decode signature
        let sig_bytes = hex_decode(&signature_hex)
            .map_err(|_| CorpPolicyError::SignatureInvalid)?;

        let signature = ghost_signing::Signature::from_bytes(&sig_bytes)
            .ok_or(CorpPolicyError::SignatureInvalid)?;

        // Verify
        if !ghost_signing::verify(content.as_bytes(), &signature, verifying_key) {
            return Err(CorpPolicyError::SignatureInvalid);
        }

        Ok(CorpPolicyDocument {
            content: content.to_string(),
            signature_verified: true,
        })
    }
}

fn extract_signature(raw: &str) -> Result<(&str, String), CorpPolicyError> {
    // Look for <!-- SIGNATURE: <hex> --> at the end
    let trimmed = raw.trim_end();
    if let Some(start) = trimmed.rfind("<!-- SIGNATURE:") {
        if let Some(end) = trimmed[start..].find("-->") {
            let sig_line = &trimmed[start..start + end + 3];
            let hex = sig_line
                .strip_prefix("<!-- SIGNATURE:")
                .and_then(|s| s.strip_suffix("-->"))
                .map(|s| s.trim().to_string())
                .ok_or(CorpPolicyError::SignatureMissing)?;
            let content = trimmed[..start].trim_end();
            return Ok((content, hex));
        }
    }
    Err(CorpPolicyError::SignatureMissing)
}

fn hex_decode(hex: &str) -> Result<Vec<u8>, ()> {
    if hex.len() % 2 != 0 {
        return Err(());
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| ()))
        .collect()
}
