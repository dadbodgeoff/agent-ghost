//! Rate limiting middleware using `governor` crate.
//!
//! Three tiers:
//! - Unauthenticated: 20 req/min per IP
//! - Authenticated: 200 req/min per token
//! - Safety-critical (`/api/safety/*`): 10 req/min per token
//!
//! Returns 429 Too Many Requests with `Retry-After` header.
//!
//! Ref: T-1.1.5, §5.0.13

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use dashmap::DashMap;
use governor::clock::DefaultClock;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use crate::api::auth::Claims;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

/// Shared rate limiter state, keyed by client identifier.
///
/// Each unique client (IP for unauthenticated, JWT `sub` for authenticated)
/// gets its own token-bucket limiter. Limiters are created lazily on first
/// request and stored in concurrent DashMaps for lock-free reads.
pub struct RateLimitState {
    /// Per-IP limiters for unauthenticated requests (20 req/min).
    ip_limiters: DashMap<IpAddr, Arc<Limiter>>,
    /// Per-token limiters for authenticated requests (200 req/min).
    token_limiters: DashMap<String, Arc<Limiter>>,
    /// Per-token limiters for safety-critical endpoints (10 req/min).
    safety_limiters: DashMap<String, Arc<Limiter>>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            ip_limiters: DashMap::new(),
            token_limiters: DashMap::new(),
            safety_limiters: DashMap::new(),
        }
    }

    fn get_ip_limiter(&self, ip: IpAddr) -> Arc<Limiter> {
        self.ip_limiters
            .entry(ip)
            .or_insert_with(|| {
                let quota = Quota::per_minute(NonZeroU32::new(20).unwrap());
                Arc::new(RateLimiter::direct(quota))
            })
            .clone()
    }

    fn get_token_limiter(&self, token_id: &str) -> Arc<Limiter> {
        self.token_limiters
            .entry(token_id.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_minute(NonZeroU32::new(200).unwrap());
                Arc::new(RateLimiter::direct(quota))
            })
            .clone()
    }

    fn get_safety_limiter(&self, token_id: &str) -> Arc<Limiter> {
        self.safety_limiters
            .entry(token_id.to_string())
            .or_insert_with(|| {
                let quota = Quota::per_minute(NonZeroU32::new(10).unwrap());
                Arc::new(RateLimiter::direct(quota))
            })
            .clone()
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate limiting middleware (axum 0.7 signature).
///
/// Checks rate limits based on authentication state and endpoint path.
/// Must run AFTER auth middleware so `Claims` are available in extensions.
///
/// Skip paths: health, ready, auth endpoints (same as auth middleware).
pub async fn rate_limit_middleware(
    request: Request<Body>,
    next: Next,
) -> Response {
    let path = request.uri().path();

    // Skip rate limiting for health/auth endpoints.
    let skip = matches!(
        path,
        "/api/health"
            | "/api/ready"
            | "/api/auth/login"
            | "/api/auth/refresh"
            | "/api/auth/logout"
            | "/api/openapi.json"
    );
    if skip {
        return next.run(request).await;
    }

    // Extract rate limit state from extensions.
    let rate_state = match request.extensions().get::<Arc<RateLimitState>>() {
        Some(state) => state.clone(),
        None => return next.run(request).await,
    };

    let is_safety = path.starts_with("/api/safety/");

    // Determine client identity: authenticated sub or IP.
    let claims = request.extensions().get::<Claims>().cloned();

    let limiter = if let Some(ref c) = claims {
        if is_safety {
            rate_state.get_safety_limiter(&c.sub)
        } else {
            rate_state.get_token_limiter(&c.sub)
        }
    } else {
        // Unauthenticated — rate limit by IP.
        let ip = request
            .extensions()
            .get::<ConnectInfo<std::net::SocketAddr>>()
            .map(|ci| ci.0.ip())
            .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED));
        rate_state.get_ip_limiter(ip)
    };

    match limiter.check() {
        Ok(_) => next.run(request).await,
        Err(not_until) => {
            let wait = not_until
                .wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
            let secs = wait.as_secs().max(1);

            let body = serde_json::json!({
                "error": {
                    "code": "RATE_LIMITED",
                    "message": format!("Too many requests. Retry after {}s.", secs),
                }
            });

            let mut resp = (StatusCode::TOO_MANY_REQUESTS, body.to_string()).into_response();

            if let Ok(val) = axum::http::HeaderValue::from_str(&secs.to_string()) {
                resp.headers_mut().insert("retry-after", val);
            }

            resp
        }
    }
}

/// Request ID middleware (T-1.1.6).
///
/// Injects `X-Request-ID` into every request (from header or generated UUID v7).
/// Propagates the same ID into the response headers for client correlation.
pub async fn request_id_middleware(
    mut request: Request<Body>,
    next: Next,
) -> Response {
    // Read existing X-Request-ID or generate a new one.
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    // Store in extensions for handlers/tracing to access.
    request.extensions_mut().insert(RequestId(request_id.clone()));

    let mut response = next.run(request).await;

    // Set X-Request-ID on response.
    if let Ok(val) = axum::http::HeaderValue::from_str(&request_id) {
        response.headers_mut().insert("x-request-id", val);
    }

    response
}

/// Newtype for request ID stored in extensions.
#[derive(Clone, Debug)]
pub struct RequestId(pub String);
