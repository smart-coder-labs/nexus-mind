//! HTTP-level tests for the factory F3 operator endpoints: the human digest,
//! economics, a person's decision on one merge, and submitting a factory task.

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
    let path =
        std::env::temp_dir().join(format!("factory-operator-{}.db", uuid::Uuid::new_v4()));
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

const SUBJECT: &str = "acme/app#7@0123456789abcdef0123456789abcdef01234567";

#[tokio::test]
async fn digest_and_economics_need_the_factory_permission() {
    let (router, _) = app("admin");
    let cookie = login(&router).await;
    assert_eq!(get(&router, &cookie, "/v1/factory/digest").await.0, StatusCode::FORBIDDEN);
    assert_eq!(get(&router, &cookie, "/v1/factory/economics").await.0, StatusCode::FORBIDDEN);
    let (status, _) = post(&router, &cookie, "/v1/factory/decisions", json!({"subject": SUBJECT, "action": "merge", "approve": true})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

fn hold(path: &std::path::Path, subject: &str) -> String {
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let org_id: String = conn.query_row("SELECT org_id FROM users WHERE email = ?1", [EMAIL], |r| r.get(0)).unwrap();
    use nexusmind::factory::contracts::{Action, ActionVerdict, Verdict, VerdictSource};
    nexusmind::db::factory_queries::record_decision(
        &conn,
        &org_id,
        &nexusmind::db::factory_queries::DecisionRecord {
            subject: subject.into(),
            action: Action::Merge,
            verdict: ActionVerdict { verdict: Verdict::Hold, reason: "decision_model_not_configured".into(), source: VerdictSource::Policy },
            policy_version: Some(1),
            provider: None,
            model: None,
            confidence: None,
            inputs: json!({"run_id": "run-1", "required_checks": ["build"]}),
        },
    )
    .unwrap();
    org_id
}

#[tokio::test]
async fn approving_a_held_merge_starts_its_soak_and_rejecting_cancels_it() {
    let (router, path) = app("super_user");
    let cookie = login(&router).await;
    // Nothing held: nothing to approve.
    let (status, body) = post(&router, &cookie, "/v1/factory/decisions", json!({"subject": SUBJECT, "action": "merge", "approve": true})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "nothing_held");

    let org_id = hold(&path, SUBJECT);
    let (_, digest) = get(&router, &cookie, "/v1/factory/digest").await;
    assert_eq!(digest["held_merges"][0]["subject"], SUBJECT);

    for bad in [
        json!({"subject": "acme/app#7@abc", "action": "merge", "approve": true}),
        json!({"subject": SUBJECT, "action": "deploy", "approve": true}),
    ] {
        let (status, _) = post(&router, &cookie, "/v1/factory/decisions", bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (status, body) = post(&router, &cookie, "/v1/factory/decisions", json!({"subject": SUBJECT, "action": "merge", "approve": true, "reason": "checked the diff"})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert!(body["merges_after"].is_string(), "{body}");
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let soaks: i64 = conn.query_row("SELECT COUNT(*) FROM factory_merge_soaks WHERE org_id = ?1 AND pull_number = 7", [&org_id], |r| r.get(0)).unwrap();
    assert_eq!(soaks, 1);
    let (_, digest) = get(&router, &cookie, "/v1/factory/digest").await;
    assert_eq!(digest["held_merges"], json!([]));
    assert_eq!(digest["approved_merges"][0]["subject"], SUBJECT);
    assert!(digest["approved_merges"][0]["merges_after"].is_string());

    // Rejecting cancels the pending soak.
    let (status, _) = post(&router, &cookie, "/v1/factory/decisions", json!({"subject": SUBJECT, "action": "merge", "approve": false})).await;
    assert_eq!(status, StatusCode::CREATED);
    let soaks: i64 = conn.query_row("SELECT COUNT(*) FROM factory_merge_soaks WHERE org_id = ?1", [&org_id], |r| r.get(0)).unwrap();
    assert_eq!(soaks, 0);

    let (status, body) = get(&router, &cookie, "/v1/factory/economics?days=7").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["days"], 7);
    assert_eq!(body["accepted_changes_tracked"], false);
}

#[tokio::test]
async fn a_submitted_factory_task_is_a_labelled_backlog_task() {
    let (router, _) = app("super_user");
    let cookie = login(&router).await;
    let (status, body) = post(&router, &cookie, "/v1/factory/tasks", json!({"project": "app", "title": "Document the refund flow", "task_class": "docs"})).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["status"], "backlog");
    let mut labels: Vec<String> = serde_json::from_value(body["labels"].clone()).unwrap();
    labels.sort();
    assert_eq!(labels, ["docs", "factory"]);
    let (status, _) = post(&router, &cookie, "/v1/factory/tasks", json!({"project": "app", "title": "x", "task_class": "poetry"})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = post(&router, &cookie, "/v1/factory/tasks", json!({"project": "app", "title": "  "})).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, body) = get(&router, &cookie, "/v1/tasks?label=factory").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body.as_array().unwrap().len(), 1);
    let (_, digest) = get(&router, &cookie, "/v1/factory/digest").await;
    assert_eq!(digest["factory_tasks"][0]["title"], "Document the refund flow");
}
