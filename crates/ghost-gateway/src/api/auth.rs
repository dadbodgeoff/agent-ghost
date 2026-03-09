//! Dual-mode REST authentication middleware and JWT auth endpoints.
//!
//! Supports three modes (checked in order):
//! 1. JWT mode: `GHOST_JWT_SECRET` set → validate Bearer as JWT
//! 2. Legacy token mode: `GHOST_TOKEN` set → plain string match
//! 3. No-auth mode: neither set → allow all (local dev)
//!
//! Ref: ADE_DESIGN_PLAN §5.0.6, tasks.md T-1.1.1, T-1.1.3

use std::collections::HashSet;
use std::sync::{Arc, RwLock};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::authz::{AUTHZ_CLAIMS_VERSION_V1, INTERNAL_JWT_ISSUER};

/// JWT claims extracted from a validated token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or agent ID).
    pub sub: String,
    /// Role: "admin", "operator", "viewer".
    pub role: String,
    /// Optional typed capabilities for authz v1 tokens.
    #[serde(default)]
    pub capabilities: Vec<String>,
    /// Optional version marker for typed authz claims.
    #[serde(default)]
    pub authz_v: Option<u16>,
    /// Expiration (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// JWT ID (for revocation).
    pub jti: String,
    /// Optional issuer for typed authz claims.
    #[serde(default)]
    pub iss: Option<String>,
}

impl Claims {
    /// Fallback claims for legacy token mode — implicit admin.
    pub fn admin_fallback() -> Self {
        Self {
            sub: "legacy-token-user".into(),
            role: "admin".into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: u64::MAX,
            iat: 0,
            jti: String::new(),
            iss: None,
        }
    }

    /// Fallback claims for no-auth mode — dev role only.
    ///
    /// T-5.1.1: Never assign admin role in no-auth mode. Dev role grants
    /// read-all + write-non-safety. Safety endpoints always require proper auth.
    pub fn no_auth_fallback() -> Self {
        Self {
            sub: "anonymous".into(),
            role: "dev".into(),
            capabilities: Vec::new(),
            authz_v: None,
            exp: u64::MAX,
            iat: 0,
            jti: String::new(),
            iss: None,
        }
    }
}

/// Persistent JWT revocation set backed by SQLite (WP0-B).
///
/// Write-through: every `revoke()` call writes to both the in-memory set
/// and the `revoked_tokens` DB table. On startup, `load_from_db()` hydrates
/// the in-memory set from the DB so revocations survive restarts.
///
/// Uses a dedicated read-write `Connection` (not `DbPool::read()` which is
/// read-only) to ensure INSERT operations succeed.
pub struct RevocationSet {
    revoked: RwLock<HashSet<String>>,
    /// Dedicated read-write connection for write-through persistence.
    /// Using a standalone connection (not DbPool::read()) because read pool
    /// connections are SQLITE_OPEN_READ_ONLY and cannot execute INSERT.
    db_writer: RwLock<Option<Arc<std::sync::Mutex<rusqlite::Connection>>>>,
}

impl std::fmt::Debug for RevocationSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let count = self.revoked.read().map(|s| s.len()).unwrap_or(0);
        f.debug_struct("RevocationSet")
            .field("revoked_count", &count)
            .field(
                "db_attached",
                &self.db_writer.read().map(|d| d.is_some()).unwrap_or(false),
            )
            .finish()
    }
}

impl Default for RevocationSet {
    fn default() -> Self {
        Self {
            revoked: RwLock::new(HashSet::new()),
            db_writer: RwLock::new(None),
        }
    }
}

impl RevocationSet {
    pub fn new() -> Self {
        Self::default()
    }

