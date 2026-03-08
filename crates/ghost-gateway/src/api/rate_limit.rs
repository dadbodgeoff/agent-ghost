//! Rate limiting middleware.
//!
//! Supports two backends:
//! - `process`: in-memory `governor` buckets
//! - `database`: shared SQLite buckets for multi-node deployments
//!
//! Three tiers:
//! - Unauthenticated: 20 req/min per IP
//! - Authenticated: 200 req/min per token
//! - Safety-critical (`/api/safety/*`): 10 req/min per token
//!
//! Returns 429 Too Many Requests with `Retry-After` header.

use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::atomic::{AtomicU32, Ordering};
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
use rusqlite::params;

use crate::api::auth::Claims;
use crate::config::RateLimitScope;
use crate::db_pool::DbPool;

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;
const RATE_LIMIT_WINDOW_SECS: u64 = 60;
const DB_CLEANUP_INTERVAL_REQUESTS: u32 = 256;

#[derive(Clone, Copy)]
struct RateLimitPolicy {
    bucket_scope: &'static str,
    limit: u32,
    window_secs: u64,
}

#[derive(Clone, Copy)]
enum RateLimitSubject<'a> {
    Ip(IpAddr),
    Token(&'a str),
}

struct RateLimitDecision {
    limit: u32,
    remaining: Option<u32>,
    reset_secs: u64,
    retry_after_secs: Option<u64>,
}

/// Shared rate limiter state.
pub struct RateLimitState {
    scope: RateLimitScope,
    db: Option<Arc<DbPool>>,
    cleanup_counter: AtomicU32,
    /// Per-IP limiters for unauthenticated requests (20 req/min).
    ip_limiters: DashMap<IpAddr, Arc<Limiter>>,
    /// Per-token limiters for authenticated requests (200 req/min).
    token_limiters: DashMap<String, Arc<Limiter>>,
    /// Per-token limiters for safety-critical endpoints (10 req/min).
    safety_limiters: DashMap<String, Arc<Limiter>>,
}

