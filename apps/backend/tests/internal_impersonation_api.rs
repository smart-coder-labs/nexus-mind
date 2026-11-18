//! Integration tests for `POST /internal/orgs/:id/impersonate`.
//!
//! Impersonation used to mint a `web-session` key through the same function the
//! login path uses, which revoked the impersonated admin's own session — the
//! support action logged the customer out. It is now a separate, short-lived
//! `impersonation` credential, and it is audited.

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use nexusmind::{
    api::router,
    config::Config,
    db::{connection, migrations, queries},
};
use serde_json::Value;
use tower::util::ServiceExt;

const SUPERUSER_KEY: &str = "test-superuser";

fn test_config() -> Config {
    Config {
        port: 0,
        db_path: ":memory:".into(),
        log_level: "error".into(),
        cors_origins: "*".into(),
        superuser_key: Some(SUPERUSER_KEY.to_string()),
        smtp_host: "localhost".into(),
        smtp_port: 587,
        smtp_username: None,
        smtp_password: None,
        smtp_from: None,
        app_base_url: "http://localhost:5173".into(),
        admin_origin: "http://localhost:3000".into(),
        cookie_secure: false,
        session_ttl_hours: 168,
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

/// Two tenants: the impersonation target, plus a second org whose data must stay
/// unreachable with the impersonation credential.
struct Fixture {
    router: axum::Router,
    org_id: String,
    admin_id: String,
    /// A live browser session for the target admin, minted before impersonation.
    admin_session: String,
    /// The other tenant's admin API key.
    other_key: String,
}

fn fixture() -> Fixture {
    let conn = connection::connect(":memory:").expect("in-memory db");
    migrations::run(&conn).expect("migrations");

    let (org, admin, _) = queries::bootstrap(
        &conn,
        "Target Org",
        "target-org",
        "admin@target.com",
        "Target Admin",
    )
    .expect("bootstrap");
    let (_, _, other_key) = queries::create_org(
        &conn,
        "Other Org",
        "other-org",
        "admin@other.com",
        "Other Admin",
    )
    .expect("second org");

    let admin_session = queries::create_web_session_key(&conn, &admin.id, &org.id, 168)
        .expect("admin browser session");

    Fixture {
        router: router::build(conn, test_config()),
        org_id: org.id,
        admin_id: admin.id,
        admin_session,
        other_key,
    }
}

/// Drives one request and returns `(status, parsed body)`.
async fn send(
    router: axum::Router,
    method: &str,
    uri: &str,
    bearer: Option<&str>,
    cookie: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(token) = bearer {
        builder = builder.header("Authorization", format!("Bearer {token}"));
    }
    if let Some(session) = cookie {
        builder = builder.header("Cookie", format!("nexusmind_session={session}"));
    }
    let request = match body {
        Some(json) => builder
            .header("Content-Type", "application/json")
            .body(Body::from(json.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

async fn impersonate(fixture: &Fixture) -> (StatusCode, Value) {
    send(
        fixture.router.clone(),
        "POST",
        &format!("/internal/orgs/{}/impersonate", fixture.org_id),
        Some(SUPERUSER_KEY),
        None,
        None,
    )
    .await
}

/// Exchanges `api_key` for a session cookie the way the admin panel does
/// (`apps/admin/src/api/client.ts::loginWithApiKey`) and returns
/// `(status, Set-Cookie)`. The response headers are the point here, so this
/// cannot go through [`send`].
async fn login_with_api_key(router: axum::Router, api_key: &str) -> (StatusCode, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/v1/admin/auth/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            serde_json::json!({ "api_key": api_key }).to_string(),
        ))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    let status = response.status();
    let set_cookie = response
        .headers()
        .get("set-cookie")
        .map(|value| value.to_str().unwrap().to_string())
        .unwrap_or_default();
    (status, set_cookie)
}

#[tokio::test]
async fn impersonation_mints_a_labelled_key_and_keeps_the_admin_signed_in() {
    let fixture = fixture();

    let (status, body) = impersonate(&fixture).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["label"], "impersonation",
        "the credential must be labelled so it is distinguishable from a login"
    );
    assert!(
        body["expires_at"].is_string(),
        "the response must say when the credential dies, body={body}"
    );
    let token = body["token"]
        .as_str()
        .expect("`token` is the field the backoffice reads")
        .to_string();

    let (status, me) = send(
        fixture.router.clone(),
        "GET",
        "/v1/admin/auth/me",
        Some(&token),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "the minted key must authenticate");
    assert_eq!(me["org"]["id"], fixture.org_id);
    assert_eq!(me["user"]["id"], fixture.admin_id);

    let (status, _) = send(
        fixture.router.clone(),
        "GET",
        "/v1/admin/auth/me",
        None,
        Some(&fixture.admin_session),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "impersonating an admin must not revoke that admin's browser session"
    );
}

/// The backoffice tells the operator to paste the token into the admin panel's
/// API-key login. That exchange must not launder a 1-hour audited break-glass
/// credential into an ordinary `session_ttl_hours` browser session.
#[tokio::test]
async fn exchanging_the_impersonation_token_keeps_the_session_short_and_labelled() {
    let fixture = fixture();

    let (status, body) = impersonate(&fixture).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let token = body["token"].as_str().expect("token").to_string();

    let (status, set_cookie) = login_with_api_key(fixture.router.clone(), &token).await;
    assert_eq!(status, StatusCode::OK, "the token must be exchangeable");
    assert!(
        set_cookie.contains("Max-Age=3600"),
        "the exchanged cookie must die with the 1h credential, not after the \
         configured {}h session TTL, got {set_cookie}",
        test_config().session_ttl_hours
    );

    let session = set_cookie
        .split(';')
        .next()
        .and_then(|pair| pair.strip_prefix("nexusmind_session="))
        .expect("Set-Cookie must carry the session cookie")
        .to_string();

    // `/v1/admin/keys` lists the org's live keys with their labels, so the row
    // behind the cookie can be inspected without reaching past the HTTP layer.
    let (status, keys) = send(
        fixture.router.clone(),
        "GET",
        "/v1/admin/keys",
        None,
        Some(&session),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={keys}");
    let keys = keys.as_array().expect("key list must be an array").clone();

    let impersonation: Vec<&Value> = keys
        .iter()
        .filter(|key| key["label"] == "impersonation")
        .collect();
    assert_eq!(
        impersonation.len(),
        1,
        "the exchange must leave exactly one live impersonation key — the one \
         the cookie carries, with the exchanged token revoked, got {keys:?}"
    );
    assert!(
        keys.iter().any(|key| key["label"] == "web-session"),
        "the admin's own browser session must survive the exchange, got {keys:?}"
    );
}

#[tokio::test]
async fn impersonation_is_audited() {
    let fixture = fixture();
    let (status, _) = impersonate(&fixture).await;
    assert_eq!(status, StatusCode::OK);

    let (status, audit) = send(
        fixture.router.clone(),
        "GET",
        "/internal/audit",
        Some(SUPERUSER_KEY),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let entries = audit.as_array().expect("audit list must be an array");
    let matching: Vec<&Value> = entries
        .iter()
        .filter(|e| e["action"] == "org.impersonate")
        .collect();
    assert_eq!(
        matching.len(),
        1,
        "exactly one impersonation entry must be written, got {audit}"
    );

    let entry = matching[0];
    assert_eq!(entry["org_id"], fixture.org_id);
    assert_eq!(
        entry["user_id"], fixture.admin_id,
        "the entry is attributed to the borrowed identity"
    );
    assert_eq!(entry["resource_type"], "org");
    assert_eq!(entry["metadata"]["origin"], "superadmin");
    assert_eq!(entry["metadata"]["label"], "impersonation");
    assert_eq!(entry["metadata"]["ttl_hours"], 1);
    assert!(entry["metadata"]["expires_at"].is_string());
}

#[tokio::test]
async fn impersonation_key_cannot_read_another_orgs_memory() {
    let fixture = fixture();

    // The other tenant stores a memory in its own default project.
    let (status, memory) = send(
        fixture.router.clone(),
        "POST",
        "/v1/memory/store",
        Some(&fixture.other_key),
        None,
        Some(serde_json::json!({
            "tool": "claude",
            "project": queries::DEFAULT_PROJECT_NAME,
            "content": "other tenant only",
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "body={memory}");
    let other_memory_id = memory["id"].as_str().expect("stored memory id").to_string();

    let (status, body) = impersonate(&fixture).await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let token = body["token"].as_str().expect("token").to_string();

    let (status, _) = send(
        fixture.router.clone(),
        "GET",
        &format!("/v1/memory/{other_memory_id}"),
        Some(&token),
        None,
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "the impersonation key must resolve only to the org it was minted for"
    );

    // Sanity check that the memory really is readable by its owner, so the 404
    // above is tenant isolation and not a bad id.
    let (status, _) = send(
        fixture.router.clone(),
        "GET",
        &format!("/v1/memory/{other_memory_id}"),
        Some(&fixture.other_key),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn impersonation_requires_the_superuser_key() {
    let fixture = fixture();

    let (status, _) = send(
        fixture.router.clone(),
        "POST",
        &format!("/internal/orgs/{}/impersonate", fixture.org_id),
        Some("not-the-superuser-key"),
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