    /// Attach a dedicated read-write DB connection for persistent revocation storage.
    /// Call after DB pool is created during bootstrap.
    pub fn set_db(&self, db: &Arc<crate::db_pool::DbPool>) {
        match db.legacy_connection() {
            Ok(conn) => {
                if let Ok(mut slot) = self.db_writer.write() {
                    *slot = Some(conn);
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Failed to create write connection for RevocationSet — revocations will NOT persist across restarts");
            }
        }
    }

    /// Load active (non-expired) revocations from DB into the in-memory set.
    /// Call once during bootstrap after migrations have run.
    pub fn load_from_db(&self, conn: &rusqlite::Connection) {
        match cortex_storage::queries::revoked_token_queries::load_active_revocations(conn) {
            Ok(jtis) => {
                let count = jtis.len();
                if let Ok(mut set) = self.revoked.write() {
                    for jti in jtis {
                        set.insert(jti);
                    }
                }
                if count > 0 {
                    tracing::info!(count, "Loaded revoked tokens from DB");
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to load revoked tokens from DB — starting with empty set");
            }
        }
        // Also clean up expired entries.
        match cortex_storage::queries::revoked_token_queries::cleanup_expired(conn) {
            Ok(n) if n > 0 => tracing::debug!(deleted = n, "Cleaned up expired revoked tokens"),
            _ => {}
        }
    }

    /// Revoke a token by JTI. Writes through to DB if available.
    /// `expires_at` is the token's expiration as an ISO 8601 string.
    pub fn revoke_with_expiry(&self, jti: &str, expires_at: &str) {
        if let Ok(mut set) = self.revoked.write() {
            set.insert(jti.to_string());
        }
        // Write-through to DB via dedicated read-write connection.
        if let Ok(db_guard) = self.db_writer.read() {
            if let Some(ref conn_mutex) = *db_guard {
                match conn_mutex.lock() {
                    Ok(conn) => {
                        if let Err(e) = cortex_storage::queries::revoked_token_queries::revoke_token(
                            &conn, jti, expires_at,
                        ) {
                            tracing::warn!(jti, error = %e, "Failed to persist token revocation to DB");
                        }
                    }
                    Err(e) => {
                        tracing::warn!(jti, error = %e, "Failed to acquire DB lock for token revocation");
                    }
                }
            }
        }
    }

    /// Revoke a token by JTI (legacy API without expiry — uses far-future default).
    pub fn revoke(&self, jti: &str) {
        self.revoke_with_expiry(jti, "9999-12-31T23:59:59Z");
    }

    pub fn is_revoked(&self, jti: &str) -> bool {
        self.revoked
            .read()
            .map(|set| set.contains(jti))
            .unwrap_or(false)
    }
}

/// Configuration resolved once from environment variables at startup.
/// Injected into middleware via Extension to avoid per-request env reads.
pub struct AuthConfig {
    pub jwt_secret: Option<String>,
    pub legacy_token: Option<String>,
    /// Whether GHOST_ENV=production.
    pub is_production: bool,
}

impl AuthConfig {
    /// Read auth configuration from environment (call once at startup).
    pub fn from_env() -> Self {
        let ghost_env = std::env::var("GHOST_ENV").unwrap_or_default();
        let is_production = ghost_env.eq_ignore_ascii_case("production");
        Self {
            jwt_secret: std::env::var("GHOST_JWT_SECRET").ok(),
            legacy_token: std::env::var("GHOST_TOKEN").ok(),
            is_production,
        }
    }

    /// Whether any authentication is configured.
    pub fn auth_required(&self) -> bool {
        self.jwt_secret.is_some() || self.legacy_token.is_some()
    }

    /// T-5.1.1: Validate that production environments have auth configured.
    /// Call during bootstrap — exits with fatal error if production has no auth.
    pub fn validate_production(&self) {
        if self.is_production && !self.auth_required() {
            eprintln!(
                "FATAL: GHOST_ENV=production but no authentication configured. \
                 Set GHOST_JWT_SECRET or GHOST_TOKEN before running in production."
            );
            std::process::exit(1);
        }
    }
}

/// T-5.1.1: Check if a path is a safety-critical endpoint that requires auth
/// even in dev/no-auth mode. Kill/pause/resume/quarantine are irreversible.
fn is_safety_endpoint(path: &str) -> bool {
    path.starts_with("/api/safety/")
}

/// Extract the Bearer token from an Authorization header value.
fn extract_bearer(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}

/// Decode and validate a JWT token.
fn decode_jwt(token: &str, secret: &str) -> Result<Claims, String> {
    let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = true;
    validation.leeway = 30; // 30s clock skew tolerance

    jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("JWT decode error: {e}"))
}

/// Decode a JWT without validating expiration (for refresh flow).
fn decode_jwt_allow_expired(token: &str, secret: &str) -> Result<Claims, String> {
    let key = jsonwebtoken::DecodingKey::from_secret(secret.as_bytes());
    let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
    validation.validate_exp = false;

    jsonwebtoken::decode::<Claims>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|e| format!("JWT decode error: {e}"))
}

