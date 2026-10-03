//! HTTP-level tests for the factory F3 shadow endpoints: the per-class OD-5
//! report, the decision list, and a person's label, behind the explicit
//! `factory_policy:*` permissions.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, factory_queries, migrations, queries},
    factory::jev,
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
    let path =
        std::env::temp_dir().join(format!("factory-shadow-{}.db", uuid::Uuid::new_v4()));
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

async fn post(router: &axum::Router, cookie: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("Cookie", cookie)
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}


async fn get(router: &axum::Router, cookie: &str, uri: &str) -> (StatusCode, Value) {
    let response = router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header("Cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Two `allow` docs decisions with known outcomes, one of them a false low.
fn seed(path: &std::path::Path) -> String {
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let org_id: String = conn
        .query_row("SELECT org_id FROM users WHERE email = ?1", [EMAIL], |r| r.get(0))
        .unwrap();
    let answer = jev::Answer {
        model: Some("jev-1.13.0".into()),
        task_class: "docs".into(),
        class_confidence: 1.0,
        risk: 0.0,
        risk_confidence: 0.95,
        needs_human: Some(0.4),
        input_tokens: Some(640),
    };
    let mut ids = Vec::new();
    for (n, outcome) in [(1, "clean"), (2, "high_risk")] {
        let head = format!("{n:040}");
        factory_queries::record_shadow_decision(
            &conn,
            &org_id,
            &factory_queries::ShadowDecision {
                provider: "jev",
                repository: "acme/app",
                pull_number: n,
                head_sha: &head,
                answer: &answer,
                verdict: jev::verdict(&answer),
                floor: None,
                latency_ms: 300,
            },
        )
        .unwrap();
        let id = factory_queries::shadow_decision_id(&conn, &org_id, "acme/app", n, &head)
            .unwrap()
            .unwrap();
        factory_queries::set_shadow_outcome(&conn, &org_id, &id, Some("2026-09-01T00:00:00Z"), None, outcome, &["merge_commit_ci_failed".to_string()]).unwrap();
        ids.push(id);
    }
    ids[1].clone()
}

#[tokio::test]
async fn the_report_counts_false_lows_and_a_label_overrides_the_signals() {
    let (router, path) = app("super_user");
    let flaky = seed(&path);
    let cookie = login(&router).await;

    let (status, body) = get(&router, &cookie, "/v1/factory/shadow/report").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["min_settled_allows"], 50);
    let docs = &body["classes"][0];
    assert_eq!((docs["settled_allowed"].as_i64(), docs["false_low"].as_i64()), (Some(2), Some(1)));
    assert_eq!(docs["meets_od5"], false);

    let (status, body) = get(&router, &cookie, "/v1/factory/shadow/decisions?limit=10").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().unwrap().len(), 2);
    assert_eq!(body[0]["outcome_signals"][0], "merge_commit_ci_failed");

    // A person says the red CI was flaky: the change was not risky.
    let (status, _) = post(&router, &cookie, &format!("/v1/factory/shadow/decisions/{flaky}/label"), json!({"label": "low"})).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, body) = get(&router, &cookie, "/v1/factory/shadow/report").await;
    assert_eq!(body["classes"][0]["false_low"], 0);

    let (status, _) = post(&router, &cookie, &format!("/v1/factory/shadow/decisions/{flaky}/label"), json!({"label": "maybe"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = post(&router, &cookie, "/v1/factory/shadow/decisions/nope/label", json!({"label": "high"})).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn shadow_endpoints_need_the_explicit_factory_permission() {
    let (router, path) = app("admin");
    seed(&path);
    let cookie = login(&router).await;
    let (status, _) = get(&router, &cookie, "/v1/factory/shadow/report").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = get(&router, &cookie, "/v1/factory/shadow/decisions").await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}
