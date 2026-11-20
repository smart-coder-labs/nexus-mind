//! HTTP tests for runtime error monitoring: sources and groups never cross
//! organizations, and the public push endpoint rejects unsigned requests.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, migrations, monitor, queries},
    monitor::RuntimeError,
};
use serde_json::{json, Value};
use tower::util::ServiceExt;

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

/// Two organizations with one admin each, over a file-backed DB.
fn app() -> (axum::Router, std::path::PathBuf) {
    let path = std::env::temp_dir().join(format!("monitor-{}.db", uuid::Uuid::new_v4()));
    let conn = connection::connect(path.to_str().unwrap()).expect("db");
    migrations::run(&conn).expect("migrations");
    for (org, slug, email) in [("Org A", "org-a", "a@test.com"), ("Org B", "org-b", "b@test.com")] {
        let (_, user, _) = queries::create_org(&conn, org, slug, email, "Admin").expect("create org");
        queries::set_user_password(&conn, &user.id, &hash_password(PASSWORD).unwrap()).unwrap();
        conn.execute("UPDATE users SET role = 'admin' WHERE id = ?1", [&user.id]).unwrap();
    }
    (router::build(conn, test_config()), path)
}

async fn login(router: &axum::Router, email: &str) -> String {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/admin/auth/login")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"email": email, "password": PASSWORD}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()["set-cookie"].to_str().unwrap();
    cookie.split(';').next().unwrap().trim().to_string()
}

async fn send(router: &axum::Router, cookie: &str, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut request = Request::builder().method(method).uri(uri).header("Cookie", cookie);
    if body.is_some() {
        request = request.header("Content-Type", "application/json");
    }
    let response = router
        .clone()
        .oneshot(request.body(body.map_or_else(Body::empty, |b| Body::from(b.to_string()))).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

fn source_body(name: &str) -> Value {
    json!({
        "project": "kasymir",
        "name": name,
        "service_account": "nexusmind-push@my-proj-1.iam.gserviceaccount.com",
        "throttle_minutes": 30
    })
}

#[tokio::test]
async fn sources_and_groups_never_cross_organizations() {
    let (router, path) = app();
    let a = login(&router, "a@test.com").await;
    let b = login(&router, "b@test.com").await;

    let (status, source) = send(&router, &a, "POST", "/v1/monitor/sources", Some(source_body("gcp"))).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    let id = source["id"].as_str().unwrap().to_string();
    assert_eq!(source["audience"], format!("nexusmind-monitor:{id}"));

    // One error group for org A, written the way the push endpoint does.
    let group_id = {
        let conn = connection::connect(path.to_str().unwrap()).unwrap();
        let stored = monitor::get_source_by_id(&conn, &id).unwrap().unwrap();
        let error = RuntimeError {
            service: "sales".into(),
            severity: "ERROR".into(),
            title: "boom".into(),
            message: "boom".into(),
            stack: None,
            log_url: None,
            group_hint: None,
        };
        monitor::record(&conn, &stored, "fp", &error, chrono::Utc::now()).unwrap().1.id
    };

    let (_, listed) = send(&router, &b, "GET", "/v1/monitor/sources", None).await;
    assert_eq!(listed, json!([]));
    let (_, groups) = send(&router, &b, "GET", "/v1/monitor/groups", None).await;
    assert_eq!(groups, json!([]));
    let (_, groups) = send(&router, &b, "GET", &format!("/v1/monitor/groups?source_id={id}"), None).await;
    assert_eq!(groups, json!([]));
    let (status, _) = send(&router, &b, "PUT", &format!("/v1/monitor/sources/{id}"), Some(source_body("stolen"))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&router, &b, "POST", &format!("/v1/monitor/sources/{id}/test"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&router, &b, "PATCH", &format!("/v1/monitor/groups/{group_id}"), Some(json!({"status": "muted"}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = send(&router, &b, "DELETE", &format!("/v1/monitor/sources/{id}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, groups) = send(&router, &a, "GET", "/v1/monitor/groups", None).await;
    assert_eq!(groups[0]["status"], "open");
    assert_eq!(groups[0]["count"], 1);
    let (status, _) = send(&router, &a, "PATCH", &format!("/v1/monitor/groups/{group_id}"), Some(json!({"status": "resolved"}))).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&router, &a, "DELETE", &format!("/v1/monitor/sources/{id}"), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sources_are_validated() {
    let (router, path) = app();
    let a = login(&router, "a@test.com").await;
    let mut body = source_body("gcp");
    body["service_account"] = json!("me@gmail.com");
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut body = source_body("gcp");
    body["slack_connector_id"] = json!("not-a-connector");
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let mut body = source_body("gcp");
    body["github_repo"] = json!("acme/app");
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "a repo needs a GitHub connector");
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(source_body("gcp"))).await;
    assert_eq!(status, StatusCode::CREATED);
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(source_body("gcp"))).await;
    assert_eq!(status, StatusCode::CONFLICT);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn the_push_endpoint_requires_a_known_source_and_a_token() {
    let (router, path) = app();
    let a = login(&router, "a@test.com").await;
    let (_, source) = send(&router, &a, "POST", "/v1/monitor/sources", Some(source_body("gcp"))).await;
    let id = source["id"].as_str().unwrap();
    let push = |uri: String, auth: Option<&'static str>| {
        let mut request = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
        if let Some(auth) = auth {
            request = request.header("Authorization", auth);
        }
        request.body(Body::from(json!({"message": {"data": ""}}).to_string())).unwrap()
    };
    let response = router.clone().oneshot(push(format!("/v1/monitor/gcp/{id}"), None)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = router.clone().oneshot(push(format!("/v1/monitor/gcp/{id}"), Some("Basic abc"))).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let response = router.clone().oneshot(push("/v1/monitor/gcp/unknown".into(), Some("Bearer x.y.z"))).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let _ = std::fs::remove_file(path);
}
