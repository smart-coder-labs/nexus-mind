use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tower_cookies::Cookies;

use crate::{auth::api_keys, db::queries, models::types::ApiError};

pub async fn accept_json(req: Request<Body>, next: Next) -> Response {
    if let Some(accept) = req.headers().get(axum::http::header::ACCEPT) {
        let accept_str = accept.to_str().unwrap_or("");
        let is_acceptable = accept_str.split(',').any(|media_range| {
            let media_type = media_range.split(';').next().unwrap_or("").trim();
            matches!(media_type, "*/*" | "application/*" | "application/json")
        });
        if !is_acceptable {
            return (
                StatusCode::NOT_ACCEPTABLE,
                Json(serde_json::json!({
                    "error": "Unsupported media type in Accept header",
                    "supported": ["application/json"]
                })),
            )
                .into_response();
        }
    }
    next.run(req).await
}

pub async fn auth(
    cookies: Cookies,
    State(db): State<Arc<Mutex<Connection>>>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, Response> {
    let unauthorized = || {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiError {
                error: "Unauthorized".to_string(),
                code: "unauthorized".to_string(),
            }),
        )
            .into_response()
    };

    // Cookie-first extraction, Bearer fallback
    let token = if let Some(cookie) = cookies.get("nexusmind_session") {
        cookie.value().to_string()
    } else {
        req.headers()
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .map(|s| s.to_string())
            .ok_or_else(unauthorized)?
    };

    let hash = api_keys::hash_key(&token);

    let validate_result = {
        let conn = db.lock().map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError {
                error: "Internal server error".to_string(),
                code: "internal_error".to_string(),
            })).into_response()
        })?;
        queries::validate_api_key(&conn, &hash).map_err(|_| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError {
                error: "Internal server error".to_string(),
                code: "internal_error".to_string(),
            })).into_response()
        })?
    };

    // If key not found through normal validation, check if it's because the account is disabled
    let ctx = match validate_result {
        Some(ctx) => ctx,
        None => {
            // Check if the key exists but the account is disabled
            let is_disabled = {
                let conn = db.lock().map_err(|_| {
                    (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError {
                        error: "Internal server error".to_string(),
                        code: "internal_error".to_string(),
                    })).into_response()
                })?;
                queries::is_key_account_disabled(&conn, &hash).map_err(|_| unauthorized())?
            };
            if is_disabled {
                return Err((
                    StatusCode::UNAUTHORIZED,
                    Json(ApiError {
                        error: "Account disabled".to_string(),
                        code: "account_disabled".to_string(),
                    }),
                )
                    .into_response());
            }
            return Err(unauthorized());
        }
    };

    req.extensions_mut().insert(ctx);
    Ok(next.run(req).await)
}

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

    use crate::db::{connection::connect, migrations, queries as q};

    fn accept_app() -> Router {
        Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(accept_json))
    }

    #[tokio::test]
    async fn no_accept_header_passes_through() {
        let response = accept_app()
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accept_json_passes_through() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accept_wildcard_passes_through() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "*/*")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accept_application_wildcard_passes_through() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "application/*")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accept_xml_returns_406() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "application/xml")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["error"], "Unsupported media type in Accept header");
        assert_eq!(json["supported"], serde_json::json!(["application/json"]));
    }

    #[tokio::test]
    async fn accept_text_html_returns_406() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    }

    #[tokio::test]
    async fn accept_with_quality_factors_passes_when_json_present() {
        // text/html;q=0.9, application/json;q=0.8
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "text/html;q=0.9, application/json;q=0.8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn accept_with_quality_factors_returns_406_when_no_json() {
        let response = accept_app()
            .oneshot(
                Request::builder()
                    .uri("/test")
                    .header("Accept", "text/html;q=0.9, application/xml;q=0.8")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_ACCEPTABLE);
    }

    fn make_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn app(db: Arc<Mutex<Connection>>) -> Router {
        Router::new()
            .route("/protected", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(db.clone(), auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(db)
    }

    #[tokio::test]
    async fn missing_auth_header_returns_401() {
        let db = make_db();
        let response = app(db)
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn invalid_key_returns_401() {
        let db = make_db();
        let response = app(db)
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", "Bearer nm_invalid_key_that_does_not_exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn disabled_account_returns_401_with_account_disabled_code() {
        let db = make_db();

        let raw_key = {
            let conn = db.lock().unwrap();
            let (_org, user, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            // Disable the user
            q::disable_user(&conn, &user.org_id, &user.id).unwrap();
            key
        };

        let response = app(db)
            .oneshot(
                Request::builder()
                    .uri("/protected")
                    .header("Authorization", format!("Bearer {raw_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);

        // Parse body to confirm the error code
        let body = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "account_disabled");
    }
}
