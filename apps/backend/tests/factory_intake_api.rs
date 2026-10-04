//! HTTP-level tests for the factory F3 intake sources and watchdog endpoints.

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
        std::env::temp_dir().join(format!("factory-intake-{}.db", uuid::Uuid::new_v4()));
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

/// An issue resolver, a secret connector and a Slack webhook connector.
fn fixtures(path: &std::path::Path) -> String {
    let conn = connection::connect(path.to_str().unwrap()).unwrap();
    let (org, user): (String, String) =
        conn.query_row("SELECT org_id, id FROM users LIMIT 1", [], |r| Ok((r.get(0)?, r.get(1)?))).unwrap();
    conn.execute(
        "INSERT INTO autonomous_agent_definitions (id,org_id,template_key,template_version,name,status,current_revision,created_by)
         VALUES ('def-1',?1,'github_issue_resolver',1,'Resolver','enabled',1,?2)",
        [&org, &user],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO autonomous_agent_revisions
           (id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,validation_status,created_by)
         VALUES ('rev-1','def-1',1,'{\"repository\":\"acme/app\"}','h','{}','{}','valid',?1)",
        [&user],
    )
    .unwrap();
    for (id, kind, metadata) in [
        ("conn-secret", "target_secret", r#"{"purpose":"factory_intake","source_kind":"slack"}"#),
        ("conn-qa", "target_secret", "{}"),
        ("conn-hook", "slack", "{}"),
    ] {
        conn.execute(
            "INSERT INTO autonomous_agent_connectors (id,org_id,kind,name,health,metadata_json,created_by)
             VALUES (?1,?2,?3,?1,'ready',?4,?5)",
            [id, &org, kind, metadata, &user],
        )
        .unwrap();
    }
    org
}

fn slack_source() -> Value {
    json!({
        "kind": "slack",
        "name": "Bugs channel",
        "project": "app",
        "resolver_definition_id": "def-1",
        "connector_id": "conn-secret",
        "config": {"channel_id": "C0123ABCD", "allowed_reactors": ["ULEAD0001"]}
    })
}

#[tokio::test]
async fn intake_and_watchdog_need_the_factory_permission() {
    let (router, _) = app("admin");
    let cookie = login(&router).await;
    for uri in ["/v1/factory/intake/sources", "/v1/factory/intake/items", "/v1/factory/watchdog"] {
        assert_eq!(send(&router, &cookie, "GET", uri, None).await.0, StatusCode::FORBIDDEN, "{uri}");
    }
    let (status, _) = send(&router, &cookie, "POST", "/v1/factory/intake/sources", Some(slack_source())).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn intake_sources_are_validated_created_updated_and_deleted() {
    let (router, path) = app("super_user");
    let cookie = login(&router).await;
    fixtures(&path);
    let uri = "/v1/factory/intake/sources";

    let mut bad_kind = slack_source();
    bad_kind["kind"] = json!("github");
    let mut bad_config = slack_source();
    bad_config["config"]["allowed_reactors"] = json!([]);
    let mut hook_as_token = slack_source();
    hook_as_token["connector_id"] = json!("conn-hook");
    let mut other_secret = slack_source();
    other_secret["connector_id"] = json!("conn-qa");
    let mut slack_token_for_sentry = slack_source();
    slack_token_for_sentry["kind"] = json!("sentry");
    slack_token_for_sentry["config"] =
        json!({"base_url": "https://sentry.evil.dev", "org_slug": "a", "project_slug": "b", "query": "is:unresolved"});
    let mut no_resolver = slack_source();
    no_resolver["resolver_definition_id"] = json!("nope");
    let mut bad_ref = slack_source();
    bad_ref["base_ref"] = json!("--upload-pack=x");
    for (body, code) in [
        (bad_kind, "validation_error"),
        (bad_config, "invalid_allowed_reactors"),
        (hook_as_token, "invalid_connector"),
        (other_secret, "invalid_connector"),
        (slack_token_for_sentry, "invalid_connector"),
        (no_resolver, "invalid_resolver"),
        (bad_ref, "validation_error"),
    ] {
        let (status, response) = send(&router, &cookie, "POST", uri, Some(body)).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{response}");
        assert_eq!(response["code"], code);
    }

    let (status, source) = send(&router, &cookie, "POST", uri, Some(slack_source())).await;
    assert_eq!(status, StatusCode::CREATED, "{source}");
    assert_eq!(source["repository"], "acme/app");
    assert_eq!(source["enabled"], true);
    assert_eq!(send(&router, &cookie, "POST", uri, Some(slack_source())).await.0, StatusCode::CONFLICT);

    let id = source["id"].as_str().unwrap();
    let mut paused = slack_source();
    paused["enabled"] = json!(false);
    paused["privacy_class"] = json!("confidential");
    let (status, updated) = send(&router, &cookie, "PUT", &format!("{uri}/{id}"), Some(paused)).await;
    assert_eq!(status, StatusCode::OK, "{updated}");
    assert_eq!((updated["enabled"].clone(), updated["privacy_class"].clone()), (json!(false), json!("confidential")));
    let mut other_kind = slack_source();
    other_kind["kind"] = json!("sentry");
    assert_eq!(
        send(&router, &cookie, "PUT", &format!("{uri}/{id}"), Some(other_kind)).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );

    // Pausing never re-validates, so a broken source can always be paused.
    {
        let conn = connection::connect(path.to_str().unwrap()).unwrap();
        conn.execute("UPDATE autonomous_agent_connectors SET health='revoked' WHERE id='conn-secret'", []).unwrap();
    }
    let (status, resumed) = send(&router, &cookie, "PATCH", &format!("{uri}/{id}"), Some(json!({"enabled": true}))).await;
    assert_eq!((status, resumed["enabled"].clone()), (StatusCode::OK, json!(true)));
    assert_eq!(
        send(&router, &cookie, "PATCH", &format!("{uri}/missing"), Some(json!({"enabled": false}))).await.0,
        StatusCode::NOT_FOUND
    );

    let (_, list) = send(&router, &cookie, "GET", uri, None).await;
    assert_eq!(list.as_array().unwrap().len(), 1);
    let (status, items) = send(&router, &cookie, "GET", "/v1/factory/intake/items", None).await;
    assert_eq!((status, items), (StatusCode::OK, json!([])));
    assert_eq!(send(&router, &cookie, "DELETE", &format!("{uri}/{id}"), None).await.0, StatusCode::NO_CONTENT);
    assert_eq!(send(&router, &cookie, "DELETE", &format!("{uri}/{id}"), None).await.0, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_watchdog_takes_only_a_slack_connector() {
    let (router, path) = app("super_user");
    let cookie = login(&router).await;
    fixtures(&path);
    let uri = "/v1/factory/watchdog";
    assert_eq!(send(&router, &cookie, "GET", uri, None).await, (StatusCode::OK, Value::Null));
    for body in [
        json!({"slack_connector_id": "conn-secret", "enabled": true}),
        json!({"slack_connector_id": "conn-hook", "enabled": true, "daily_hour_utc": 24}),
    ] {
        assert_eq!(send(&router, &cookie, "PUT", uri, Some(body)).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    let (status, watchdog) =
        send(&router, &cookie, "PUT", uri, Some(json!({"slack_connector_id": "conn-hook", "enabled": true, "daily_hour_utc": 12}))).await;
    assert_eq!(status, StatusCode::OK, "{watchdog}");
    assert_eq!(watchdog["slack_connector_id"], "conn-hook");
    assert_eq!(watchdog["daily_hour_utc"], 12);
}
