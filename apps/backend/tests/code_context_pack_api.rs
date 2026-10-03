//! HTTP-level tests for `/v1/code/context-pack` and the BM25 `/v1/code/locate`
//! (factory F2): a pack carries ranked code with reasons, hashes and bodies, is
//! pinned to a commit, and rejects malformed input.

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
        std::env::temp_dir().join(format!("code-context-pack-{}.db", uuid::Uuid::new_v4()));
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

/// Indexes a small repository (no embedder: BM25 needs none) into the app's DB.
fn index_repo(path: &std::path::Path) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::create_dir_all(root.join("src/pages")).unwrap();
    std::fs::create_dir_all(root.join("src/api")).unwrap();
    std::fs::write(
        root.join("src/pages/SaleDetail.tsx"),
        "import { createRefund } from '../api/refunds';\nexport function SaleDetail() { return createRefund(); }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/api/refunds.ts"),
        "export async function createRefund(id: string) { return post('/refunds', id); }\n",
    )
    .unwrap();
    std::fs::write(root.join("src/util.ts"), "export const pad = (n: number) => n;\n").unwrap();
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let org_id: String = conn
        .query_row("SELECT org_id FROM users WHERE email = ?1", [EMAIL], |r| r.get(0))
        .unwrap();
    let db = std::sync::Arc::new(std::sync::Mutex::new(conn));
    nexusmind::indexer::index_project(&org_id, "app", root.to_str().unwrap(), &db, None, false)
        .unwrap();
    dir
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

#[tokio::test]
async fn a_context_pack_carries_ranked_code_pinned_to_a_commit() {
    let (router, path) = app("super_user");
    let _repo = index_repo(&path);
    let cookie = login(&router).await;

    let (status, body) = post(
        &router,
        &cookie,
        "/v1/code/context-pack",
        json!({"project": "app", "query": "refund from the sale detail page", "commit": COMMIT}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["pack"]["schema_version"], 1);
    assert_eq!(body["pack"]["repository"]["commit"], COMMIT);
    let artifacts = body["pack"]["artifacts"].as_array().unwrap();
    let paths: Vec<&str> = artifacts.iter().map(|a| a["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"src/pages/SaleDetail.tsx"), "{paths:?}");
    assert!(paths.contains(&"src/api/refunds.ts"), "{paths:?}");
    assert!(!paths.contains(&"src/util.ts"), "{paths:?}");
    assert!(artifacts[0]["content_hash"].as_str().unwrap().starts_with("sha256:"));
    let evidence = body["evidence"].as_array().unwrap();
    assert_eq!(evidence.len(), artifacts.len());
    assert!(evidence[0]["content"].as_str().unwrap().contains("export"));
    // A temp directory is no checkout: the index has no commit of its own.
    assert_eq!(body["index"]["commit"], Value::Null);
}

#[tokio::test]
async fn a_pack_without_any_commit_is_a_conflict_and_bad_input_is_rejected() {
    let (router, path) = app("super_user");
    let _repo = index_repo(&path);
    let cookie = login(&router).await;

    let (status, body) = post(
        &router,
        &cookie,
        "/v1/code/context-pack",
        json!({"project": "app", "query": "refund"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "index_commit_unknown");

    for bad in [
        json!({"project": "app", "query": "", "commit": COMMIT}),
        json!({"project": "app", "query": "refund", "commit": "abc"}),
        json!({"project": "app", "query": "refund", "commit": COMMIT, "task_id": "NOT-A-UUID"}),
    ] {
        let (status, body) = post(&router, &cookie, "/v1/code/context-pack", bad).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    }
    let (status, _) = post(
        &router,
        &cookie,
        "/v1/code/context-pack",
        json!({"project": "missing", "query": "refund", "commit": COMMIT}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn locate_ranks_files_by_bm25_without_an_embedder() {
    let (router, path) = app("super_user");
    let _repo = index_repo(&path);
    let cookie = login(&router).await;
    let (status, body) = post(
        &router,
        &cookie,
        "/v1/code/locate",
        json!({"project": "app", "query": "create refund", "limit": 5}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["results"][0]["file_path"], "src/api/refunds.ts");
}

#[tokio::test]
async fn a_pack_is_not_served_to_a_user_outside_the_project() {
    let (router, path) = app("admin");
    let _repo = index_repo(&path);
    let cookie = login(&router).await;
    let (status, _) = post(
        &router,
        &cookie,
        "/v1/code/context-pack",
        json!({"project": "app", "query": "refund", "commit": COMMIT}),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}
