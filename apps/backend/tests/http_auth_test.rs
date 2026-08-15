/// HTTP-level integration tests for cookie-based auth and JSON error handling.
///
/// These tests spin up the full Axum router (with CookieManagerLayer, auth middleware,
/// and all auth routes) against an in-memory SQLite database, then drive it via
/// tower's `oneshot` helper — no network, no port binding.
use axum::{body::Body, http::{Request, StatusCode}};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, migrations, queries},
};
use tower::util::ServiceExt;

// ── Test helpers ──────────────────────────────────────────────────────────────

const ADMIN_EMAIL: &str = "admin@test.com";
const ADMIN_PASSWORD: &str = "testpass1";

/// Build a minimal Config suitable for tests (no SMTP, no superuser key,
/// permissive CORS origin so the router parses without panic).
fn test_config() -> Config {
    Config {
        port: 8080,
        db_path: ":memory:".into(),
        log_level: "error".into(),
        cors_origins: "*".into(),
        superuser_key: None,
        smtp_host: "localhost".into(),
        smtp_port: 587,
        smtp_username: None,
        smtp_password: None,
        smtp_from: None,
        app_base_url: "http://localhost:5173".into(),
        admin_origin: "http://localhost:3000".into(),
        cookie_secure: false,
        github_client_id: None,
        github_client_secret: None,
        github_redirect_uri: None,
        backup_database_url: None,
        backup_interval_hours: 6,
    }
}

/// Bootstrap an in-memory DB with one admin user whose password is set,
/// then hand the connection to `router::build`.
fn app() -> axum::Router {
    let conn = connection::connect(":memory:").expect("in-memory db");
    migrations::run(&conn).expect("migrations");

    let (_, admin, _) = queries::bootstrap(
        &conn,
        "Test Org",
        "test-org",
        ADMIN_EMAIL,
        "Test Admin",
    )
    .expect("bootstrap");

    let hashed = hash_password(ADMIN_PASSWORD).expect("hash");
    queries::set_user_password(&conn, &admin.id, &hashed).expect("set password");

    router::build(conn, test_config())
}

/// Same as [`app`] but with an explicit `cookie_secure` setting, so the tests
/// can assert both sides of the flag.
fn app_with_cookie_secure(cookie_secure: bool) -> axum::Router {
    let conn = connection::connect(":memory:").expect("in-memory db");
    migrations::run(&conn).expect("migrations");

    let (_, admin, _) = queries::bootstrap(
        &conn,
        "Test Org",
        "test-org",
        ADMIN_EMAIL,
        "Test Admin",
    )
    .expect("bootstrap");

    let hashed = hash_password(ADMIN_PASSWORD).expect("hash");
    queries::set_user_password(&conn, &admin.id, &hashed).expect("set password");

    router::build(conn, Config { cookie_secure, ..test_config() })
}

/// Read the raw `Set-Cookie` header off a response.
fn set_cookie_header(response: &axum::response::Response) -> String {
    response
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie header must be present")
        .to_str()
        .expect("Set-Cookie must be ASCII")
        .to_string()
}