/// Encode a new JWT token.
fn encode_jwt(claims: &Claims, secret: &str) -> Result<String, String> {
    let key = jsonwebtoken::EncodingKey::from_secret(secret.as_bytes());
    jsonwebtoken::encode(&jsonwebtoken::Header::default(), claims, &key)
        .map_err(|e| format!("JWT encode error: {e}"))
}

/// Constant-time string comparison to prevent timing attacks (T-5.2.4).
///
/// Compares all bytes regardless of length difference to avoid leaking
/// the token length via timing side-channel.
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    // T-5.2.4: Always compare the same number of bytes to avoid length leak.
    // XOR-fold both strings against the longer length, using 0 padding.
    let max_len = a_bytes.len().max(b_bytes.len());
    let mut diff = (a_bytes.len() != b_bytes.len()) as u8; // length mismatch contributes to diff
    for i in 0..max_len {
        let x = if i < a_bytes.len() { a_bytes[i] } else { 0 };
        let y = if i < b_bytes.len() { b_bytes[i] } else { 0 };
        diff |= x ^ y;
    }
    diff == 0
}

fn cookie_security_attrs(secure: bool) -> &'static str {
    if secure {
        "; Secure"
    } else {
        ""
    }
}

/// Build a `Set-Cookie` header value for the refresh token.
fn build_refresh_cookie(token: &str, max_age_secs: u64, secure: bool) -> String {
    format!(
        "ghost_refresh={}; HttpOnly{}; SameSite=Strict; Path=/api/auth; Max-Age={}",
        token,
        cookie_security_attrs(secure),
        max_age_secs
    )
}

/// Build a `Set-Cookie` header that clears the refresh cookie.
fn build_clear_refresh_cookie(secure: bool) -> String {
    format!(
        "ghost_refresh=; HttpOnly{}; SameSite=Strict; Path=/api/auth; Max-Age=0",
        cookie_security_attrs(secure)
    )
}

/// Extract refresh token from Cookie header.
fn extract_refresh_cookie(request: &Request<Body>) -> Option<String> {
    request
        .headers()
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|cookies| {
            cookies.split(';').find_map(|cookie| {
                let cookie = cookie.trim();
                cookie.strip_prefix("ghost_refresh=").map(|v| v.to_string())
            })
        })
}

