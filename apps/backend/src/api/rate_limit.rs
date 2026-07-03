use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::Response,
    Extension, Json,
};
use dashmap::DashMap;
use rusqlite::Connection;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use crate::models::types::{ApiError, AuthContext};

// ── Quota constants ────────────────────────────────────────────────────────────

/// Per-tier token-bucket parameters.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TierQuota {
    /// Maximum tokens in the bucket (= burst capacity = requests per minute).
    pub capacity: f64,
    /// Tokens added per second (capacity / 60).
    pub refill_per_sec: f64,
}

const QUOTAS: &[(&str, TierQuota)] = &[
    (
        "free",
        TierQuota {
            capacity: 100.0,
            refill_per_sec: 100.0 / 60.0,
        },
    ),
    (
        "team",
        TierQuota {
            capacity: 1000.0,
            refill_per_sec: 1000.0 / 60.0,
        },
    ),
    (
        "enterprise",
        TierQuota {
            capacity: 10000.0,
            refill_per_sec: 10000.0 / 60.0,
        },
    ),
];

/// Returns the `TierQuota` for the given plan string.
/// Defaults to "free" if unrecognised.
pub fn quota_for(plan: &str) -> TierQuota {
    QUOTAS
        .iter()
        .find(|(name, _)| *name == plan)
        .map(|(_, q)| *q)
        .unwrap_or(QUOTAS[0].1)
}

// ── Bucket ────────────────────────────────────────────────────────────────────

/// A single token-bucket entry keyed by user_id.
///
/// The refill and consume logic accepts an injectable `now: Instant` so that
/// tests can simulate time advancement without sleeping.
pub struct Bucket {
    /// Current token count (float to support fractional refill).
    pub tokens: f64,
    /// Last time the bucket was refilled (used to compute elapsed duration).
    pub last_refill: Instant,
    /// Last time a request was served (used for idle eviction).
    pub last_seen: Instant,
    /// Cached tier quota — avoids a DB hit on every request.
    pub quota: TierQuota,
}

impl Bucket {
    /// Create a new full bucket for the given quota.
    pub fn new(quota: TierQuota, now: Instant) -> Self {
        Self {
            tokens: quota.capacity,
            last_refill: now,
            last_seen: now,
            quota,
        }
    }

    /// Refill the bucket based on elapsed time, then attempt to consume one token.
    ///
    /// Returns `true` if a token was consumed (request allowed), `false` if
    /// the bucket is empty (request should be rate-limited).
    ///
    /// `now` is injected so tests can control the clock.
    pub fn try_consume(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_refill);
        self.tokens = (self.tokens + elapsed.as_secs_f64() * self.quota.refill_per_sec)
            .min(self.quota.capacity);
        self.last_refill = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            self.last_seen = now;
            true
        } else {
            false
        }
    }

    /// Seconds until the bucket has at least one token.
    pub fn retry_after_secs(&self) -> u64 {
        let deficit = 1.0 - self.tokens;
        (deficit / self.quota.refill_per_sec).ceil() as u64
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

/// Axum state for the rate-limit middleware layer.
///
/// `buckets` is keyed by `user_id` (string).
/// `conn` is the DB connection used to fetch the org's plan on bucket creation.
/// `request_counter` drives lazy eviction every 1024 requests.
#[derive(Clone)]
pub struct RateLimitState {
    pub buckets: Arc<DashMap<String, Bucket>>,
    pub conn: Arc<Mutex<Connection>>,
    pub request_counter: Arc<AtomicU64>,
}

impl RateLimitState {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            conn,
            request_counter: Arc::new(AtomicU64::new(0)),
        }
    }
}

// ── Eviction ──────────────────────────────────────────────────────────────────

/// Sweep stale buckets whose `last_seen` is older than `max_age` from now.
///
/// Called lazily every 1024 requests.
fn evict_stale(state: &RateLimitState, now: Instant, max_age: Duration) {
    state.buckets.retain(|_, bucket| {
        now.duration_since(bucket.last_seen) < max_age
    });
}

// ── Plan lookup ───────────────────────────────────────────────────────────────

