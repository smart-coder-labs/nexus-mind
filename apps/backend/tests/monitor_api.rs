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
    let (status, _) = send(&router, &b, "PATCH", &format!("/v1/monitor/sources/{id}"), Some(json!({"enabled": false}))).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // Org A pauses and resumes its own source.
    let (status, paused) = send(&router, &a, "PATCH", &format!("/v1/monitor/sources/{id}"), Some(json!({"enabled": false}))).await;
    assert_eq!((status, paused["enabled"].clone()), (StatusCode::OK, json!(false)));
    let (_, resumed) = send(&router, &a, "PATCH", &format!("/v1/monitor/sources/{id}"), Some(json!({"enabled": true}))).await;
    assert_eq!(resumed["enabled"], json!(true));

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

#[tokio::test]
async fn server_slack_and_github_are_only_for_the_operator_org() {
    let (router, path) = app();
    let a = login(&router, "a@test.com").await;
    let org_a: String = {
        let conn = connection::connect(path.to_str().unwrap()).unwrap();
        conn.query_row("SELECT org_id FROM users WHERE email='a@test.com'", [], |r| r.get(0)).unwrap()
    };
    let mut body = source_body("server");
    body["slack_connector_id"] = json!("server_slack_mcp");
    body["slack_channel"] = json!("C0123ABCDE");
    body["github_connector_id"] = json!("server_gh_cli");
    body["github_repo"] = json!("acme/app");

    // The only test in this binary that touches the variable; every other one
    // uses its own connectors, so running in parallel is safe.
    std::env::remove_var("FACTORY_GITHUB_PUBLISH_ORG_ID");
    let (_, options) = send(&router, &a, "GET", "/v1/monitor/options", None).await;
    assert_eq!(options, json!({"server_slack_mcp": false, "server_github": false}));
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body.clone())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    std::env::set_var("FACTORY_GITHUB_PUBLISH_ORG_ID", &org_a);
    let (_, options) = send(&router, &a, "GET", "/v1/monitor/options", None).await;
    assert_eq!(options, json!({"server_slack_mcp": true, "server_github": true}));
    let mut bad_channel = body.clone();
    bad_channel["slack_channel"] = json!("#alerts");
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(bad_channel)).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, source) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body)).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    assert_eq!(source["slack_channel"], "C0123ABCDE");
    assert_eq!(source["github_connector_id"], "server_gh_cli");
    let id = source["id"].as_str().unwrap().to_string();

    // Turning the error audit on creates an enabled, managed `error_audit` agent.
    let mut audited = json!({
        "project": "kasymir", "name": "server",
        "service_account": "nexusmind-push@my-proj-1.iam.gserviceaccount.com",
        "slack_connector_id": "server_slack_mcp", "slack_channel": "C0123ABCDE",
        "github_connector_id": "server_gh_cli", "github_repo": "acme/app",
        "audit_executor": "claude"
    });
    let (status, source) = send(&router, &a, "PUT", &format!("/v1/monitor/sources/{id}"), Some(audited.clone())).await;
    assert_eq!(status, StatusCode::OK, "{source}");
    let agent = source["audit_agent_id"].as_str().expect("managed agent").to_string();
    let (agent_row, agent_owner): (String, String) = {
        let conn = connection::connect(path.to_str().unwrap()).unwrap();
        conn.query_row(
            "SELECT d.template_key||':'||d.status, json_extract(r.config_json,'$.github_owner') FROM autonomous_agent_definitions d
               JOIN autonomous_agent_revisions r ON r.definition_id=d.id AND r.revision=d.current_revision WHERE d.id=?1 AND d.org_id=?2",
            [&agent, &org_a],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap()
    };
    assert_eq!((agent_row.as_str(), agent_owner.as_str()), ("error_audit:enabled", "acme"));

    // An unknown executor is refused; turning the audit off disables the agent.
    audited["audit_executor"] = json!("gpt");
    let (status, _) = send(&router, &a, "PUT", &format!("/v1/monitor/sources/{id}"), Some(audited.clone())).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    audited["audit_executor"] = json!(null);
    let (status, source) = send(&router, &a, "PUT", &format!("/v1/monitor/sources/{id}"), Some(audited)).await;
    assert_eq!((status, source["audit_executor"].clone()), (StatusCode::OK, json!(null)));
    let agent_status: String = {
        let conn = connection::connect(path.to_str().unwrap()).unwrap();
        conn.query_row("SELECT status FROM autonomous_agent_definitions WHERE id=?1", [&agent], |r| r.get(0)).unwrap()
    };
    assert_eq!(agent_status, "disabled");
    std::env::remove_var("FACTORY_GITHUB_PUBLISH_ORG_ID");
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn drains_authenticate_with_their_own_token_and_record_errors() {
    let (router, path) = app();
    let a = login(&router, "a@test.com").await;
    let b = login(&router, "b@test.com").await;
    let body = json!({"provider": "vercel", "project": "shop", "provider_ref": "acme", "name": "Shop", "throttle_minutes": 30});
    let (status, source) = send(&router, &a, "POST", "/v1/monitor/sources", Some(body.clone())).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    let id = source["id"].as_str().unwrap().to_string();
    let token = source["ingest_token"].as_str().unwrap().to_string();
    assert!(token.starts_with("nmd_"));
    assert_eq!((source["provider"].clone(), source["provider_ref"].clone()), (json!("vercel"), json!("acme")));
    // Shown once: never listed again, and the hash is never serialized.
    let (_, listed) = send(&router, &a, "GET", "/v1/monitor/sources", None).await;
    assert!(listed[0].get("ingest_token").is_none() && listed[0].get("ingest_token_hash").is_none());
    // Org B's token never opens org A's drain.
    let (_, other) = send(&router, &b, "POST", "/v1/monitor/sources", Some(body)).await;
    let other_token = other["ingest_token"].as_str().unwrap().to_string();

    let batch = json!([
        {"projectName": "shop", "level": "error", "message": "TypeError: boom", "statusCode": 500, "requestId": "r1"},
        {"projectName": "shop", "level": "info", "message": "ok", "statusCode": 200}
    ]);
    let drain = |uri: String, auth: Option<String>| {
        let mut request = Request::builder().method("POST").uri(uri).header("Content-Type", "application/json");
        if let Some(auth) = auth {
            request = request.header("Authorization", auth);
        }
        request.body(Body::from(batch.to_string())).unwrap()
    };
    let uri = format!("/v1/monitor/drain/{id}");
    for auth in [None, Some("Bearer nmd_wrong".to_string()), Some(format!("Bearer {other_token}"))] {
        let response = router.clone().oneshot(drain(uri.clone(), auth)).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let response = router.clone().oneshot(drain(uri.clone(), Some(format!("Bearer {token}")))).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let (_, groups) = send(&router, &a, "GET", "/v1/monitor/groups", None).await;
    assert_eq!(groups.as_array().unwrap().len(), 1, "only the error becomes a group: {groups}");
    assert_eq!(groups[0]["service"], "vercel-shop");
    let (_, groups) = send(&router, &b, "GET", "/v1/monitor/groups", None).await;
    assert_eq!(groups, json!([]));

    // A drain source is not a Pub/Sub target, and a GCP source is not a drain.
    let response = router.clone().oneshot(drain(format!("/v1/monitor/gcp/{id}"), Some(format!("Bearer {token}")))).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let (_, gcp) = send(&router, &a, "POST", "/v1/monitor/sources", Some(source_body("gcp"))).await;
    assert!(gcp.get("ingest_token").is_none());
    let gcp_id = gcp["id"].as_str().unwrap();
    let response = router.clone().oneshot(drain(format!("/v1/monitor/drain/{gcp_id}"), Some(format!("Bearer {token}")))).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(json!({"provider": "datadog", "project": "x", "name": "Bad"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let (status, _) = send(&router, &a, "POST", "/v1/monitor/sources", Some(json!({"provider": "supabase", "project": "x", "provider_ref": "../x", "name": "Bad ref"}))).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    let _ = std::fs::remove_file(path);
}
