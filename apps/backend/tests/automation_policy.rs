use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use nexusmind::{
    api::{automation, middleware as auth_middleware},
    auth::api_keys,
    automation::{
        policy::{resolve_execution, AuthorizationRequest, AuthorizationStatus},
        profiles::{ExecutionProfileVersion, ExtensionRef},
    },
    db::{connection, migrations, queries},
    store::sqlite::SqliteStore,
};
use tower::util::ServiceExt;
use uuid::Uuid;

fn profile(kind: &str) -> ExecutionProfileVersion {
    ExecutionProfileVersion {
        id: "profile-version-1".to_string(),
        profile: kind.to_string(),
        version: 1,
        provider: "claude-code".to_string(),
        model: "claude-sonnet".to_string(),
        settings_hash: "settings-sha256".to_string(),
        extensions: vec![ExtensionRef {
            name: "approved-mcp".to_string(),
            version: "1.0.0".to_string(),
            hash: "extension-sha256".to_string(),
            required: true,
        }],
    }
}

#[test]
fn profile_resolution_rejects_other_providers_and_project_widening() {
    let implementation = profile("implementation");

    let other_provider = resolve_execution(
        &AuthorizationRequest {
            provider: "openai".to_string(),
            requested_profile: "implementation".to_string(),
            organization_allowed_profiles: vec!["implementation".to_string()],
            project_allowed_profiles: vec!["implementation".to_string()],
            requested_capabilities: vec![],
            extensions: implementation.extensions.clone(),
        },
        &[implementation.clone()],
    );
    assert_eq!(other_provider.status, AuthorizationStatus::Denied);
    assert_eq!(
        other_provider.reason.as_deref(),
        Some("unsupported_provider")
    );

    let repository_widening = resolve_execution(
        &AuthorizationRequest {
            provider: "claude-code".to_string(),
            requested_profile: "qa-deploy".to_string(),
            organization_allowed_profiles: vec!["implementation".to_string()],
            project_allowed_profiles: vec!["implementation".to_string(), "qa-deploy".to_string()],
            requested_capabilities: vec![],
            extensions: implementation.extensions.clone(),
        },
        &[implementation],
    );
    assert_eq!(repository_widening.status, AuthorizationStatus::Denied);
    assert_eq!(
        repository_widening.reason.as_deref(),
        Some("profile_not_allowed")
    );
}

#[test]
fn read_only_profile_denies_repository_writes_and_required_extension_failures() {
    let read_only = profile("read-only");

    let write_attempt = resolve_execution(
        &AuthorizationRequest {
            provider: "claude-code".to_string(),
            requested_profile: "read-only".to_string(),
            organization_allowed_profiles: vec!["read-only".to_string()],
            project_allowed_profiles: vec!["read-only".to_string()],
            requested_capabilities: vec!["repository_write".to_string(), "pr_publish".to_string()],
            extensions: read_only.extensions.clone(),
        },
        &[read_only.clone()],
    );
    assert_eq!(write_attempt.status, AuthorizationStatus::Denied);
    assert_eq!(
        write_attempt.reason.as_deref(),
        Some("read_only_write_denied")
    );

    let missing_extension = resolve_execution(
        &AuthorizationRequest {
            provider: "claude-code".to_string(),
            requested_profile: "read-only".to_string(),
            organization_allowed_profiles: vec!["read-only".to_string()],
            project_allowed_profiles: vec!["read-only".to_string()],
            requested_capabilities: vec![],
            extensions: vec![],
        },
        &[read_only],
    );
    assert_eq!(missing_extension.status, AuthorizationStatus::Denied);
    assert_eq!(
        missing_extension.reason.as_deref(),
        Some("required_extension_unavailable")
    );
}

#[test]
fn implementation_profile_allows_approved_writes_with_pinned_provenance() {
    let implementation = profile("implementation");
    let decision = resolve_execution(
        &AuthorizationRequest {
            provider: "claude-code".to_string(),
            requested_profile: "implementation".to_string(),
            organization_allowed_profiles: vec!["implementation".to_string()],
            project_allowed_profiles: vec!["implementation".to_string()],
            requested_capabilities: vec!["repository_write".to_string(), "pr_publish".to_string()],
            extensions: implementation.extensions.clone(),
        },
        &[implementation],
    );

    assert_eq!(decision.status, AuthorizationStatus::Allowed);
    let provenance = decision
        .provenance
        .expect("approved profile records provenance");
    assert_eq!(provenance.provider, "claude-code");
    assert_eq!(provenance.extension_hashes, vec!["extension-sha256"]);
}

fn app(store: SqliteStore) -> Router {
    Router::new()
        .route("/v1/automation/profiles", get(automation::list_profiles))
        .route(
            "/v1/automation/authorize",
            post(automation::authorize_profile),
        )
        .layer(middleware::from_fn_with_state(
            store.conn(),
            auth_middleware::auth,
        ))
        .layer(tower_cookies::CookieManagerLayer::new())
        .with_state(store)
}

fn setup_store() -> (SqliteStore, String, String) {
    let conn = connection::connect(":memory:").unwrap();
    migrations::run(&conn).unwrap();
    let store = SqliteStore::new(conn);
    let (org, _, admin_key) = {
        let db = store.conn();
        let conn = db.lock().unwrap();
        queries::bootstrap(&conn, "Acme", "acme", "admin@acme.example", "Admin").unwrap()
    };
    (store, org.id, admin_key)
}

#[tokio::test]
async fn authorization_route_requires_automation_write_permission() {
    let (store, org_id, _) = setup_store();
    let member_key = {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, ?3, 'Member', 'member', 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, "member@acme.example"],
        ).unwrap();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at) VALUES (?1, ?2, ?3, ?4, 'member', datetime('now'))",
            rusqlite::params![Uuid::new_v4().to_string(), user_id, org_id, key_hash],
        ).unwrap();
        raw_key
    };

    let response = app(store)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/automation/authorize")
                .header("Authorization", format!("Bearer {member_key}"))
                .header("Content-Type", "application/json")
                .body(Body::from(r#"{"profile":"read-only"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn profiles_route_returns_pinned_profile_provenance() {
    let (store, _, admin_key) = setup_store();

    let response = app(store)
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/automation/profiles")
                .header("Authorization", format!("Bearer {admin_key}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(body["profiles"][1]["provider"], "claude-code");
    assert_eq!(body["profiles"][1]["settings_hash"], "settings-sha256");
}

#[tokio::test]
async fn authorization_route_rejects_repository_supplied_allowlists() {
    let (store, _, admin_key) = setup_store();

    let response = app(store)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/automation/authorize")
                .header("Authorization", format!("Bearer {admin_key}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    r#"{"profile":"implementation","organization_allowed_profiles":["implementation"]}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}
