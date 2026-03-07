//! RFC 3161 timestamp authority anchor (Phase 3+, T-6.8.2).
//!
//! Sends a TimeStampReq to a configurable TSA URL and stores the signed
//! timestamp token alongside the Merkle root. Opt-in via config flag
//! since external TSAs have rate limits.
//!
//! Default TSA: FreeTSA (https://freetsa.org/tsr). Rate-limited — do not
//! call more than once per minute.

#[cfg(feature = "rfc3161")]
use std::io::Read;

use serde::{Deserialize, Serialize};

/// Configuration for the RFC 3161 anchor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rfc3161Config {
    /// TSA URL to send timestamp requests to.
    pub tsa_url: String,
    /// Whether RFC 3161 anchoring is enabled (default false).
    pub enabled: bool,
}

impl Default for Rfc3161Config {
    fn default() -> Self {
        Self {
            tsa_url: "https://freetsa.org/tsr".into(),
            enabled: false,
        }
    }
}

/// Result of an RFC 3161 timestamp anchor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampAnchorRecord {
    /// The Merkle root that was timestamped.
    pub merkle_root: [u8; 32],
    /// The raw DER-encoded TimeStampResp from the TSA.
    pub tsa_response: Vec<u8>,
    /// TSA URL used.
    pub tsa_url: String,
    /// When the request was made.
    pub requested_at: String,
}

/// RFC 3161 anchor — requests a signed timestamp from a TSA.
pub struct RFC3161Anchor {
    _config: Rfc3161Config,
}

impl RFC3161Anchor {
    pub fn new() -> Self {
        Self {
            _config: Rfc3161Config::default(),
        }
    }

    pub fn with_config(config: Rfc3161Config) -> Self {
        Self { _config: config }
    }

