use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    api::helpers::{require_permission, AppJson},
    db::queries,
    models::types::{
        ApiError, AuthContext, CreateWebhookRequest, UpdateWebhookRequest, Webhook,
        WebhookDelivery, WebhookTestResult,
    },
    store::sqlite::SqliteStore,
};

#[derive(Serialize)]
pub struct RetryDeliveryResponse {
    pub delivery_id: String,
    pub status: String,
}

// ── Error helpers ─────────────────────────────────────────────────────────────

fn internal_error(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Webhook not found".to_string(),
            code: "webhook_not_found".to_string(),
        }),
    )
}

// ── Response wrappers ─────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct WebhooksResponse {
    pub webhooks: Vec<Webhook>,
}

// ── Handlers ──────────────────────────────────────────────────────────────────

/// `GET /v1/webhooks` — list all webhooks for the caller's org.
/// Admin-only.
pub async fn list_webhooks(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
) -> Result<Json<WebhooksResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "settings:write")?;

    let webhooks = queries::list_webhooks(&conn, &ctx.org_id).map_err(internal_error)?;
    Ok(Json(WebhooksResponse { webhooks }))
}

/// `POST /v1/webhooks` — create a new webhook.
/// Admin-only.
pub async fn create_webhook(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    AppJson(req): AppJson<CreateWebhookRequest>,
) -> Result<(StatusCode, Json<Webhook>), (StatusCode, Json<ApiError>)> {
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(bad_request("name must not be empty", "invalid_name"));
    }
    if name.len() > 128 {
        return Err(bad_request(
            "name must be at most 128 characters",
            "invalid_name",
        ));
    }

    let target_url = req.target_url.trim().to_string();
    if target_url.is_empty() {
        return Err(bad_request(
            "target_url must not be empty",
            "invalid_target_url",
        ));
    }
    if !target_url.starts_with("http://") && !target_url.starts_with("https://") {
        return Err(bad_request(
            "target_url must start with http:// or https://",
            "invalid_target_url",
        ));
    }

    let clean_req = CreateWebhookRequest {
        name,
        target_url,
        secret: req.secret,
        events: req.events,
    };

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "settings:write")?;

    let webhook =
        queries::create_webhook(&conn, &ctx.org_id, &clean_req).map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(webhook)))
}

/// `PATCH /v1/webhooks/:id` — update an existing webhook.
/// Admin-only.
pub async fn update_webhook(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(req): AppJson<UpdateWebhookRequest>,
) -> Result<Json<Webhook>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "settings:write")?;

    let updated = queries::update_webhook(&conn, &ctx.org_id, &id, &req)
        .map_err(internal_error)?
        .ok_or_else(not_found)?;

    Ok(Json(updated))
}

/// `DELETE /v1/webhooks/:id` — delete a webhook.
/// Admin-only.
pub async fn delete_webhook(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "settings:write")?;

    let deleted = queries::delete_webhook(&conn, &ctx.org_id, &id).map_err(internal_error)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// `POST /v1/webhooks/:id/test` — send a test payload to the webhook URL.
/// Admin-only. The webhook must be active.
pub async fn test_webhook(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<WebhookTestResult>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    // Scope the lock guard so it's dropped before the await point below.
    // `std::sync::MutexGuard` is `!Send`, so holding it across `.await`
    // would make the future `!Send`, breaking axum's `Handler` bound.
    let webhook = {
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &ctx, None, "settings:write")?;
        queries::get_webhook(&conn, &id, &ctx.org_id)
            .map_err(internal_error)?
            .ok_or_else(not_found)?
    };

    if !webhook.active {
        return Err(bad_request("Webhook is not active", "webhook_not_active"));
    }

    let payload = serde_json::json!({
        "event": "test",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "webhook_id": webhook.id,
        "test": true,
    })
    .to_string();

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| internal_error(anyhow::anyhow!(e)))?;

    let result = client
        .post(&webhook.target_url)
        .header("Content-Type", "application/json")
        .body(payload.clone())
        .send()
        .await;

    let test_result = match result {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let success = (200..300).contains(&status_code);
            WebhookTestResult {
                success,
                status_code: Some(status_code),
                error: if success {
                    None
                } else {
                    Some(format!("Received HTTP {status_code}"))
                },
            }
        }
        Err(e) => WebhookTestResult {
            success: false,
            status_code: None,
            error: Some(e.to_string()),
        },
    };

    // Log the delivery attempt.
    let db = store.conn();
    {
        if let Ok(conn) = db.lock() {
            let _ = queries::log_webhook_delivery(
                &conn,
                &ctx.org_id,
                &webhook.id,
                "test",
                &payload,
                test_result.status_code.map(|c| c as i64),
                test_result.success,
                test_result.error.as_deref(),
            );
        }
    }

    Ok(Json(test_result))
}