/// Tower middleware: dual-mode authentication for all `/api/*` routes.
///
/// Reads `AuthConfig` and `RevocationSet` from Extensions (injected at startup).
///
/// Skips auth for:
/// - `GET /api/health`, `GET /api/ready`, and `GET /api/compatibility` (bootstrap probes)
/// - `POST /api/auth/login` (login endpoint itself)
/// - `POST /api/auth/refresh` (token refresh — uses cookie, not Bearer)
/// - `GET /api/oauth/callback` (browser redirect target from external OAuth provider)
/// - `GET /api/ws` (WebSocket has its own auth in the handler)
/// - `GET /.well-known/agent.json` and `POST /a2a` (mesh has Ed25519 auth)
pub async fn auth_middleware(request: Request<Body>, next: Next) -> Response {
    let path = request.uri().path();

    // Skip auth for public endpoints.
    let skip_auth = matches!(
        path,
        "/api/health"
            | "/api/ready"
            | "/api/compatibility"
            | "/api/auth/login"
            | "/api/auth/refresh"
            | "/api/auth/logout"
            | "/api/oauth/callback"
            | "/api/openapi.json"
            | "/api/ws"
            | "/.well-known/agent.json"
            | "/a2a"
    );

    if skip_auth {
        return next.run(request).await;
    }

    // Read cached AuthConfig from extensions (set once at startup in bootstrap).
    let config = match request.extensions().get::<Arc<AuthConfig>>() {
        Some(c) => c.clone(),
        None => {
            // T-5.1.1: Safety endpoints MUST reject without proper auth.
            if is_safety_endpoint(path) {
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": {
                            "code": "AUTH_REQUIRED",
                            "message": "Safety-critical endpoints require authentication"
                        }
                    })),
                )
                    .into_response();
            }
            // Fallback: no config injected — treat as no-auth dev mode.
            tracing::warn!(
                path,
                "Auth disabled — no AuthConfig injected. Granting dev role."
            );
            let mut request = request;
            request.extensions_mut().insert(Claims::no_auth_fallback());
            return next.run(request).await;
        }
    };

    // No auth configured → local dev mode (T-5.1.1).
    if !config.auth_required() {
        // T-5.1.1: In production, this should never happen (validate_production exits at startup).
        // Defense-in-depth: reject production requests without auth.
        if config.is_production {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": {
                        "code": "AUTH_NOT_CONFIGURED",
                        "message": "Authentication not configured in production mode"
                    }
                })),
            )
                .into_response();
        }

        // T-5.1.1: Safety endpoints MUST reject without proper auth even in dev.
        if is_safety_endpoint(path) {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "AUTH_REQUIRED",
                        "message": "Safety-critical endpoints require authentication even in dev mode"
                    }
                })),
            )
                .into_response();
        }

        tracing::warn!(
            path,
            "Auth disabled — no auth configured. Granting dev role."
        );
        let mut request = request;
        request.extensions_mut().insert(Claims::no_auth_fallback());
        return next.run(request).await;
    }

    // Extract Bearer token from Authorization header.
    let bearer = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(extract_bearer);

    let bearer = match bearer {
        Some(b) => b.to_string(),
        None => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "MISSING_TOKEN",
                        "message": "Authorization header with Bearer token required"
                    }
                })),
            )
                .into_response();
        }
    };

    // Try JWT mode first.
    if let Some(ref secret) = config.jwt_secret {
        match decode_jwt(&bearer, secret) {
            Ok(claims) => {
                // Check revocation set before accepting the token.
                if !claims.jti.is_empty() {
                    if let Some(revocation) = request.extensions().get::<Arc<RevocationSet>>() {
                        if revocation.is_revoked(&claims.jti) {
                            return (
                                StatusCode::UNAUTHORIZED,
                                Json(serde_json::json!({
                                    "error": {
                                        "code": "TOKEN_REVOKED",
                                        "message": "Token has been revoked"
                                    }
                                })),
                            )
                                .into_response();
                        }
                    }
                }
                let mut request = request;
                request.extensions_mut().insert(claims);
                return next.run(request).await;
            }
            Err(e) => {
                tracing::debug!(error = %e, "JWT validation failed");
                return (
                    StatusCode::UNAUTHORIZED,
                    Json(serde_json::json!({
                        "error": {
                            "code": "INVALID_TOKEN",
                            "message": "Invalid or expired JWT token"
                        }
                    })),
                )
                    .into_response();
            }
        }
    }

    // Legacy token mode.
    if let Some(ref expected) = config.legacy_token {
        if constant_time_eq(&bearer, expected) {
            let mut request = request;
            request.extensions_mut().insert(Claims::admin_fallback());
            return next.run(request).await;
        }
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "INVALID_TOKEN",
                    "message": "Invalid bearer token"
                }
            })),
        )
            .into_response();
    }

    // Should not reach here if auth_required() is true, but be safe.
    (StatusCode::UNAUTHORIZED, "Unauthorized").into_response()
}

// ─── JWT Auth Endpoints ─────────────────────────────────────────────────────

/// Login request body.
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    /// For JWT mode: the pre-shared credential (GHOST_TOKEN value or GHOST_JWT_SECRET).
    /// For legacy mode: the GHOST_TOKEN value.
    pub token: String,
}

/// Login response with access token.
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Authenticated session summary resolved by the auth middleware.
#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub authenticated: bool,
    pub subject: String,
    pub role: String,
    pub mode: &'static str,
}

/// Refresh token TTL: 7 days in seconds.
const REFRESH_TOKEN_TTL: u64 = 7 * 24 * 60 * 60;
/// Access token TTL: 15 minutes in seconds.
const ACCESS_TOKEN_TTL: u64 = 900;