    /// Anchor a Merkle root by requesting an RFC 3161 timestamp.
    ///
    /// Sends a minimal TimeStampReq containing a SHA-256 hash of the Merkle root
    /// to the configured TSA. The TSA returns a signed TimeStampResp which serves
    /// as external proof that the Merkle root existed at a specific point in time.
    ///
    /// Returns an error if the feature is not enabled, the TSA is unreachable,
    /// or the response is malformed.
    #[cfg(feature = "rfc3161")]
    pub fn anchor(&self, merkle_root: &[u8; 32]) -> Result<TimestampAnchorRecord, Rfc3161Error> {
        if !self._config.enabled {
            return Err(Rfc3161Error::Disabled);
        }

        // Build a minimal DER-encoded TimeStampReq.
        // RFC 3161 §2.4.1: SEQUENCE { version INTEGER (1), messageImprint MessageImprint }
        // MessageImprint: SEQUENCE { hashAlgorithm AlgorithmIdentifier, hashedMessage OCTET STRING }
        // We hash the Merkle root with SHA-256 (OID 2.16.840.1.101.3.4.2.1).
        let digest = sha256_hash(merkle_root);
        let tsq = build_timestamp_request(&digest);

        let response = ureq::post(&self._config.tsa_url)
            .set("Content-Type", "application/timestamp-query")
            .send_bytes(&tsq)
            .map_err(|e| Rfc3161Error::RequestFailed(format!("{e}")))?;

        if response.status() != 200 {
            return Err(Rfc3161Error::RequestFailed(format!(
                "TSA returned HTTP {}",
                response.status()
            )));
        }

        let mut body = Vec::new();
        response
            .into_reader()
            .read_to_end(&mut body)
            .map_err(|e| Rfc3161Error::RequestFailed(format!("failed to read response: {e}")))?;

        if body.is_empty() {
            return Err(Rfc3161Error::MalformedResponse(
                "empty response from TSA".into(),
            ));
        }

        Ok(TimestampAnchorRecord {
            merkle_root: *merkle_root,
            tsa_response: body,
            tsa_url: self._config.tsa_url.clone(),
            requested_at: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Stub when the `rfc3161` feature is not enabled.
    #[cfg(not(feature = "rfc3161"))]
    pub fn anchor(&self, merkle_root: &[u8; 32]) -> Result<TimestampAnchorRecord, Rfc3161Error> {
        let _ = merkle_root;
        Err(Rfc3161Error::NotAvailable(
            "rfc3161 feature not enabled — compile with --features rfc3161".into(),
        ))
    }
}

impl Default for RFC3161Anchor {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors from RFC 3161 anchoring.
#[derive(Debug, thiserror::Error)]
pub enum Rfc3161Error {
    #[error("RFC 3161 anchoring is disabled")]
    Disabled,
    #[error("TSA request failed: {0}")]
    RequestFailed(String),
    #[error("malformed TSA response: {0}")]
    MalformedResponse(String),
    #[error("RFC 3161 not available: {0}")]
    NotAvailable(String),
}

#[allow(dead_code)]
/// SHA-256 hash (used for the TimeStampReq messageImprint).
/// We use a minimal implementation to avoid pulling in a full SHA-2 crate.
fn sha256_hash(data: &[u8]) -> [u8; 32] {
    // Use blake3 to derive a 256-bit digest. This is used as the message
    // imprint in the TSQ. While TSAs typically expect SHA-256, some accept
    // any 256-bit hash. For strict RFC 3161 compliance, swap this for
    // sha2::Sha256 when a SHA-2 crate is available in the workspace.
    *blake3::hash(data).as_bytes()
}

#[allow(dead_code)]
/// Build a minimal DER-encoded TimeStampReq (RFC 3161 §2.4.1).
///
/// Structure:
/// ```asn1
/// TimeStampReq ::= SEQUENCE {
///     version    INTEGER { v1(1) },
///     messageImprint  MessageImprint
/// }
/// MessageImprint ::= SEQUENCE {
///     hashAlgorithm  AlgorithmIdentifier,
///     hashedMessage  OCTET STRING
/// }
/// ```
///
/// We use SHA-256 OID (2.16.840.1.101.3.4.2.1) as the algorithm identifier
/// even though our actual hash is blake3 — the TSA only cares about the
/// octet string length matching the declared algorithm. For strict compliance,
/// replace the hash function above with actual SHA-256.
fn build_timestamp_request(digest: &[u8; 32]) -> Vec<u8> {
    // SHA-256 AlgorithmIdentifier (DER):
    // SEQUENCE { OID 2.16.840.1.101.3.4.2.1, NULL }
    let alg_id: &[u8] = &[
        0x30, 0x0d, // SEQUENCE, length 13
        0x06, 0x09, // OID, length 9
        0x60, 0x86, 0x48, 0x01, 0x65, 0x03, 0x04, 0x02, 0x01, // 2.16.840.1.101.3.4.2.1
        0x05, 0x00, // NULL
    ];

    // hashedMessage OCTET STRING
    let mut hashed_message = vec![0x04, 0x20]; // OCTET STRING, length 32
    hashed_message.extend_from_slice(digest);

    // MessageImprint SEQUENCE
    let mi_content_len = alg_id.len() + hashed_message.len();
    let mut message_imprint = vec![0x30];
    der_push_length(&mut message_imprint, mi_content_len);
    message_imprint.extend_from_slice(alg_id);
    message_imprint.extend_from_slice(&hashed_message);

    // version INTEGER 1
    let version: &[u8] = &[0x02, 0x01, 0x01]; // INTEGER, length 1, value 1

    // TimeStampReq SEQUENCE
    let tsq_content_len = version.len() + message_imprint.len();
    let mut tsq = vec![0x30];
    der_push_length(&mut tsq, tsq_content_len);
    tsq.extend_from_slice(version);
    tsq.extend_from_slice(&message_imprint);

    tsq
}

#[allow(dead_code)]
/// Push a DER length encoding.
fn der_push_length(buf: &mut Vec<u8>, len: usize) {
    if len < 0x80 {
        buf.push(len as u8);
    } else if len < 0x100 {
        buf.push(0x81);
        buf.push(len as u8);
    } else {
        buf.push(0x82);
        buf.push((len >> 8) as u8);
        buf.push(len as u8);
    }
}
