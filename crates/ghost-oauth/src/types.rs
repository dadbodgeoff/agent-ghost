//! Core OAuth types: TokenSet, PkceChallenge, OAuthRefId, ApiRequest, ApiResponse, ProviderConfig.
//!
//! All tokens wrapped in `SecretString` (zeroized on drop). `BTreeMap` for headers
//! (deterministic ordering for signed payloads).

use std::collections::BTreeMap;
use std::fmt;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::Rng;
use secrecy::{ExposeSecret, SecretString};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// OAuthRefId — opaque reference the agent uses instead of raw tokens
// ---------------------------------------------------------------------------

/// Opaque reference ID (UUID) that the agent uses instead of raw tokens.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OAuthRefId(Uuid);

impl OAuthRefId {
    /// Create a new random ref ID.
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    /// Reconstruct from a UUID.
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    /// The inner UUID.
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

impl Default for OAuthRefId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OAuthRefId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// PkceChallenge — PKCE S256 challenge/verifier pair
// ---------------------------------------------------------------------------

/// PKCE challenge/verifier pair for OAuth 2.0 Authorization Code + PKCE flow.
///
/// `code_verifier` is a cryptographically random URL-safe string (43-128 chars).
/// `code_challenge` is `BASE64URL(SHA256(code_verifier))`.
#[derive(Clone)]
pub struct PkceChallenge {
    /// High-entropy random verifier (zeroized on drop).
    pub code_verifier: SecretString,
    /// SHA-256 hash of verifier, base64url-encoded (sent to authorization server).
    pub code_challenge: String,
    /// Always "S256".
    pub method: String,
}

impl PkceChallenge {
    /// Generate a new PKCE challenge with a 128-char code_verifier.
    pub fn generate() -> Self {
        let verifier = Self::random_verifier(128);
        let challenge = Self::compute_challenge(verifier.expose_secret());
        Self {
            code_verifier: verifier,
            code_challenge: challenge,
            method: "S256".to_string(),
        }
    }

    /// Compute `BASE64URL(SHA256(verifier))`.
    pub fn compute_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        URL_SAFE_NO_PAD.encode(hash)
    }

    /// Generate a cryptographically random URL-safe verifier of `len` characters.
    fn random_verifier(len: usize) -> SecretString {
        const CHARSET: &[u8] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let mut rng = rand::thread_rng();
        let verifier: String = (0..len)
            .map(|_| {
                let idx = rng.gen_range(0..CHARSET.len());
                CHARSET[idx] as char
            })
            .collect();
        SecretString::from(verifier)
    }
}

impl fmt::Debug for PkceChallenge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PkceChallenge")
            .field("code_verifier", &"[REDACTED]")
            .field("code_challenge", &self.code_challenge)
            .field("method", &self.method)
            .finish()
    }
}

// ---------------------------------------------------------------------------
// TokenSet — access + refresh tokens with expiry and scopes
// ---------------------------------------------------------------------------

/// A set of OAuth tokens obtained from a provider.
///
/// Tokens are `SecretString` (zeroized on drop). For serialization to disk,
/// use the custom serde impl that redacts in Debug but encrypts for storage.
#[derive(Clone)]
pub struct TokenSet {
    pub access_token: SecretString,
    pub refresh_token: Option<SecretString>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}

impl TokenSet {
    /// Check if the access token has expired.
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"[REDACTED]")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at", &self.expires_at)
            .field("scopes", &self.scopes)
            .finish()
    }
}

/// Serializable form of TokenSet for encrypted storage.
#[derive(Serialize, Deserialize)]
pub(crate) struct TokenSetSerde {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub scopes: Vec<String>,
}

impl From<&TokenSet> for TokenSetSerde {
    fn from(ts: &TokenSet) -> Self {
        Self {
            access_token: ts.access_token.expose_secret().to_string(),
            refresh_token: ts
                .refresh_token
                .as_ref()
                .map(|r| r.expose_secret().to_string()),
            expires_at: ts.expires_at,
            scopes: ts.scopes.clone(),
        }
    }
}

impl From<TokenSetSerde> for TokenSet {
    fn from(s: TokenSetSerde) -> Self {
        Self {
            access_token: SecretString::from(s.access_token),
            refresh_token: s.refresh_token.map(SecretString::from),
            expires_at: s.expires_at,
            scopes: s.scopes,
        }
    }
}

// ---------------------------------------------------------------------------
// ApiRequest / ApiResponse — broker-mediated HTTP calls
// ---------------------------------------------------------------------------

/// An API request the agent wants to make via the OAuth broker.
///
/// The agent provides method, URL, headers, and body. The broker injects
/// the Bearer token and executes the request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRequest {
    pub method: String,
    pub url: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<String>,
}

/// Response from a broker-mediated API call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse {
    pub status: u16,
    pub headers: BTreeMap<String, String>,
    pub body: String,
}

// ---------------------------------------------------------------------------
// ProviderConfig — OAuth provider registration
// ---------------------------------------------------------------------------

/// Configuration for an OAuth provider (stored in ghost.yml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// OAuth client ID (public).
    pub client_id: String,
    /// Key name in SecretProvider for the client secret.
    pub client_secret_key: String,
    /// Authorization endpoint URL.
    pub auth_url: String,
    /// Token exchange endpoint URL.
    pub token_url: String,
    /// Token revocation endpoint URL (optional — not all providers support it).
    pub revoke_url: Option<String>,
    /// Available scope groups: e.g. {"email": ["gmail.readonly"], "calendar": ["calendar"]}
    pub scopes: BTreeMap<String, Vec<String>>,
}

// ---------------------------------------------------------------------------
// ConnectionInfo — agent-visible connection metadata (no tokens)
// ---------------------------------------------------------------------------

/// Agent-visible metadata about an OAuth connection (no raw tokens).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub ref_id: OAuthRefId,
    pub provider: String,
    pub scopes: Vec<String>,
    pub connected_at: DateTime<Utc>,
    pub status: ConnectionStatus,
}

/// Status of an OAuth connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Expired,
    Revoked,
    Error,
}
