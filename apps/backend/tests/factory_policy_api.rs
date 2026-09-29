//! HTTP-level tests for the factory policy API (`/v1/factory/policies`): the
//! permission is explicit (no admin-role bypass), writes are version-checked,
//! and every write is audited.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, migrations, queries},
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

const EMAIL: &str = "owner@test.com";
const PASSWORD: &str = "testpass1";

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
        autonomous_agents_enabled: false,
        claude_code_bin: "/usr/local/bin/claude".into(),
        nexus_worker_bin: "/app/nexus".into(),
        claude_code_probe_interval_seconds: 300,
        autonomous_agent_poll_seconds: 15,
    }
}

/// A router over a file-backed DB (so the test can inspect it afterwards) with one
/// user of the given role.
fn app(role: &str) -> (axum::Router, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("factory-policy-{}.db", uuid::Uuid::new_v4()));
    let conn = connection::connect(path.to_str().unwrap()).expect("db");
    migrations::run(&conn).expect("migrations");
    let (_, user, _) =
        queries::bootstrap(&conn, "Test Org", "test-org", EMAIL, "Owner").expect("bootstrap");
    queries::set_user_password(&conn, &user.id, &hash_password(PASSWORD).unwrap()).unwrap();
    conn.execute("UPDATE users SET role = ?1 WHERE id = ?2", [role, &user.id])
        .unwrap();
    (router::build(conn, test_config()), path)
}

async fn login(router: &axum::Router) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"email": EMAIL, "password": PASSWORD}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    cookie.split(';').next().unwrap().trim().to_string()
}

async fn call(
    router: &axum::Router,
    cookie: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header("Cookie", cookie);
    if body.is_some() {
        request = request.header("Content-Type", "application/json");
    }
    let response = router
        .clone()
        .oneshot(
            request
                .body(body.map_or(Body::empty(), |b| Body::from(b.to_string())))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

fn merge_policy(mode: &str, version: u32) -> Value {
    json!({
        "schema_version": 1,
        "action": "merge",
        "mode": mode,
        "scope": {"task_class": "docs"},
        "allow": ["docs/tests paths only"],
        "stop": [],
        "version": version
    })
}

#[tokio::test]
async fn admin_role_alone_cannot_read_or_write_policies() {
    let (router, _) = app("admin");
    let cookie = login(&router).await;
    let (status, _) = call(&router, &cookie, "GET", "/v1/factory/policies", None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = call(
        &router,
        &cookie,
        "PUT",
        "/v1/factory/policies",
        Some(merge_policy("criteria", 1)),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn super_user_manages_policies_with_version_checks_and_audit() {
    let (router, path) = app("super_user");
    let cookie = login(&router).await;

    let (status, body) = call(&router, &cookie, "GET", "/v1/factory/policies", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!([]));

    let (status, created) = call(
        &router,
        &cookie,
        "PUT",
        "/v1/factory/policies",
        Some(merge_policy("manual", 1)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    assert_eq!(created["mode"], "manual");
    let id = created["id"].as_str().unwrap().to_string();

    // Replaying version 1 is a stale write.
    let (status, error) = call(
        &router,
        &cookie,
        "PUT",
        "/v1/factory/policies",
        Some(merge_policy("criteria", 1)),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(error["code"], "policy_version_conflict");

    let (status, updated) = call(
        &router,
        &cookie,
        "PUT",
        "/v1/factory/policies",
        Some(merge_policy("criteria", 2)),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["id"], id.as_str());

    // criteria without allow is invalid.
    let mut invalid = merge_policy("criteria", 3);
    invalid["allow"] = json!([]);
    let (status, error) = call(
        &router,
        &cookie,
        "PUT",
        "/v1/factory/policies",
        Some(invalid),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(error["code"], "invalid_policy");

    let (status, _) = call(
        &router,
        &cookie,
        "DELETE",
        &format!("/v1/factory/policies/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = call(
        &router,
        &cookie,
        "DELETE",
        &format!("/v1/factory/policies/{id}"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let audited: Vec<String> = conn
        .prepare(
            "SELECT action FROM audit_logs WHERE resource_type = 'factory_policy' ORDER BY rowid",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(
        audited,
        vec![
            "factory_policy.upsert",
            "factory_policy.upsert",
            "factory_policy.delete"
        ]
    );
    let _ = std::fs::remove_file(path);
}
