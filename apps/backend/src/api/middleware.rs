use axum::{
    body::Body,
    extract::{MatchedPath, State},
    http::{Method, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};
use tower_cookies::Cookies;

use crate::{
    auth::api_keys,
    db::queries,
    models::types::{ApiError, AuthContext},
};

pub async fn accept_json(req: Request<Body>, next: Next) -> Response {
    if let Some(accept) = req.headers().get(axum::http::header::ACCEPT) {
        let accept_str = accept.to_str().unwrap_or("");
        let is_acceptable = accept_str.split(',').any(|media_range| {
            let media_type = media_range.split(';').next().unwrap_or("").trim();
            // `image/*` is allowed so the public /evidence redirect works for
            // <img> tags and GitHub's camo image proxy (which sends `Accept: image/*`).
            matches!(
                media_type,
                "*/*" | "application/*" | "application/json" | "image/*" | "image/png"
            )
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

// The Err variant is a full axum `Response` by design (the middleware short-circuits
// with a ready response); boxing it would complicate every `?` call site.
#[allow(clippy::result_large_err)]
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
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Internal server error".to_string(),
                    code: "internal_error".to_string(),
                }),
            )
                .into_response()
        })?;
        queries::validate_api_key(&conn, &hash).map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Internal server error".to_string(),
                    code: "internal_error".to_string(),
                }),
            )
                .into_response()
        })?
    };

    // If key not found through normal validation, check if it's because the account is disabled
    let ctx = match validate_result {
        Some(ctx) => ctx,
        None => {
            // Check if the key exists but the account is disabled
            let is_disabled = {
                let conn = db.lock().map_err(|_| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ApiError {
                            error: "Internal server error".to_string(),
                            code: "internal_error".to_string(),
                        }),
                    )
                        .into_response()
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

/// Route patterns whose handlers already write their own (often richer) audit
/// event. The blanket [`audit`] layer skips these to avoid double-logging.
const AUDIT_SKIP_PATTERNS: &[&str] = &[
    "/v1/memory/store",
    "/v1/memory/search",
    "/v1/memory/:id",
    "/v1/users/invite",
    "/v1/users/:id",
    "/v1/users/:id/rotate-key",
    "/v1/users/:id/role",
    "/v1/roles",
    "/v1/roles/:id",
    "/v1/admin/keys",
    "/v1/admin/keys/:key_id",
    "/v1/admin/keys/:key_id/rotate",
    "/v1/admin/keys/:key_id/revoke",
    "/v1/admin/users/:user_id/disable",
    "/v1/admin/users/:user_id/enable",
    "/v1/admin/users/:user_id/reset-key",
    "/v1/admin/users/:id/note",
    "/v1/admin/import",
];

/// Blanket audit layer: records an audit entry for every successful mutating
/// request whose handler does not already self-log (see [`AUDIT_SKIP_PATTERNS`]).
/// Must run inside the auth layer so the [`AuthContext`] is present.
pub async fn audit(
    State(db): State<Arc<Mutex<Connection>>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().clone();
    let is_mutating = matches!(
        method,
        Method::POST | Method::PUT | Method::PATCH | Method::DELETE
    );
    let auth = req.extensions().get::<AuthContext>().cloned();
    let pattern = req
        .extensions()
        .get::<MatchedPath>()
        .map(|m| m.as_str().to_string());
    let actual_path = req.uri().path().to_string();

    let resp = next.run(req).await;

    if is_mutating && resp.status().is_success() {
        if let (Some(ctx), Some(pat)) = (auth, pattern.as_deref()) {
            if !AUDIT_SKIP_PATTERNS.contains(&pat) {
                let (action, resource, resource_id) = derive_audit(&method, pat, &actual_path);
                if let Ok(conn) = db.lock() {
                    let _ = queries::log_audit(
                        &conn,
                        &ctx.org_id,
                        &ctx.user_id,
                        &action,
                        &resource,
                        resource_id.as_deref(),
                        serde_json::json!({ "method": method.as_str(), "path": actual_path }),
                    );
                }
            }
        }
    }
    resp
}

/// Derives `(action, resource_type, resource_id)` from a route pattern and the
/// concrete path. e.g. `POST /v1/conventions/:id/archive`
/// → `("convention.archive", "convention", Some(id))`.
fn derive_audit(method: &Method, pattern: &str, actual: &str) -> (String, String, Option<String>) {
    let pat_segs: Vec<&str> = pattern.trim_matches('/').split('/').collect();
    let act_segs: Vec<&str> = actual.trim_matches('/').split('/').collect();

    // resource_id = value of the first path parameter (`:id`, `:user_id`, …)
    let resource_id = pat_segs
        .iter()
        .position(|s| s.starts_with(':'))
        .and_then(|i| act_segs.get(i))
        .map(|s| (*s).to_string());

    // Meaningful segments = literals, minus routing/namespace prefixes.
    let meaningful: Vec<&str> = pat_segs
        .iter()
        .copied()
        .filter(|s| !s.starts_with(':') && !matches!(*s, "v1" | "internal" | "admin"))
        .collect();

    let resource = meaningful
        .first()
        .map(|s| singularize(s))
        .unwrap_or_else(|| "resource".to_string());

    // If the last literal segment names an explicit action, use it as the verb.
    const VERB_SEGMENTS: &[&str] = &[
        "archive",
        "restore",
        "reindex",
        "index",
        "disable",
        "enable",
        "rotate",
        "revoke",
        "reset-key",
        "test",
        "retry",
        "impersonate",
        "suspend",
        "redeem",
        "merge",
        "import",
        "bulk-tag",
        "rename",
        "logout",
        "login",
        "change-password",
        "set-password",
        "request-reset",
        "forgot-password",
        "reset-password",
        "mark-all-read",
        "disconnect",
        "callback",
    ];
    let last = meaningful.last().copied().unwrap_or("");
    let action = if meaningful.len() > 1 && VERB_SEGMENTS.contains(&last) {
        format!("{resource}.{}", last.replace('-', "_"))
    } else {
        let verb = match *method {
            Method::POST => "created",
            Method::PATCH | Method::PUT => "updated",
            Method::DELETE => "deleted",
            _ => "changed",
        };
        format!("{resource}.{verb}")
    };
    (action, resource, resource_id)
}

fn singularize(s: &str) -> String {
    match s {
        "policies" => "policy".to_string(),
        "memories" | "memory" => "memory".to_string(),
        "settings" => "settings".to_string(),
        other => other.strip_suffix('s').unwrap_or(other).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_audit_maps_routes_to_actions() {
        let cases = [
            (
                Method::POST,
                "/v1/policies",
                "/v1/policies",
                "policy.created",
                "policy",
                None,
            ),
            (
                Method::DELETE,
                "/v1/webhooks/:id",
                "/v1/webhooks/wh_1",
                "webhook.deleted",
                "webhook",
                Some("wh_1"),
            ),
            (
                Method::POST,
                "/v1/conventions/:id/archive",
                "/v1/conventions/c9/archive",
                "convention.archive",
                "convention",
                Some("c9"),
            ),
            (
                Method::POST,
                "/v1/sessions",
                "/v1/sessions",
                "session.created",
                "session",
                None,
            ),
            (
                Method::POST,
                "/v1/code/projects/:id/reindex",
                "/v1/code/projects/p2/reindex",
                "code.reindex",
                "code",
                Some("p2"),
            ),
            (
                Method::PATCH,
                "/v1/admin/org",
                "/v1/admin/org",
                "org.updated",
                "org",
                None,
            ),
        ];
        for (method, pattern, actual, action, resource, id) in cases {
            let (a, r, rid) = derive_audit(&method, pattern, actual);
            assert_eq!(a, action, "action for {pattern}");
            assert_eq!(r, resource, "resource for {pattern}");
            assert_eq!(rid.as_deref(), id, "resource_id for {pattern}");
        }
    }
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
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
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
            let (_org, user, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
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
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "account_disabled");
    }
}