impl RateLimitState {
    pub fn new(db: Arc<DbPool>, scope: RateLimitScope) -> Self {
        Self {
            scope,
            db: Some(db),
            cleanup_counter: AtomicU32::new(0),
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

    async fn evaluate(
        &self,
        policy: RateLimitPolicy,
        subject: RateLimitSubject<'_>,
    ) -> Result<RateLimitDecision, String> {
        match self.scope {
            RateLimitScope::Process => Ok(self.evaluate_process(policy, subject)),
            RateLimitScope::Database => self.evaluate_database(policy, subject).await,
        }
    }

    fn evaluate_process(
        &self,
        policy: RateLimitPolicy,
        subject: RateLimitSubject<'_>,
    ) -> RateLimitDecision {
        let limiter = match subject {
            RateLimitSubject::Ip(ip) => self.get_ip_limiter(ip),
            RateLimitSubject::Token(token_id) if policy.bucket_scope == "safety_token" => {
                self.get_safety_limiter(token_id)
            }
            RateLimitSubject::Token(token_id) => self.get_token_limiter(token_id),
        };

        match limiter.check() {
            Ok(_) => RateLimitDecision {
                limit: policy.limit,
                remaining: None,
                reset_secs: policy.window_secs,
                retry_after_secs: None,
            },
            Err(not_until) => {
                let wait =
                    not_until.wait_time_from(governor::clock::Clock::now(&DefaultClock::default()));
                let retry_after_secs = wait.as_secs().max(1);
                RateLimitDecision {
                    limit: policy.limit,
                    remaining: Some(0),
                    reset_secs: retry_after_secs,
                    retry_after_secs: Some(retry_after_secs),
                }
            }
        }
    }

    async fn evaluate_database(
        &self,
        policy: RateLimitPolicy,
        subject: RateLimitSubject<'_>,
    ) -> Result<RateLimitDecision, String> {
        let Some(db) = self.db.as_ref() else {
            return Err("database-backed rate limiting is not configured".into());
        };
        let subject_key = match subject {
            RateLimitSubject::Ip(ip) => ip.to_string(),
            RateLimitSubject::Token(token_id) => token_id.to_string(),
        };
        let now_epoch = chrono::Utc::now().timestamp();
        let window_secs = policy.window_secs as i64;
        let bucket_start = now_epoch - now_epoch.rem_euclid(window_secs);

        let request_count: u32 = {
            let conn = db.write().await;
            conn.query_row(
                "INSERT INTO rate_limit_buckets (
                    scope, subject_key, bucket_start, window_seconds, request_count, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 1, datetime('now'))
                 ON CONFLICT(scope, subject_key, bucket_start)
                 DO UPDATE SET
                    request_count = rate_limit_buckets.request_count + 1,
                    updated_at = datetime('now')
                 RETURNING request_count",
                params![policy.bucket_scope, subject_key, bucket_start, window_secs],
                |row| row.get(0),
            )
            .map_err(|error| format!("persist database rate limit bucket: {error}"))?
        };

        self.maybe_cleanup_database_buckets(now_epoch).await;

        let reset_at = bucket_start + window_secs;
        let reset_secs = (reset_at - now_epoch).max(1) as u64;
        if request_count > policy.limit {
            return Ok(RateLimitDecision {
                limit: policy.limit,
                remaining: Some(0),
                reset_secs,
                retry_after_secs: Some(reset_secs),
            });
        }

        Ok(RateLimitDecision {
            limit: policy.limit,
            remaining: Some(policy.limit.saturating_sub(request_count)),
            reset_secs,
            retry_after_secs: None,
        })
    }

    async fn maybe_cleanup_database_buckets(&self, now_epoch: i64) {
        if self.scope != RateLimitScope::Database {
            return;
        }
        let should_cleanup = self.cleanup_counter.fetch_add(1, Ordering::Relaxed)
            % DB_CLEANUP_INTERVAL_REQUESTS
            == 0;
        if !should_cleanup {
            return;
        }
        let Some(db) = self.db.as_ref() else {
            return;
        };
        let conn = db.write().await;
        if let Err(error) = conn.execute(
            "DELETE FROM rate_limit_buckets WHERE bucket_start + window_seconds < ?1",
            params![now_epoch],
        ) {
            tracing::warn!(error = %error, "failed to prune expired database rate-limit buckets");
        }
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self {
            scope: RateLimitScope::Process,
            db: None,
            cleanup_counter: AtomicU32::new(0),
            ip_limiters: DashMap::new(),
            token_limiters: DashMap::new(),
            safety_limiters: DashMap::new(),
        }
    }
}

fn policy_for_request(is_authenticated: bool, is_safety: bool) -> RateLimitPolicy {
    match (is_authenticated, is_safety) {
        (false, _) => RateLimitPolicy {
            bucket_scope: "unauth_ip",
            limit: 20,
            window_secs: RATE_LIMIT_WINDOW_SECS,
        },
        (true, true) => RateLimitPolicy {
            bucket_scope: "safety_token",
            limit: 10,
            window_secs: RATE_LIMIT_WINDOW_SECS,
        },
        (true, false) => RateLimitPolicy {
            bucket_scope: "auth_token",
            limit: 200,
            window_secs: RATE_LIMIT_WINDOW_SECS,
        },
    }
}

fn apply_rate_limit_headers(headers: &mut axum::http::HeaderMap, decision: &RateLimitDecision) {
    if let Ok(value) = axum::http::HeaderValue::from_str(&decision.limit.to_string()) {
        headers.insert("x-ratelimit-limit", value);
    }
    if let Some(remaining) = decision.remaining {
        if let Ok(value) = axum::http::HeaderValue::from_str(&remaining.to_string()) {
            headers.insert("x-ratelimit-remaining", value);
        }
    }
    if let Ok(value) = axum::http::HeaderValue::from_str(&decision.reset_secs.to_string()) {
        headers.insert("x-ratelimit-reset", value);
    }
    if let Some(retry_after_secs) = decision.retry_after_secs {
        if let Ok(value) = axum::http::HeaderValue::from_str(&retry_after_secs.to_string()) {
            headers.insert("retry-after", value);
        }
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

    let policy = policy_for_request(claims.is_some(), is_safety);
    let subject = if let Some(ref c) = claims {
        RateLimitSubject::Token(&c.sub)
    } else {
        RateLimitSubject::Ip(
            request
                .extensions()
                .get::<ConnectInfo<std::net::SocketAddr>>()
                .map(|ci| ci.0.ip())
                .unwrap_or(IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED)),
        )
    };

    match rate_state.evaluate(policy, subject).await {
        Ok(decision) if decision.retry_after_secs.is_none() => {
            let mut resp = next.run(request).await;
            apply_rate_limit_headers(resp.headers_mut(), &decision);
            resp
        }
        Ok(decision) => {
            let secs = decision.retry_after_secs.unwrap_or(decision.reset_secs);
            let body = serde_json::json!({
                "error": {
                    "code": "RATE_LIMITED",
                    "message": format!("Too many requests. Retry after {}s.", secs),
                }
            });

            let mut resp = (StatusCode::TOO_MANY_REQUESTS, body.to_string()).into_response();
            apply_rate_limit_headers(resp.headers_mut(), &decision);
            resp
        }
        Err(error) => {
            tracing::error!(error = %error, "rate limiter backend unavailable");
            let body = serde_json::json!({
                "error": {
                    "code": "RATE_LIMIT_UNAVAILABLE",
                    "message": "Rate limiter backend unavailable.",
                }
            });
            let mut resp = (StatusCode::SERVICE_UNAVAILABLE, body.to_string()).into_response();
            if let Ok(v) = axum::http::HeaderValue::from_str("no-store") {
                resp.headers_mut().insert("cache-control", v);
            }
            resp
        }
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    struct Harness {
        _temp_dir: TempDir,
        db: Arc<DbPool>,
    }

    impl Harness {
        async fn new() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            let db = crate::db_pool::create_pool(temp_dir.path().join("rate-limit.db")).unwrap();
            {
                let writer = db.writer_for_migrations().await;
                cortex_storage::migrations::run_migrations(&writer).unwrap();
            }
            Self {
                _temp_dir: temp_dir,
                db,
            }
        }
    }

    #[tokio::test]
    async fn database_scope_shares_counts_across_pool_instances() {
        let harness = Harness::new().await;
        let state_a = RateLimitState::new(Arc::clone(&harness.db), RateLimitScope::Database);
        let state_b = RateLimitState::new(Arc::clone(&harness.db), RateLimitScope::Database);
        let policy = RateLimitPolicy {
            bucket_scope: "test_shared",
            limit: 2,
            window_secs: RATE_LIMIT_WINDOW_SECS,
        };

        let first = state_a
            .evaluate(policy, RateLimitSubject::Token("shared-subject"))
            .await
            .unwrap();
        let second = state_b
            .evaluate(policy, RateLimitSubject::Token("shared-subject"))
            .await
            .unwrap();
        let third = state_a
            .evaluate(policy, RateLimitSubject::Token("shared-subject"))
            .await
            .unwrap();

        assert_eq!(first.remaining, Some(1));
        assert_eq!(second.remaining, Some(0));
        let retry_after = third.retry_after_secs.expect("retry-after");
        assert!(retry_after >= 1);
        assert!(retry_after <= RATE_LIMIT_WINDOW_SECS);
    }

    #[tokio::test]
    async fn database_scope_reports_remaining_quota() {
        let harness = Harness::new().await;
        let state = RateLimitState::new(Arc::clone(&harness.db), RateLimitScope::Database);
        let policy = RateLimitPolicy {
            bucket_scope: "test_remaining",
            limit: 3,
            window_secs: RATE_LIMIT_WINDOW_SECS,
        };

        let first = state
            .evaluate(
                policy,
                RateLimitSubject::Ip(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            )
            .await
            .unwrap();
        let second = state
            .evaluate(
                policy,
                RateLimitSubject::Ip(IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)),
            )
            .await
            .unwrap();

        assert_eq!(first.remaining, Some(2));
        assert_eq!(second.remaining, Some(1));
        assert!(second.reset_secs >= 1);
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
