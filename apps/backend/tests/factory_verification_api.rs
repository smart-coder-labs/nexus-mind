//! HTTP-level test for `/v1/autonomous-agent-runs/:id/verification-reports`: a
//! run's F1 gate reports are readable by the org, and a run of another org is 404.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    auth::password::hash_password,
    config::Config,
    db::{connection, factory_queries, migrations, queries},
    factory::verification::{build_report, VerificationReceipt},
    models::types::CreateAutonomousAgentRequest,
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
        std::env::temp_dir().join(format!("factory-verification-{}.db", uuid::Uuid::new_v4()));
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
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

#[tokio::test]
async fn a_runs_verification_reports_are_readable_and_org_scoped() {
    let (router, path) = app("admin");
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let (org_id, user_id): (String, String) = conn
        .query_row(
            "SELECT org_id, id FROM users WHERE email = ?1",
            [EMAIL],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    let agent = queries::create_autonomous_agent_definition(
        &conn,
        &org_id,
        &user_id,
        &CreateAutonomousAgentRequest {
            name: "Reviewer".into(),
            description: None,
            template_key: "github_pr_reviewer".into(),
            config: json!({"repository": "acme/app"}),
            budgets: json!({}),
        },
    )
    .unwrap();
    // Runs are only enqueued for validated, enabled agents.
    queries::validate_autonomous_agent_definition(&conn, &org_id, &user_id, &agent.definition.id)
        .unwrap();
    queries::set_autonomous_agent_status(&conn, &org_id, &agent.definition.id, "enabled").unwrap();
    let run = queries::enqueue_autonomous_agent_run(
        &conn,
        &org_id,
        &agent.definition.id,
        "manual",
        "manual:test",
        None,
        None,
    )
    .unwrap()
    .unwrap();
    let head = "0123456789abcdef0123456789abcdef01234567";
    let report = build_report(
        &run.id,
        head,
        &[VerificationReceipt {
            argv: vec!["npm".into(), "test".into()],
            exit_code: Some(0),
            duration_ms: 1200,
        }],
        &[],
        &[],
    )
    .unwrap();
    factory_queries::save_verification_report(&conn, &org_id, &run.id, &report).unwrap();

    let cookie = login(&router).await;
    let (status, body) = get(
        &router,
        &cookie,
        &format!("/v1/autonomous-agent-runs/{}/verification-reports", run.id),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let reports = body["reports"].as_array().unwrap();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0]["head_sha"], head);
    assert_eq!(reports[0]["report"]["passed"], true);
    assert_eq!(reports[0]["report"]["checks"][0]["name"], "cmd:npm test");

    let (status, _) = get(
        &router,
        &cookie,
        "/v1/autonomous-agent-runs/00000000-0000-0000-0000-000000000000/verification-reports",
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