/// Fetch the org plan from the database.
/// Returns "free" on any error to fail-open.
fn lookup_plan(conn: &Arc<Mutex<Connection>>, org_id: &str) -> String {
    let Ok(db) = conn.lock() else {
        return "free".to_string();
    };
    db.query_row(
        "SELECT plan FROM organizations WHERE id = ?1",
        rusqlite::params![org_id],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_else(|_| "free".to_string())
}

// ── Exempt paths ──────────────────────────────────────────────────────────────

/// Authenticated paths that must never be rate-limited. A 429 on the session
/// bootstrap (`/v1/admin/auth/me`) makes the frontend treat the session as invalid
/// and logs the user out — so a burst of dashboard navigation could sign an admin
/// out. These endpoints are already gated by valid auth and are cheap, so exempting
/// them removes the logout failure mode without weakening tenant limits on real work.
const RATE_LIMIT_EXEMPT_PATHS: &[&str] = &[
    "/v1/admin/auth/me",
];

// ── Middleware function ───────────────────────────────────────────────────────

/// Axum `from_fn_with_state` middleware that enforces per-user token-bucket rate
/// limits.
///
/// Must be layered BELOW the auth middleware in code (so auth runs first at
/// runtime and this middleware can read `Extension<AuthContext>`).
///
/// Every response — success or 429 — includes `X-RateLimit-Limit` and
/// `X-RateLimit-Remaining`. Throttled responses additionally carry `Retry-After`.
pub async fn rate_limit(
    State(state): State<RateLimitState>,
    Extension(auth): Extension<AuthContext>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, (StatusCode, axum::http::HeaderMap, Json<ApiError>)> {
    // Session-bootstrap and other exempt paths bypass the limiter entirely so a
    // throttle can never log the user out (see RATE_LIMIT_EXEMPT_PATHS).
    if RATE_LIMIT_EXEMPT_PATHS.contains(&req.uri().path()) {
        return Ok(next.run(req).await);
    }

    let now = Instant::now();
    let user_key = auth.user_id.clone();

    // Lazy eviction every 1024 requests.
    let count = state
        .request_counter
        .fetch_add(1, Ordering::Relaxed);
    if count % 1024 == 0 && count > 0 {
        evict_stale(&state, now, Duration::from_secs(120));
    }

    // Consume one token and capture the quota state for response headers.
    // Returns (allowed, limit, remaining_after_consume, retry_after_secs).
    let (allowed, limit, remaining, retry_after) = {
        if let Some(mut bucket) = state.buckets.get_mut(&user_key) {
            let ok = bucket.try_consume(now);
            let lim = bucket.quota.capacity as u64;
            let rem = bucket.tokens.max(0.0) as u64;
            let retry = if ok { 0 } else { bucket.retry_after_secs() };
            (ok, lim, rem, retry)
        } else {
            // Slow path: create a new bucket — one DB round-trip to get the plan.
            let plan = lookup_plan(&state.conn, &auth.org_id);
            let quota = quota_for(&plan);
            let mut bucket = Bucket::new(quota, now);
            let ok = bucket.try_consume(now);
            let lim = bucket.quota.capacity as u64;
            let rem = bucket.tokens.max(0.0) as u64;
            let retry = if ok { 0 } else { bucket.retry_after_secs() };
            state.buckets.insert(user_key.clone(), bucket);
            (ok, lim, rem, retry)
        }
    };

    if allowed {
        let mut response = next.run(req).await;
        let hdrs = response.headers_mut();
        if let Ok(v) = limit.to_string().parse() {
            hdrs.insert("x-ratelimit-limit", v);
        }
        if let Ok(v) = remaining.to_string().parse() {
            hdrs.insert("x-ratelimit-remaining", v);
        }
        Ok(response)
    } else {
        let mut headers = axum::http::HeaderMap::new();
        if let Ok(v) = retry_after.to_string().parse() {
            headers.insert("retry-after", v);
        }
        if let Ok(v) = limit.to_string().parse() {
            headers.insert("x-ratelimit-limit", v);
        }
        headers.insert("x-ratelimit-remaining", "0".parse().unwrap());

        Err((
            StatusCode::TOO_MANY_REQUESTS,
            headers,
            Json(ApiError {
                error: "Rate limit exceeded".to_string(),
                code: "rate_limited".to_string(),
            }),
        ))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    use crate::api::middleware as auth_mw;
    use crate::db::{connection::connect, migrations};
    use crate::db::queries::bootstrap;
    use crate::store::sqlite::SqliteStore;

    fn make_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn make_state(conn: Arc<Mutex<Connection>>) -> RateLimitState {
        RateLimitState::new(conn)
    }

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    /// Build a test router that mirrors the real protected-routes setup:
    /// auth middleware is outermost (last `.layer()`) so it runs first;
    /// rate_limit middleware is inner so it runs second (after auth populates
    /// `Extension<AuthContext>`).
    fn app_with_rate_limit(store: SqliteStore) -> Router {
        let rate_state = RateLimitState::new(store.conn());
        Router::new()
            .route("/v1/test", get(|| async { "ok" }))
            .route("/v1/admin/auth/me", get(|| async { "me" }))
            .layer(middleware::from_fn_with_state(
                rate_state,
                super::rate_limit,
            ))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    /// The auth/session bootstrap path must never be rate-limited: a 429 there logs
    /// the user out of the frontend. Even after the user's shared bucket is drained,
    /// GET /v1/admin/auth/me must still succeed.
    #[tokio::test]
    async fn auth_me_bootstrap_is_exempt_from_rate_limit() {
        let store = make_store();
        let api_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        let app = app_with_rate_limit(store);

        // Drain the free-tier bucket (100) via a non-exempt path.
        for _ in 0..100 {
            let _ = app.clone().oneshot(
                Request::builder().uri("/v1/test")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .body(Body::empty()).unwrap(),
            ).await.unwrap();
        }
        // Non-exempt path is now throttled.
        let throttled = app.clone().oneshot(
            Request::builder().uri("/v1/test")
                .header("Authorization", format!("Bearer {api_key}"))
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(throttled.status(), StatusCode::TOO_MANY_REQUESTS, "non-exempt path must be throttled once drained");

        // The auth bootstrap path must STILL succeed — no logout.
        let me = app.clone().oneshot(
            Request::builder().uri("/v1/admin/auth/me")
                .header("Authorization", format!("Bearer {api_key}"))
                .body(Body::empty()).unwrap(),
        ).await.unwrap();
        assert_eq!(me.status(), StatusCode::OK, "auth/me bootstrap must be exempt from rate limiting");
    }

    // ── Unit tests for Bucket logic ──────────────────────────────────────────

    /// A bucket that has not been exhausted allows the request through.
    #[test]
    fn rate_limit_within_quota_passes_through() {
        let now = Instant::now();
        let quota = quota_for("free"); // capacity = 100
        let mut bucket = Bucket::new(quota, now);

        // Consume 99 tokens — still allowed.
        for _ in 0..99 {
            assert!(bucket.try_consume(now), "each of the first 99 requests must be allowed");
        }
        // 100th (last token) — still allowed.
        assert!(bucket.try_consume(now), "100th request must be allowed on free tier");
    }

    /// After exhausting the free-tier bucket the 101st call must be rejected and
    /// `retry_after_secs` must be a positive value.
    #[test]
    fn rate_limit_exhausted_returns_429_with_retry_after() {
        let now = Instant::now();
        let quota = quota_for("free");
        let mut bucket = Bucket::new(quota, now);

        // Drain all 100 tokens.
        for _ in 0..100 {
            bucket.try_consume(now);
        }

        // 101st must be denied.
        assert!(!bucket.try_consume(now), "101st request must be denied");
        assert!(
            bucket.retry_after_secs() > 0,
            "retry_after_secs must be positive when bucket is empty"
        );
    }

    /// Exhausting the bucket and then advancing the clock past the refill window
    /// must restore tokens so the next request succeeds.
    #[test]
    fn rate_limit_bucket_refills_after_window() {
        let now = Instant::now();
        let quota = quota_for("free");
        let mut bucket = Bucket::new(quota, now);

        // Drain fully.
        for _ in 0..100 {
            bucket.try_consume(now);
        }
        assert!(!bucket.try_consume(now), "bucket must be empty after 100 requests");

        // Advance the clock by 61 seconds (> 1-minute window).
        let future = now + Duration::from_secs(61);
        assert!(
            bucket.try_consume(future),
            "bucket must allow requests after the refill window has elapsed"
        );
    }

    /// A team-tier bucket (capacity 1000) must not be exhausted after 100 requests
    /// where a free-tier bucket would be.
    #[test]
    fn rate_limit_team_key_higher_quota() {
        let now = Instant::now();
        let free_quota = quota_for("free");
        let team_quota = quota_for("team");

        let mut free_bucket = Bucket::new(free_quota, now);
        let mut team_bucket = Bucket::new(team_quota, now);

        // Consume 100 tokens from both.
        for _ in 0..100 {
            free_bucket.try_consume(now);
            team_bucket.try_consume(now);
        }

        // Free tier is exhausted.
        assert!(
            !free_bucket.try_consume(now),
            "free bucket must be exhausted after 100 requests"
        );
        // Team tier still has plenty of capacity.
        assert!(
            team_bucket.try_consume(now),
            "team bucket must still allow requests after 100 (capacity = 1000)"
        );
    }

    /// Two different user_id keys must operate on independent buckets — exhausting
    /// one must not affect the other.
    #[test]
    fn rate_limit_different_users_independent_buckets() {
        let now = Instant::now();
        let quota = quota_for("free");

        let mut bucket_a = Bucket::new(quota, now);
        let mut bucket_b = Bucket::new(quota, now);

        // Drain user A fully.
        for _ in 0..100 {
            bucket_a.try_consume(now);
        }
        assert!(!bucket_a.try_consume(now), "user A must be rate-limited");

        // User B is unaffected.
        assert!(
            bucket_b.try_consume(now),
            "user B must still be allowed (independent bucket)"
        );
    }

    /// Inserting >1024 requests from distinct users with stale last_seen timestamps
    /// must trigger the lazy eviction sweep and bound the map size.
    #[test]
    fn rate_limit_lazy_eviction_removes_stale_entries() {
        let db = make_db();
        let state = make_state(db);

        // Insert 1025 buckets with a `last_seen` in the past (>2 min ago).
        let stale_time = Instant::now() - Duration::from_secs(300);

        for i in 0..1025_u64 {
            let key = format!("user-stale-{i}");
            let quota = quota_for("free");
            let bucket = Bucket {
                tokens: quota.capacity,
                last_refill: stale_time,
                last_seen: stale_time,
                quota,
            };
            state.buckets.insert(key, bucket);
        }

        assert_eq!(
            state.buckets.len(),
            1025,
            "all 1025 entries must be present before eviction"
        );

        // Simulate the eviction sweep (max_age = 2 min = 120 s).
        let now = Instant::now();
        evict_stale(&state, now, Duration::from_secs(120));

        assert_eq!(
            state.buckets.len(),
            0,
            "all stale entries must be removed after eviction sweep"
        );
    }

    // ── T-10 integration tests ────────────────────────────────────────────────

    /// An unauthenticated request must return 401, not 429.
    ///
    /// This proves that auth runs before the rate limiter: an unauth flood must
    /// not allocate buckets or generate 429 responses.
    #[tokio::test]
    async fn router_rate_limit_applied_after_auth() {
        let store = make_store();

        let resp = app_with_rate_limit(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    // No Authorization header — should be rejected by auth, not rate limiter.
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "unauthenticated request must be rejected by auth (401), not rate limiter (429)"
        );
    }

    /// The first successful request must include X-RateLimit-Limit and
    /// X-RateLimit-Remaining headers so callers can track their quota.
    #[tokio::test]
    async fn router_rate_limit_headers_present_on_success() {
        let store = make_store();
        let api_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let resp = app_with_rate_limit(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            resp.headers().contains_key("x-ratelimit-limit"),
            "200 response must include X-RateLimit-Limit header"
        );
        assert!(
            resp.headers().contains_key("x-ratelimit-remaining"),
            "200 response must include X-RateLimit-Remaining header"
        );

        let limit: u64 = resp
            .headers()
            .get("x-ratelimit-limit")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .expect("X-RateLimit-Limit must be a valid integer");
        let remaining: u64 = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .expect("X-RateLimit-Remaining must be a valid integer");

        assert!(limit > 0, "X-RateLimit-Limit must be positive");
        assert!(
            remaining < limit,
            "X-RateLimit-Remaining ({remaining}) must be less than limit ({limit}) after one request"
        );
    }

    /// Sending 101 requests with a free-tier API key must result in the 101st
    /// request returning 429 with Retry-After, X-RateLimit-Limit, and
    /// X-RateLimit-Remaining headers; the remaining value must be 0.
    #[tokio::test]
    async fn router_rate_limit_returns_429_on_exhaustion_integration() {
        let store = make_store();
        let api_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };

        let app = app_with_rate_limit(store);

        // The first 100 requests must all succeed (free tier = 100 req/min).
        for i in 0..100 {
            let resp = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/v1/test")
                        .header("Authorization", format!("Bearer {api_key}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "request #{} must succeed (within free-tier quota)",
                i + 1
            );
        }

        // The 101st request must be rate-limited.
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/v1/test")
                    .header("Authorization", format!("Bearer {api_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "101st request must be rate-limited (429)"
        );
        assert!(
            resp.headers().contains_key("retry-after"),
            "429 response must include Retry-After header"
        );
        assert!(
            resp.headers().contains_key("x-ratelimit-limit"),
            "429 response must include X-RateLimit-Limit header"
        );
        assert!(
            resp.headers().contains_key("x-ratelimit-remaining"),
            "429 response must include X-RateLimit-Remaining header"
        );

        let remaining: u64 = resp
            .headers()
            .get("x-ratelimit-remaining")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .expect("X-RateLimit-Remaining must be a valid integer");
        assert_eq!(remaining, 0, "X-RateLimit-Remaining must be 0 when rate-limited");
    }
}