/// POST /v1/admin/auth/login with valid credentials and return the full response.
async fn do_login(router: axum::Router) -> axum::response::Response {
    let body = serde_json::json!({
        "email": ADMIN_EMAIL,
        "password": ADMIN_PASSWORD,
    });

    router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// Extract the `nexusmind_session` value from the `Set-Cookie` header of a response.
fn extract_session_cookie(response: &axum::response::Response) -> String {
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie header must be present")
        .to_str()
        .expect("Set-Cookie must be ASCII");

    // Header looks like: nexusmind_session=<value>; Path=/; HttpOnly; SameSite=Lax
    set_cookie
        .split(';')
        .next()
        .expect("at least one segment")
        .trim()
        .strip_prefix("nexusmind_session=")
        .expect("cookie must be named nexusmind_session")
        .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

/// Test 1: login sets an HttpOnly cookie and does NOT leak the raw key in the body.
#[tokio::test]
async fn login_sets_http_only_cookie() {
    let response = do_login(app()).await;

    assert_eq!(response.status(), StatusCode::OK);

    // Set-Cookie header must exist and name the right cookie.
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .expect("Set-Cookie header must be present")
        .to_str()
        .unwrap();

    assert!(
        set_cookie.contains("nexusmind_session="),
        "cookie must be named nexusmind_session, got: {set_cookie}"
    );
    assert!(
        set_cookie.to_lowercase().contains("httponly"),
        "cookie must have HttpOnly attribute, got: {set_cookie}"
    );

    // Body must contain org and user but NOT api_key.
    let body_bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

    assert!(body.get("org").is_some(), "response body must contain 'org'");
    assert!(body.get("user").is_some(), "response body must contain 'user'");
    assert!(
        body.get("api_key").is_none(),
        "response body must NOT contain 'api_key'"
    );
}

/// Test 2: a cookie obtained from login can authenticate GET /v1/admin/auth/me.
#[tokio::test]
async fn cookie_auth_reaches_me() {
    // Login on a fresh app instance and grab the cookie value.
    // We need a single shared store so the session key persists between requests.
    let router = app();

    // Step 1: login — use clone so we can reuse the router.
    let login_resp = do_login(router.clone()).await;
    assert_eq!(login_resp.status(), StatusCode::OK);
    let session_value = extract_session_cookie(&login_resp);

    // Step 2: GET /me with the session cookie.
    let me_resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/auth/me")
                .header("Cookie", format!("nexusmind_session={session_value}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(me_resp.status(), StatusCode::OK);

    let body_bytes = axum::body::to_bytes(me_resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
    assert!(body.get("org").is_some(), "/me must return 'org'");
    assert!(body.get("user").is_some(), "/me must return 'user'");
}

/// Test 3: Bearer token auth still works (backward-compatibility with API-key clients).
#[tokio::test]
async fn bearer_auth_still_works() {
    use nexusmind::db::queries as q;

    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (_, _, raw_key) = q::bootstrap(&conn, "Bearer Org", "bearer-org", "bearer@test.com", "Admin").unwrap();

    let hashed = hash_password(ADMIN_PASSWORD).unwrap();
    // find the admin we just created and set their password (not strictly needed for bearer, but keeps the helper uniform)
    let (user, _) = q::find_admin_by_email(&conn, "bearer@test.com").unwrap().unwrap();
    q::set_user_password(&conn, &user.id, &hashed).unwrap();

    let router = router::build(conn, test_config());

    let resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/auth/me")
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK, "Bearer auth must still return 200");
}

/// Test 4: GET /me with no auth at all must return 401.
#[tokio::test]
async fn me_without_auth_returns_401() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/auth/me")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Build a router + return a bearer API key for an admin of a fresh org.
fn app_with_bearer() -> (axum::Router, String) {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (_, _, raw_key) =
        queries::bootstrap(&conn, "Val Org", "val-org", "val@test.com", "Val Admin").unwrap();
    let router = router::build(conn, test_config());
    (router, raw_key)
}

/// POST /v1/projects with the given JSON body and a Bearer token; return status + body.
async fn post_project(
    router: axum::Router,
    bearer: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let resp = router
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/projects")
                .header("Content-Type", "application/json")
                .header("Authorization", format!("Bearer {bearer}"))
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, json)
}

/// Test 6: POST /v1/projects with an empty name returns 422.
#[tokio::test]
async fn create_project_empty_name_returns_422() {
    let (router, key) = app_with_bearer();
    let (status, body) = post_project(router, &key, serde_json::json!({ "name": "" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "empty name must be 422, body={body}");
    assert_eq!(body["code"], "validation_error");
}

/// Test 7: POST /v1/projects with a whitespace-only name returns 422.
#[tokio::test]
async fn create_project_whitespace_name_returns_422() {
    let (router, key) = app_with_bearer();
    let (status, body) = post_project(router, &key, serde_json::json!({ "name": "   " })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "whitespace-only name must be 422, body={body}");
    assert_eq!(body["code"], "validation_error");
}

/// Test 8: POST /v1/projects with a name containing control characters returns 422.
#[tokio::test]
async fn create_project_control_chars_returns_422() {
    let (router, key) = app_with_bearer();
    let (status, body) = post_project(router, &key, serde_json::json!({ "name": "foo\tbar\n" })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "control-char name must be 422, body={body}");
    assert_eq!(body["code"], "validation_error");
}

/// Test 9: POST /v1/projects with a name longer than 100 characters returns 422.
#[tokio::test]
async fn create_project_name_too_long_returns_422() {
    let (router, key) = app_with_bearer();
    let long_name = "a".repeat(101);
    let (status, body) = post_project(router, &key, serde_json::json!({ "name": long_name })).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "name >100 chars must be 422, body={body}");
    assert_eq!(body["code"], "validation_error");
}

/// Test 10: POST /v1/projects with a valid name returns 201.
#[tokio::test]
async fn create_project_valid_name_returns_201() {
    let (router, key) = app_with_bearer();
    let (status, _body) = post_project(router, &key, serde_json::json!({ "name": "my-project" })).await;
    assert_eq!(status, StatusCode::CREATED, "valid name must return 201");
}

/// Test 5: logout clears the session cookie and revokes the key — subsequent /me returns 401.
#[tokio::test]
async fn logout_clears_cookie_and_revokes_key() {
    let router = app();

    // Step 1: login.
    let login_resp = do_login(router.clone()).await;
    assert_eq!(login_resp.status(), StatusCode::OK);
    let session_value = extract_session_cookie(&login_resp);

    // Step 2: logout with the cookie.
    let logout_resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/logout")
                .header("Cookie", format!("nexusmind_session={session_value}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(logout_resp.status(), StatusCode::NO_CONTENT, "logout must return 204");

    // The logout response must clear the cookie (Max-Age=0 or an expired Expires).
    let clear_cookie = logout_resp
        .headers()
        .get("set-cookie")
        .expect("logout must send Set-Cookie to clear the session")
        .to_str()
        .unwrap();

    assert!(
        clear_cookie.to_lowercase().contains("max-age=0")
            || clear_cookie.to_lowercase().contains("expires="),
        "logout Set-Cookie must clear the session (Max-Age=0 or Expires), got: {clear_cookie}"
    );

    // Step 3: /me with the now-revoked cookie value must return 401.
    let me_resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/admin/auth/me")
                .header("Cookie", format!("nexusmind_session={session_value}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        me_resp.status(),
        StatusCode::UNAUTHORIZED,
        "/me with a revoked cookie must return 401"
    );
}

// ── JSON error response tests ─────────────────────────────────────────────────

/// Malformed JSON body must return 400 with Content-Type: application/json and
/// a structured error body (not plain text).
#[tokio::test]
async fn malformed_json_returns_json_error() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from("{invalid json}"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    let content_type = resp
        .headers()
        .get("content-type")
        .expect("must have Content-Type")
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("application/json"),
        "expected application/json, got: {content_type}"
    );

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("body must be valid JSON");

    assert_eq!(body["code"], "invalid_json");
    assert!(body["error"].is_string());
}

/// Missing Content-Type header must return 415 with Content-Type: application/json
/// and a structured error body (not plain text).
#[tokio::test]
async fn missing_content_type_returns_json_error() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .body(Body::from(r#"{"email":"x","password":"y"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);

    let content_type = resp
        .headers()
        .get("content-type")
        .expect("must have Content-Type")
        .to_str()
        .unwrap();
    assert!(
        content_type.starts_with("application/json"),
        "expected application/json, got: {content_type}"
    );

    let body_bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body_bytes).expect("body must be valid JSON");

    assert_eq!(body["code"], "invalid_content_type");
    assert!(body["error"].is_string());
}

/// Test: DELETE /v1/github/disconnect without auth returns 401.
#[tokio::test]
async fn github_disconnect_without_auth_returns_401() {
    let resp = app()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/github/disconnect")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Test: DELETE /v1/github/disconnect with valid auth returns 204 (idempotent — works even with no connection stored).
#[tokio::test]
async fn github_disconnect_with_auth_returns_204() {
    let (router, raw_key) = app_with_bearer();

    let resp = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/github/disconnect")
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "disconnect must return 204");
}

/// Test: DELETE /v1/github/connection (canonical route) also returns 204 with valid auth.
#[tokio::test]
async fn github_connection_delete_with_auth_returns_204() {
    let (router, raw_key) = app_with_bearer();

    let resp = router
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/github/connection")
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "DELETE /v1/github/connection must return 204");
}

/// Regression test: GET /v1/agents must succeed (not 500 "no such table: agents").
///
/// Bug: `run_all()` in `db/migrations.rs` jumped from `run_v38()` straight to
/// `run_v40()`, never invoking `run_v39()` — so the `agents` / `agent_assignments`
/// tables were never created despite `run_v39` being fully implemented. Every
/// `/v1/agents*` route returned HTTP 500. Fixed by restoring `run_v39()` to its
/// normal position in `run_all()` and adding a `run_v45()` backfill migration for
/// databases that already reached `user_version = 44` without the agents tables.
#[tokio::test]
async fn list_agents_returns_200_not_500() {
    let (router, raw_key) = app_with_bearer();

    let resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/agents")
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "GET /v1/agents must return 200, not 500 'no such table: agents'"
    );
}

/// Regression test: DELETE /v1/code/projects/:id must delete by the numeric id in
/// the path, not by matching a project's `name` column.
///
/// Bug: `delete_project`'s handler extracted the path segment as a `name: String`
/// and called `delete_code_project(conn, org_id, name)`, which ran
/// `DELETE FROM code_projects WHERE org_id = ?1 AND name = ?2`. Deleting by the id
/// returned from `GET /v1/code/projects` therefore always failed (404, nothing
/// deleted) unless a project happened to be named exactly that numeric string — in
/// which case it deleted the WRONG project. Fixed by extracting `Path<i64>` and
/// matching on the `id` column, consistent with the sibling archive/restore routes.
#[tokio::test]
async fn delete_code_project_by_id_removes_only_target() {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let (org, admin, raw_key) =
        queries::bootstrap(&conn, "Code Org", "code-org", "code@test.com", "Admin").unwrap();
    conn.execute("UPDATE users SET role = 'super_user' WHERE id = ?1", [&admin.id]).unwrap();

    let id_alpha = queries::upsert_code_project(&conn, &org.id, "alpha", "/ws/alpha").unwrap();
    let id_beta = queries::upsert_code_project(&conn, &org.id, "beta", "/ws/beta").unwrap();

    let router = router::build(conn, test_config());

    let resp = router
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/code/projects/{id_alpha}"))
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "DELETE by numeric id must succeed");

    let list_resp = router
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/code/projects")
                .header("Authorization", format!("Bearer {raw_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(list_resp.into_body(), usize::MAX).await.unwrap();
    let remaining: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let remaining = remaining.as_array().expect("list must be an array");
    let id_alpha_str = id_alpha.to_string();
    let id_beta_str = id_beta.to_string();

    assert!(
        remaining.iter().all(|p| p["id"].as_str() != Some(id_alpha_str.as_str())),
        "alpha (deleted id) must be gone: {remaining:?}"
    );
    assert!(
        remaining.iter().any(|p| p["id"].as_str() == Some(id_beta_str.as_str())),
        "beta (untouched id) must still be present: {remaining:?}"
    );
}

// ── COOKIE_SECURE ─────────────────────────────────────────────────────────────
//
// A `Secure` cookie is silently DROPPED by the browser on an insecure origin,
// so a deployment served over plain HTTP would return 200 from login and then
// bounce the user straight back to /login. `cookie_secure` is the escape hatch;
// these tests pin both sides of it so the flag cannot quietly stop working.

#[tokio::test]
async fn login_cookie_sets_secure_when_cookie_secure_is_true() {
    let response = do_login(app_with_cookie_secure(true)).await;
    assert_eq!(response.status(), StatusCode::OK);

    let set_cookie = set_cookie_header(&response);
    assert!(
        set_cookie.to_lowercase().contains("secure"),
        "cookie must carry Secure when cookie_secure=true, got: {set_cookie}"
    );
}

#[tokio::test]
async fn login_cookie_omits_secure_when_cookie_secure_is_false() {
    let response = do_login(app_with_cookie_secure(false)).await;
    assert_eq!(response.status(), StatusCode::OK);

    let set_cookie = set_cookie_header(&response);
    assert!(
        !set_cookie.to_lowercase().contains("secure"),
        "cookie must NOT carry Secure when cookie_secure=false (it would be \
         dropped over plain HTTP), got: {set_cookie}"
    );
    // The other hardening attributes must survive the escape hatch.
    assert!(
        set_cookie.to_lowercase().contains("httponly"),
        "HttpOnly must still be set, got: {set_cookie}"
    );
    assert!(
        set_cookie.contains("SameSite=Lax"),
        "SameSite=Lax must still be set, got: {set_cookie}"
    );
}

#[tokio::test]
async fn logout_removal_cookie_mirrors_cookie_secure() {
    // A removal cookie carrying `Secure` is itself dropped on an insecure
    // origin, which would leave the session cookie in place and make logout a
    // silent no-op. The removal must mirror however the cookie was set.
    for secure in [true, false] {
        let router = app_with_cookie_secure(secure);
        let session = extract_session_cookie(&do_login(router.clone()).await);

        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/auth/logout")
                    .header("Cookie", format!("nexusmind_session={session}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let set_cookie = set_cookie_header(&response);
        assert_eq!(
            set_cookie.to_lowercase().contains("secure"),
            secure,
            "logout removal cookie must mirror cookie_secure={secure}, got: {set_cookie}"
        );
    }
}
