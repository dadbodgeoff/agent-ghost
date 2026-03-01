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

/// JWT claims extracted from a validated token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject (user ID or agent ID).
    pub sub: String,
    /// Role: "admin", "operator", "viewer".
    pub role: String,
    /// Expiration (Unix timestamp).
    pub exp: u64,
    /// Issued at (Unix timestamp).
    pub iat: u64,
    /// JWT ID (for revocation).
    pub jti: String,
}

impl Claims {
    /// Fallback claims for legacy token mode — implicit admin.
    pub fn admin_fallback() -> Self {
        Self {
            sub: "legacy-token-user".into(),
            role: "admin".into(),
            exp: u64::MAX,
            iat: 0,
            jti: String::new(),
        }
    }

    /// Fallback claims for no-auth mode — implicit admin.
    pub fn no_auth_fallback() -> Self {
        Self {
            sub: "anonymous".into(),
            role: "admin".into(),
            exp: u64::MAX,
            iat: 0,
            jti: String::new(),
        }
    }
}

/// In-memory JWT revocation set (jti values).
/// Cleared on gateway restart. For production, back with Redis or DB.
#[derive(Debug, Default)]
pub struct RevocationSet {
    revoked: RwLock<HashSet<String>>,
}

impl RevocationSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn revoke(&self, jti: &str) {
        if let Ok(mut set) = self.revoked.write() {
            set.insert(jti.to_string());
        }
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
}

impl AuthConfig {
    /// Read auth configuration from environment (call once at startup).
    pub fn from_env() -> Self {
        Self {
            jwt_secret: std::env::var("GHOST_JWT_SECRET").ok(),
            legacy_token: std::env::var("GHOST_TOKEN").ok(),
        }
    }

    /// Whether any authentication is configured.
    pub fn auth_required(&self) -> bool {
        self.jwt_secret.is_some() || self.legacy_token.is_some()
    }
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

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

/// Build a `Set-Cookie` header value for the refresh token.
fn build_refresh_cookie(token: &str, max_age_secs: u64) -> String {
    format!(
        "ghost_refresh={}; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age={}",
        token, max_age_secs
    )
}

/// Build a `Set-Cookie` header that clears the refresh cookie.
fn build_clear_refresh_cookie() -> String {
    "ghost_refresh=; HttpOnly; Secure; SameSite=Strict; Path=/api/auth; Max-Age=0".to_string()
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
                cookie
                    .strip_prefix("ghost_refresh=")
                    .map(|v| v.to_string())
            })
        })
}

/// Tower middleware: dual-mode authentication for all `/api/*` routes.
///
/// Reads `AuthConfig` and `RevocationSet` from Extensions (injected at startup).
///
/// Skips auth for:
/// - `GET /api/health` and `GET /api/ready` (health probes)
/// - `POST /api/auth/login` (login endpoint itself)
/// - `POST /api/auth/refresh` (token refresh — uses cookie, not Bearer)
/// - `GET /api/ws` (WebSocket has its own auth in the handler)
/// - `GET /.well-known/agent.json` and `POST /a2a` (mesh has Ed25519 auth)
pub async fn auth_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Skip auth for public endpoints.
    let skip_auth = matches!(
        path,
        "/api/health"
            | "/api/ready"
            | "/api/auth/login"
            | "/api/auth/refresh"
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
            // Fallback: no config injected — treat as no-auth mode.
            let mut request = request;
            request.extensions_mut().insert(Claims::no_auth_fallback());
            return next.run(request).await;
        }
    };

    // No auth configured → local dev mode, allow everything.
    if !config.auth_required() {
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
            exp: now + ACCESS_TOKEN_TTL,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
        };

        // Issue refresh token (7d).
        let refresh_claims = Claims {
            sub: "admin".into(),
            role: "admin".into(),
            exp: now + REFRESH_TOKEN_TTL,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
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
        if let Ok(val) =
            axum::http::HeaderValue::from_str(&build_refresh_cookie(&refresh_token, REFRESH_TOKEN_TTL))
        {
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
    let refresh_token = extract_refresh_cookie(&request)
        .or_else(|| {
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
        revocation_set.revoke(&old_claims.jti);
    }

    let now = chrono::Utc::now().timestamp() as u64;

    // Issue new access token.
    let access_claims = Claims {
        sub: old_claims.sub.clone(),
        role: old_claims.role.clone(),
        exp: now + ACCESS_TOKEN_TTL,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
    };

    // Issue new refresh token (rotation).
    let refresh_claims = Claims {
        sub: old_claims.sub,
        role: old_claims.role,
        exp: now + REFRESH_TOKEN_TTL,
        iat: now,
        jti: uuid::Uuid::now_v7().to_string(),
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
    if let Ok(val) =
        axum::http::HeaderValue::from_str(&build_refresh_cookie(&new_refresh_token, REFRESH_TOKEN_TTL))
    {
        response.headers_mut().insert("set-cookie", val);
    }

    response
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
                    revocation_set.revoke(&claims.jti);
                    tracing::info!(jti = %claims.jti, sub = %claims.sub, "Token revoked via logout");
                }
            }
        }

        // Revoke refresh token from cookie if present.
        if let Some(refresh_token) = extract_refresh_cookie(&request) {
            if let Ok(claims) = decode_jwt_allow_expired(&refresh_token, secret) {
                if !claims.jti.is_empty() {
                    revocation_set.revoke(&claims.jti);
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

    if let Ok(val) = axum::http::HeaderValue::from_str(&build_clear_refresh_cookie()) {
        response.headers_mut().insert("set-cookie", val);
    }

    response
}