/// POST /api/auth/login — authenticate and issue JWT (or validate legacy token).
///
/// In JWT mode, also sets an httpOnly refresh token cookie (7d).
pub async fn login(
    axum::Extension(config): axum::Extension<Arc<AuthConfig>>,
    Json(body): Json<LoginRequest>,
) -> Response {
    // JWT mode: validate the provided credential and issue access + refresh tokens.
    if let Some(ref secret) = config.jwt_secret {
        // Validate credential against GHOST_TOKEN if set, otherwise against
        // GHOST_JWT_SECRET itself (supports JWT-only deployments without GHOST_TOKEN).
        let credential_valid = if let Some(ref expected) = config.legacy_token {
            constant_time_eq(&body.token, expected)
        } else {
            // JWT-only mode: credential is the JWT secret itself.
            constant_time_eq(&body.token, secret)
        };

        if !credential_valid {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "INVALID_CREDENTIALS",
                        "message": "Invalid credentials"
                    }
                })),
            )
                .into_response();
        }

        let now = chrono::Utc::now().timestamp() as u64;

        // Issue access token (15min).
        let access_claims = Claims {
            sub: "admin".into(),
            role: "admin".into(),
            capabilities: Vec::new(),
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: now + ACCESS_TOKEN_TTL,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        };

        // Issue refresh token (7d).
        let refresh_claims = Claims {
            sub: "admin".into(),
            role: "admin".into(),
            capabilities: Vec::new(),
            authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
            exp: now + REFRESH_TOKEN_TTL,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
            iss: Some(INTERNAL_JWT_ISSUER.into()),
        };

        let access_token = match encode_jwt(&access_claims, secret) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "Failed to encode access JWT");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        let refresh_token = match encode_jwt(&refresh_claims, secret) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!(error = %e, "Failed to encode refresh JWT");
                return StatusCode::INTERNAL_SERVER_ERROR.into_response();
            }
        };

        let mut response = (
            StatusCode::OK,
            Json(LoginResponse {
                access_token,
                token_type: "Bearer".into(),
                expires_in: ACCESS_TOKEN_TTL,
            }),
        )
            .into_response();

        // Set httpOnly refresh cookie.
        if let Ok(val) = axum::http::HeaderValue::from_str(&build_refresh_cookie(
            &refresh_token,
            REFRESH_TOKEN_TTL,
            config.is_production,
        )) {
            response.headers_mut().insert("set-cookie", val);
        }

        return response;
    }

    // Legacy token mode: validate the token directly.
    if let Some(ref expected) = config.legacy_token {
        if constant_time_eq(&body.token, expected) {
            return (
                StatusCode::OK,
                Json(LoginResponse {
                    access_token: body.token.clone(),
                    token_type: "Bearer".into(),
                    expires_in: u64::MAX,
                }),
            )
                .into_response();
        }
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "INVALID_CREDENTIALS",
                    "message": "Invalid token"
                }
            })),
        )
            .into_response();
    }

    // No auth configured — always succeed.
    (
        StatusCode::OK,
        Json(LoginResponse {
            access_token: String::new(),
            token_type: "Bearer".into(),
            expires_in: u64::MAX,
        }),
    )
        .into_response()
}

