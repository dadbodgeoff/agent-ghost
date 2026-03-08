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
pub async fn rate_limit_middleware(request: Request<Body>, next: Next) -> Response {
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

    // T-5.11.1: Determine rate limit quota for header emission.
    let quota_limit: u32 = if claims.is_some() {
        if is_safety {
            10
        } else {
            200
        }
    } else {
        20
    };

    match limiter.check() {
        Ok(_) => {
            let mut resp = next.run(request).await;
            // T-5.11.1: Inject rate limit headers on all successful responses.
            let headers = resp.headers_mut();
            if let Ok(v) = axum::http::HeaderValue::from_str(&quota_limit.to_string()) {
                headers.insert("x-ratelimit-limit", v);
            }
            if let Ok(v) = axum::http::HeaderValue::from_str("60") {
                headers.insert("x-ratelimit-reset", v);
            }
            resp
        }
        Err(not_until) => {
            let wait =
                not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
            let secs = wait.as_secs().max(1);

            let body = serde_json::json!({
                "error": {
                    "code": "RATE_LIMITED",
                    "message": format!("Too many requests. Retry after {}s.", secs),
                }
            });

            let mut resp = (StatusCode::TOO_MANY_REQUESTS, body.to_string()).into_response();

            let headers = resp.headers_mut();
            if let Ok(val) = axum::http::HeaderValue::from_str(&secs.to_string()) {
                headers.insert("retry-after", val);
            }
            // T-5.11.1: Include rate limit headers on 429 responses too.
            if let Ok(v) = axum::http::HeaderValue::from_str(&quota_limit.to_string()) {
                headers.insert("x-ratelimit-limit", v);
            }
            if let Ok(v) = axum::http::HeaderValue::from_str("0") {
                headers.insert("x-ratelimit-remaining", v);
            }
            if let Ok(v) = axum::http::HeaderValue::from_str(&secs.to_string()) {
                headers.insert("x-ratelimit-reset", v);
            }

            resp
        }
    }
}

// ── T-5.11.2: Safety action cooldown tracker ────────────────────────

/// Tracks safety actions per actor. After 3 actions in 10 minutes, a 5-minute
/// cooldown is enforced. Uses DashMap for lock-free concurrent access.
pub struct SafetyCooldown {
    /// actor → list of action timestamps (kept pruned to last 10 min)
    actions: DashMap<String, Vec<std::time::Instant>>,
}

impl SafetyCooldown {
    pub fn new() -> Self {
        Self {
            actions: DashMap::new(),
        }
    }

    /// Record a safety action for the given actor. Returns Err with cooldown
    /// seconds remaining if the actor is in cooldown.
    pub fn check_and_record(&self, actor: &str) -> Result<(), u64> {
        let now = std::time::Instant::now();
        let window = std::time::Duration::from_secs(600); // 10 minutes
        let cooldown = std::time::Duration::from_secs(300); // 5 minutes

        let mut entry = self.actions.entry(actor.to_string()).or_default();
        let timestamps = entry.value_mut();

        // Prune entries older than 10 minutes.
        timestamps.retain(|t| now.duration_since(*t) < window);

        // If 3+ actions within window, check if cooldown has elapsed since the 3rd.
        if timestamps.len() >= 3 {
            let third_action = timestamps[timestamps.len() - 3];
            let since_third = now.duration_since(third_action);
            if since_third < cooldown {
                let remaining = (cooldown - since_third).as_secs().max(1);
                return Err(remaining);
            }
            // Cooldown elapsed — clear history and allow.
            timestamps.clear();
        }

        timestamps.push(now);
        Ok(())
    }
}

impl Default for SafetyCooldown {
    fn default() -> Self {
        Self::new()
    }
}