// ── Delivery log handler ───────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DeliveriesQuery {
    pub limit: Option<i64>,
}

#[derive(Serialize)]
pub struct DeliveriesResponse {
    pub deliveries: Vec<WebhookDelivery>,
}

/// `GET /v1/webhooks/:id/deliveries?limit=20` — list recent deliveries.
/// Admin-only.
pub async fn list_deliveries(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<String>,
    Query(params): Query<DeliveriesQuery>,
) -> Result<Json<DeliveriesResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &ctx, None, "settings:write")?;

    let limit = params.limit.unwrap_or(20).min(100);
    let deliveries =
        queries::list_webhook_deliveries(&conn, &ctx.org_id, &id, limit).map_err(internal_error)?;

    Ok(Json(DeliveriesResponse { deliveries }))
}

// ── Retry delivery ─────────────────────────────────────────────────────────────

/// `POST /v1/webhooks/deliveries/:delivery_id/retry` — admin-only.
/// Fetches the original delivery, re-fires the HTTP request to the webhook URL,
/// and records a new delivery entry.
pub async fn retry_delivery(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(delivery_id): Path<String>,
) -> Result<Json<RetryDeliveryResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    // Scope the lock guard so it's dropped before the await point below.
    // `std::sync::MutexGuard` is `!Send`, so holding it across `.await`
    // would make the future `!Send`, breaking axum's `Handler` bound.
    let (delivery, webhook) = {
        let conn = db.lock().map_err(|_| lock_err())?;
        require_permission(&conn, &ctx, None, "settings:write")?;

        // Fetch original delivery
        let delivery = queries::get_webhook_delivery(&conn, &ctx.org_id, &delivery_id)
            .map_err(internal_error)?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Delivery not found".to_string(),
                        code: "delivery_not_found".to_string(),
                    }),
                )
            })?;

        // Fetch the associated webhook to get the URL
        let webhook = queries::get_webhook(&conn, &delivery.webhook_id, &ctx.org_id)
            .map_err(internal_error)?
            .ok_or_else(|| {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Webhook not found".to_string(),
                        code: "webhook_not_found".to_string(),
                    }),
                )
            })?;

        (delivery, webhook)
    };

    let payload = delivery.payload.clone();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| internal_error(anyhow::anyhow!(e)))?;

    let result = client
        .post(&webhook.target_url)
        .header("Content-Type", "application/json")
        .body(payload.clone())
        .send()
        .await;

    let (status_code, success, error_msg) = match result {
        Ok(response) => {
            let status_code = response.status().as_u16();
            let success = (200..300).contains(&status_code);
            let err = if success {
                None
            } else {
                Some(format!("Received HTTP {status_code}"))
            };
            (Some(status_code as i64), success, err)
        }
        Err(e) => (None, false, Some(e.to_string())),
    };

    let status_str = if success { "success" } else { "failed" }.to_string();

    // Record new delivery entry
    let db = store.conn();
    if let Ok(conn) = db.lock() {
        let _ = queries::log_webhook_delivery(
            &conn,
            &ctx.org_id,
            &delivery.webhook_id,
            &delivery.event_type,
            &payload,
            status_code,
            success,
            error_msg.as_deref(),
        );
    }

    Ok(Json(RetryDeliveryResponse {
        delivery_id,
        status: status_str,
    }))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{get, patch, post},
        Router,
    };
    use tower::util::ServiceExt;

    use crate::api::middleware as auth_mw;
    use crate::db::queries::bootstrap;
    use crate::db::{connection::connect, migrations};
    use crate::store::sqlite::SqliteStore;

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route(
                "/v1/webhooks",
                get(super::list_webhooks).post(super::create_webhook),
            )
            .route(
                "/v1/webhooks/:id",
                patch(super::update_webhook).delete(super::delete_webhook),
            )
            .route("/v1/webhooks/:id/test", post(super::test_webhook))
            .route("/v1/webhooks/:id/deliveries", get(super::list_deliveries))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn admin_key(store: &SqliteStore) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let (_, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        key
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── list ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn list_returns_empty_initially() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["webhooks"], serde_json::json!([]));
    }

    // ── create ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn create_returns_201_with_webhook() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "my-hook",
            "target_url": "https://example.com/webhook"
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let body = body_json(resp).await;
        assert_eq!(body["name"], "my-hook");
        assert_eq!(body["target_url"], "https://example.com/webhook");
        assert_eq!(body["active"], true);
        assert!(body["id"].is_string());
    }

    #[tokio::test]
    async fn create_with_empty_name_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "",
            "target_url": "https://example.com/webhook"
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "invalid_name");
    }

    #[tokio::test]
    async fn create_with_invalid_url_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({
            "name": "hook",
            "target_url": "ftp://not-valid.com"
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "invalid_target_url");
    }

    // ── update ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn update_active_returns_200() {
        let store = make_store();
        let key = admin_key(&store);

        // Create
        let create_payload = serde_json::json!({
            "name": "hook",
            "target_url": "https://example.com/hook"
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let created = body_json(create_resp).await;
        let id = created["id"].as_str().unwrap().to_string();

        // Update active = false
        let patch_payload = serde_json::json!({ "active": false });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/webhooks/{id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body["active"], false);
    }

    #[tokio::test]
    async fn update_unknown_id_returns_404() {
        let store = make_store();
        let key = admin_key(&store);

        let payload = serde_json::json!({ "active": false });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/webhooks/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn delete_returns_204() {
        let store = make_store();
        let key = admin_key(&store);

        let create_payload = serde_json::json!({
            "name": "to-delete",
            "target_url": "https://example.com/hook"
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let created = body_json(create_resp).await;
        let id = created["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/webhooks/{id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_unknown_id_returns_404() {
        let store = make_store();
        let key = admin_key(&store);

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/webhooks/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── delivery log ─────────────────────────────────────────────────────────

    #[test]
    fn log_delivery_and_list_returns_it() {
        use crate::db::queries::{bootstrap, list_webhook_deliveries, log_webhook_delivery};
        use crate::db::{connection::connect, migrations};

        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, _, _) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();

        // Insert a dummy webhook row directly so we have an ID.
        let wh_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url, events, active, created_at)
             VALUES (?1, ?2, 'hook', 'https://example.com', '[\"*\"]', 1, datetime('now'))",
            rusqlite::params![wh_id, org.id],
        )
        .unwrap();

        log_webhook_delivery(&conn, &org.id, &wh_id, "test", "{}", Some(200), true, None).unwrap();

        let deliveries = list_webhook_deliveries(&conn, &org.id, &wh_id, 20).unwrap();
        assert_eq!(deliveries.len(), 1, "must have exactly 1 delivery");
        assert!(deliveries[0].success, "delivery must be marked success");
        assert_eq!(deliveries[0].status_code, Some(200));
        assert_eq!(deliveries[0].event_type, "test");
    }

    #[test]
    fn list_deliveries_respects_limit() {
        use crate::db::queries::{bootstrap, list_webhook_deliveries, log_webhook_delivery};
        use crate::db::{connection::connect, migrations};

        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        let (org, _, _) = bootstrap(&conn, "Acme2", "acme2", "admin2@acme.com", "Admin2").unwrap();

        let wh_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO webhooks (id, org_id, name, target_url, events, active, created_at)
             VALUES (?1, ?2, 'hook2', 'https://example.com', '[\"*\"]', 1, datetime('now'))",
            rusqlite::params![wh_id, org.id],
        )
        .unwrap();

        // Log 5 deliveries.
        for i in 0..5 {
            log_webhook_delivery(
                &conn,
                &org.id,
                &wh_id,
                "test",
                "{}",
                Some(200 + i),
                true,
                None,
            )
            .unwrap();
        }

        // Limit to 3.
        let deliveries = list_webhook_deliveries(&conn, &org.id, &wh_id, 3).unwrap();
        assert_eq!(
            deliveries.len(),
            3,
            "limit=3 must return exactly 3 deliveries"
        );
    }

    #[tokio::test]
    async fn unauthenticated_returns_401() {
        let store = make_store();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/webhooks")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── test delivery ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_inactive_webhook_returns_400() {
        let store = make_store();
        let key = admin_key(&store);

        // Create a webhook, then deactivate it.
        let create_payload = serde_json::json!({
            "name": "test-hook",
            "target_url": "https://example.com/hook"
        });
        let create_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/webhooks")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::CREATED);
        let created = body_json(create_resp).await;
        let id = created["id"].as_str().unwrap().to_string();

        // Deactivate it.
        let patch_payload = serde_json::json!({ "active": false });
        let patch_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/webhooks/{id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch_payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(patch_resp.status(), StatusCode::OK);

        // Test delivery against inactive webhook → 400.
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/webhooks/{id}/test"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = body_json(resp).await;
        assert_eq!(body["code"], "webhook_not_active");
    }
}