/// POST /api/auth/refresh — refresh access token using httpOnly refresh cookie.
///
/// Validates the refresh token from the `ghost_refresh` cookie, checks revocation,
/// then issues a new access token + rotated refresh cookie.
pub async fn refresh(
    axum::Extension(config): axum::Extension<Arc<AuthConfig>>,
    axum::Extension(revocation_set): axum::Extension<Arc<RevocationSet>>,
    request: Request<Body>,
) -> Response {
    let Some(ref secret) = config.jwt_secret else {
        // No JWT mode — refresh is a no-op in legacy mode.
        return (
            StatusCode::OK,
            Json(serde_json::json!({"message": "No JWT mode configured, refresh not needed"})),
        )
            .into_response();
    };

    // Extract refresh token from cookie (preferred) or Authorization header (fallback).
    let refresh_token = extract_refresh_cookie(&request).or_else(|| {
        request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(extract_bearer)
            .map(|s| s.to_string())
    });

    let Some(refresh_token) = refresh_token else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "MISSING_TOKEN",
                    "message": "Refresh token cookie or Authorization header required"
                }
            })),
        )
            .into_response();
    };

    // Decode refresh token (allow expired within a grace period for clock skew).
    let old_claims = match decode_jwt_allow_expired(&refresh_token, secret) {
        Ok(c) => c,
        Err(_) => {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({
                    "error": {
                        "code": "INVALID_TOKEN",
                        "message": "Invalid refresh token"
                    }
                })),
            )
                .into_response();
        }
    };

    // Check if the refresh token's jti has been revoked.
    if !old_claims.jti.is_empty() && revocation_set.is_revoked(&old_claims.jti) {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": {
                    "code": "TOKEN_REVOKED",
                    "message": "Refresh token has been revoked"
                }
            })),
        )
            .into_response();
    }

    // Revoke the old refresh token (rotation — each refresh token is single-use).
    if !old_claims.jti.is_empty() {
        let expires_at = chrono::DateTime::from_timestamp(old_claims.exp as i64, 0)
            .unwrap_or_else(chrono::Utc::now)
            .to_rfc3339();
        revocation_set.revoke_with_expiry(&old_claims.jti, &expires_at);
    }

    let now = chrono::Utc::now().timestamp() as u64;

    // Issue new access token.
    let access_claims = Claims {
        sub: old_claims.sub.clone(),
        role: old_claims.role.clone(),
        capabilities: old_claims.capabilities.clone(),
        authz_v: old_claims.authz_v.or(Some(AUTHZ_CLAIMS_VERSION_V1)),
        exp: now + ACCESS_TOKEN_TTL,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
        iss: old_claims
            .iss
            .clone()
            .or_else(|| Some(INTERNAL_JWT_ISSUER.into())),
    };

    // Issue new refresh token (rotation).
    let refresh_claims = Claims {
        sub: old_claims.sub,
        role: old_claims.role,
        capabilities: old_claims.capabilities,
        authz_v: old_claims.authz_v.or(Some(AUTHZ_CLAIMS_VERSION_V1)),
        exp: now + REFRESH_TOKEN_TTL,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
        iss: old_claims.iss.or_else(|| Some(INTERNAL_JWT_ISSUER.into())),
    };

    let access_token = match encode_jwt(&access_claims, secret) {
        Ok(t) => t,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let new_refresh_token = match encode_jwt(&refresh_claims, secret) {
        Ok(t) => t,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    let mut response = (
        StatusCode::OK,
        Json(LoginResponse {
            access_token,
            token_type: "Bearer".into(),
            expires_in: ACCESS_TOKEN_TTL,
        }),
    )
        .into_response();

    // Set rotated refresh cookie.
    if let Ok(val) = axum::http::HeaderValue::from_str(&build_refresh_cookie(
        &new_refresh_token,
        REFRESH_TOKEN_TTL,
        config.is_production,
    )) {
        response.headers_mut().insert("set-cookie", val);
    }

    response
}

/// GET /api/auth/session — return the authenticated session resolved by middleware.
pub async fn session(
    axum::Extension(config): axum::Extension<Arc<AuthConfig>>,
    claims: axum::Extension<Claims>,
) -> impl IntoResponse {
    let mode = if config.jwt_secret.is_some() {
        "jwt"
    } else if config.legacy_token.is_some() {
        "legacy"
    } else {
        "none"
    };

    Json(SessionResponse {
        authenticated: true,
        subject: claims.sub.clone(),
        role: claims.role.clone(),
        mode,
    })
}

/// POST /api/auth/logout — revoke the current token's jti and clear refresh cookie.
pub async fn logout(
    axum::Extension(config): axum::Extension<Arc<AuthConfig>>,
    axum::Extension(revocation_set): axum::Extension<Arc<RevocationSet>>,
    request: Request<Body>,
) -> Response {
    // Revoke access token jti if present.
    if let Some(ref secret) = config.jwt_secret {
        if let Some(bearer) = request
            .headers()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(extract_bearer)
        {
            if let Ok(claims) = decode_jwt_allow_expired(bearer, secret) {
                if !claims.jti.is_empty() {
                    let expires_at = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339();
                    revocation_set.revoke_with_expiry(&claims.jti, &expires_at);
                    tracing::info!(jti = %claims.jti, sub = %claims.sub, "Token revoked via logout");
                }
            }
        }

        // Revoke refresh token from cookie if present.
        if let Some(refresh_token) = extract_refresh_cookie(&request) {
            if let Ok(claims) = decode_jwt_allow_expired(&refresh_token, secret) {
                if !claims.jti.is_empty() {
                    let expires_at = chrono::DateTime::from_timestamp(claims.exp as i64, 0)
                        .unwrap_or_else(chrono::Utc::now)
                        .to_rfc3339();
                    revocation_set.revoke_with_expiry(&claims.jti, &expires_at);
                }
            }
        }
    }

    // Clear the refresh cookie.
    let mut response = (
        StatusCode::OK,
        Json(serde_json::json!({"message": "Logged out"})),
    )
        .into_response();

    if let Ok(val) =
        axum::http::HeaderValue::from_str(&build_clear_refresh_cookie(config.is_production))
    {
        response.headers_mut().insert("set-cookie", val);
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use axum::Extension;
    use axum::Router;
    use serde_json::Value;
    use tower::ServiceExt;

    fn legacy_auth_router() -> Router {
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: None,
            legacy_token: Some("test-token".into()),
            is_production: false,
        });
        let revocation_set = Arc::new(RevocationSet::new());

        Router::new()
            .route("/api/auth/session", get(session))
            .route("/api/auth/logout", post(logout))
            .layer(axum::middleware::from_fn(auth_middleware))
            .layer(Extension(auth_config))
            .layer(Extension(revocation_set))
    }

    fn jwt_auth_router() -> (Router, Arc<RevocationSet>, String) {
        let secret = "jwt-test-secret".to_string();
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: Some(secret.clone()),
            legacy_token: None,
            is_production: false,
        });
        let revocation_set = Arc::new(RevocationSet::new());

        let router = Router::new()
            .route("/api/auth/session", get(session))
            .route("/api/auth/logout", post(logout))
            .layer(axum::middleware::from_fn(auth_middleware))
            .layer(Extension(auth_config))
            .layer(Extension(revocation_set.clone()));

        (router, revocation_set, secret)
    }

    fn jwt_for(secret: &str, sub: &str, role: &str, jti: &str) -> String {
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        encode_jwt(
            &Claims {
                sub: sub.into(),
                role: role.into(),
                capabilities: Vec::new(),
                authz_v: Some(AUTHZ_CLAIMS_VERSION_V1),
                exp: now + 3600,
                iat: now,
                jti: jti.into(),
                iss: Some(INTERNAL_JWT_ISSUER.into()),
            },
            secret,
        )
        .expect("jwt should encode")
    }

    fn legacy_jwt_for(secret: &str, sub: &str, role: &str, jti: &str) -> String {
        let now = chrono::Utc::now().timestamp().max(0) as u64;
        encode_jwt(
            &Claims {
                sub: sub.into(),
                role: role.into(),
                capabilities: Vec::new(),
                authz_v: None,
                exp: now + 3600,
                iat: now,
                jti: jti.into(),
                iss: None,
            },
            secret,
        )
        .expect("legacy jwt should encode")
    }

    #[test]
    fn refresh_cookie_omits_secure_in_dev_mode() {
        let cookie = build_refresh_cookie("refresh-token", 60, false);
        assert!(!cookie.contains("Secure"));

        let clear_cookie = build_clear_refresh_cookie(false);
        assert!(!clear_cookie.contains("Secure"));
    }

    #[test]
    fn refresh_cookie_sets_secure_in_production_mode() {
        let cookie = build_refresh_cookie("refresh-token", 60, true);
        assert!(cookie.contains("Secure"));

        let clear_cookie = build_clear_refresh_cookie(true);
        assert!(clear_cookie.contains("Secure"));
    }

    #[tokio::test]
    async fn session_requires_bearer_token_when_auth_is_enabled() {
        let response = legacy_auth_router()
            .oneshot(
                Request::builder()
                    .uri("/api/auth/session")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn compatibility_probe_is_public_when_auth_is_enabled() {
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: Some("jwt-test-secret".into()),
            legacy_token: None,
            is_production: false,
        });
        let revocation_set = Arc::new(RevocationSet::new());

        let response = Router::new()
            .route("/api/compatibility", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(auth_middleware))
            .layer(Extension(auth_config))
            .layer(Extension(revocation_set))
            .oneshot(
                Request::builder()
                    .uri("/api/compatibility")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn oauth_callback_is_public_when_auth_is_enabled() {
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: Some("jwt-test-secret".into()),
            legacy_token: None,
            is_production: false,
        });
        let revocation_set = Arc::new(RevocationSet::new());

        let response = Router::new()
            .route("/api/oauth/callback", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(auth_middleware))
            .layer(Extension(auth_config))
            .layer(Extension(revocation_set))
            .oneshot(
                Request::builder()
                    .uri("/api/oauth/callback?code=test&state=test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn login_issues_authz_v1_claims() {
        let secret = "jwt-test-secret".to_string();
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: Some(secret.clone()),
            legacy_token: Some("credential".into()),
            is_production: false,
        });

        let response = Router::new()
            .route("/api/auth/login", post(login))
            .layer(Extension(auth_config))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/login")
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"token":"credential"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        let access_token = payload["access_token"].as_str().unwrap();
        let claims = decode_jwt(access_token, &secret).expect("login jwt should decode");

        assert_eq!(claims.authz_v, Some(AUTHZ_CLAIMS_VERSION_V1));
        assert!(claims.capabilities.is_empty());
        assert_eq!(claims.iss.as_deref(), Some(INTERNAL_JWT_ISSUER));
    }

    #[tokio::test]
    async fn refresh_upgrades_legacy_role_only_jwt_to_authz_v1() {
        let secret = "jwt-test-secret".to_string();
        let auth_config = Arc::new(AuthConfig {
            jwt_secret: Some(secret.clone()),
            legacy_token: None,
            is_production: false,
        });
        let revocation_set = Arc::new(RevocationSet::new());

        let response = Router::new()
            .route("/api/auth/refresh", post(refresh))
            .layer(Extension(auth_config))
            .layer(Extension(revocation_set))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/refresh")
                    .header(
                        "authorization",
                        format!(
                            "Bearer {}",
                            legacy_jwt_for(&secret, "legacy-user", "operator", "legacy-jti")
                        ),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        let access_token = payload["access_token"].as_str().unwrap();
        let claims = decode_jwt(access_token, &secret).expect("refreshed jwt should decode");

        assert_eq!(claims.role, "operator");
        assert_eq!(claims.authz_v, Some(AUTHZ_CLAIMS_VERSION_V1));
        assert!(claims.capabilities.is_empty());
        assert_eq!(claims.iss.as_deref(), Some(INTERNAL_JWT_ISSUER));
    }

    #[tokio::test]
    async fn session_returns_authenticated_summary_for_valid_legacy_token() {
        let response = legacy_auth_router()
            .oneshot(
                Request::builder()
                    .uri("/api/auth/session")
                    .header("authorization", "Bearer test-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["authenticated"], true);
        assert_eq!(payload["subject"], "legacy-token-user");
        assert_eq!(payload["role"], "admin");
        assert_eq!(payload["mode"], "legacy");
    }

    #[tokio::test]
    async fn session_returns_authenticated_summary_for_valid_jwt_token() {
        let (router, _revocation_set, secret) = jwt_auth_router();
        let access_token = jwt_for(&secret, "jwt-user", "operator", "jwt-session-jti");

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/api/auth/session")
                    .header("authorization", format!("Bearer {access_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let payload: Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(payload["authenticated"], true);
        assert_eq!(payload["subject"], "jwt-user");
        assert_eq!(payload["role"], "operator");
        assert_eq!(payload["mode"], "jwt");
    }

    #[tokio::test]
    async fn logout_always_clears_refresh_cookie() {
        let response = legacy_auth_router()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .header("cookie", "ghost_refresh=stale-refresh-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let clear_cookie = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .unwrap();
        assert!(clear_cookie.contains("ghost_refresh="));
        assert!(clear_cookie.contains("Max-Age=0"));
    }

    #[tokio::test]
    async fn logout_revokes_jwt_access_and_refresh_tokens() {
        let (router, revocation_set, secret) = jwt_auth_router();
        let access_token = jwt_for(&secret, "jwt-user", "admin", "access-jti");
        let refresh_token = jwt_for(&secret, "jwt-user", "admin", "refresh-jti");

        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/auth/logout")
                    .header("authorization", format!("Bearer {access_token}"))
                    .header("cookie", format!("ghost_refresh={refresh_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let clear_cookie = response
            .headers()
            .get("set-cookie")
            .and_then(|value| value.to_str().ok())
            .unwrap();
        assert!(clear_cookie.contains("ghost_refresh="));
        assert!(clear_cookie.contains("Max-Age=0"));
        assert!(revocation_set.is_revoked("access-jti"));
        assert!(revocation_set.is_revoked("refresh-jti"));

        let session_response = router
            .oneshot(
                Request::builder()
                    .uri("/api/auth/session")
                    .header("authorization", format!("Bearer {access_token}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(session_response.status(), StatusCode::UNAUTHORIZED);
    }
}
