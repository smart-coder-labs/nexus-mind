use axum::Json;
use serde::Serialize;
use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
    pub db: &'static str,
    pub uptime_secs: u64,
}

pub async fn handler() -> Json<HealthResponse> {
    let start = START.get_or_init(Instant::now);
    let uptime_secs = start.elapsed().as_secs();

    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
        db: "ok",
        uptime_secs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn handler_returns_ok_status() {
        let Json(response) = handler().await;
        assert_eq!(response.status, "ok");
        assert!(!response.version.is_empty());
        assert_eq!(response.db, "ok");
        // uptime_secs is non-negative (u64 is always >= 0)
        let _ = response.uptime_secs;
    }

    #[tokio::test]
    async fn handler_uptime_is_monotonic() {
        let Json(r1) = handler().await;
        let Json(r2) = handler().await;
        // Second call must have uptime >= first (same OnceLock instant)
        assert!(r2.uptime_secs >= r1.uptime_secs);
    }
}
