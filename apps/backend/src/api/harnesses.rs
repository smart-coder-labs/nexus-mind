use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::{require_permission, AppJson},
    db::queries,
    models::types::{
        ApiError, AuthContext, CreateHarnessConfigReviewRequest, CreateHarnessRequest, Harness,
        HarnessApproval, HarnessApprovalRequest, HarnessConfigReview, HarnessDownloadResponse,
        HarnessInstallResultRequest, HarnessRecommendation, HarnessVersion,
        PublishHarnessVersionRequest,
    },
    store::sqlite::SqliteStore,
};

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    let (status, code) = if msg.contains("not_found") || msg.contains("version_not_found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else if msg.contains("approval_required") {
        (StatusCode::FORBIDDEN, "approval_required")
    } else if msg.contains("validation")
        || msg.contains("missing_")
        || msg.contains("mismatch")
        || msg.contains("secret_scan")
        || msg.contains("raw_local_content")
    {
        (StatusCode::UNPROCESSABLE_ENTITY, "validation_error")
    } else if msg.contains("UNIQUE constraint failed") {
        (StatusCode::CONFLICT, "conflict")
    } else {
        (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
    };
    (
        status,
        Json(ApiError {
            error: msg,
            code: code.to_string(),
        }),
    )
}

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn viewer_user_id(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_privileged() {
        None
    } else {
        Some(auth.user_id.as_str())
    }
}

#[derive(Debug, Deserialize)]
pub struct HarnessListQuery {
    #[serde(default)]
    pub target: Option<String>,
}

pub async fn list_harnesses(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<HarnessListQuery>,
) -> Result<Json<Vec<Harness>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:read")?;
    let rows = queries::list_visible_harnesses(
        &conn,
        &auth.org_id,
        viewer_user_id(&auth),
        params.target.as_deref(),
    )
    .map_err(db_err)?;
    Ok(Json(rows))
}

pub async fn create_harness(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateHarnessRequest>,
) -> Result<(StatusCode, Json<Harness>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:write")?;
    let harness =
        queries::create_harness(&conn, &auth.org_id, &auth.user_id, &input).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(harness)))
}

pub async fn get_harness(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Harness>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:read")?;
    let Some(harness) =
        queries::get_harness(&conn, &auth.org_id, &id, viewer_user_id(&auth)).map_err(db_err)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Harness not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    };
    Ok(Json(harness))
}

pub async fn publish_version(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<PublishHarnessVersionRequest>,
) -> Result<(StatusCode, Json<HarnessVersion>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:write")?;
    let version = queries::publish_harness_version(&conn, &auth.org_id, &auth.user_id, &id, &input)
        .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(version)))
}

pub async fn approve_install(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, version)): Path<(String, String)>,
    AppJson(input): AppJson<HarnessApprovalRequest>,
) -> Result<(StatusCode, Json<HarnessApproval>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:install")?;
    let approval = queries::create_harness_approval(
        &conn,
        &auth.org_id,
        &auth.user_id,
        viewer_user_id(&auth),
        &id,
        &version,
        &input,
    )
    .map_err(db_err)?;
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness.install_approved",
        "harness_version",
        Some(&approval.harness_version_id),
        serde_json::json!({ "harness_id": id, "version": version, "target_tool": approval.target_tool, "manifest_hash": approval.manifest_hash }),
    );
    Ok((StatusCode::CREATED, Json(approval)))
}

pub async fn download_version(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, version)): Path<(String, String)>,
) -> Result<Json<HarnessDownloadResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:download")?;
    let Some(download) = queries::download_harness_version(
        &conn,
        &auth.org_id,
        &auth.user_id,
        viewer_user_id(&auth),
        &id,
        &version,
    )
    .map_err(db_err)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Harness version not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    };
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness.downloaded",
        "harness",
        Some(&id),
        serde_json::json!({ "version": version, "manifest_hash": download.manifest_hash }),
    );
    Ok(Json(download))
}

pub async fn record_install_result(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, version)): Path<(String, String)>,
    AppJson(input): AppJson<HarnessInstallResultRequest>,
) -> Result<Json<HarnessApproval>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:install")?;
    let approval = queries::record_harness_install_result(
        &conn,
        &auth.org_id,
        &auth.user_id,
        &id,
        &version,
        &input,
    )
    .map_err(db_err)?;
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness.install_result_recorded",
        "harness_version",
        Some(&approval.harness_version_id),
        serde_json::json!({ "harness_id": id, "version": version, "manifest_hash": approval.manifest_hash, "status": approval.metadata.get("install_result").and_then(|v| v.get("status")).cloned().unwrap_or(serde_json::Value::Null) }),
    );
    Ok(Json(approval))
}

pub async fn recommendations(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<HarnessListQuery>,
) -> Result<Json<Vec<HarnessRecommendation>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:read")?;
    let rows = queries::list_harness_recommendations(
        &conn,
        &auth.org_id,
        viewer_user_id(&auth),
        params.target.as_deref(),
    )
    .map_err(db_err)?;
    Ok(Json(rows))
}

pub async fn create_config_review(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateHarnessConfigReviewRequest>,
) -> Result<(StatusCode, Json<HarnessConfigReview>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    let review = queries::create_harness_config_review(&conn, &auth.org_id, &auth.user_id, &input)
        .map_err(db_err)?;
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness_config_review.shared",
        "harness_config_review",
        Some(&review.id),
        serde_json::json!({ "source_tool": review.source_tool, "content_hash": review.content_hash, "status": review.status }),
    );
    Ok((StatusCode::CREATED, Json(review)))
}

pub async fn get_config_review(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<HarnessConfigReview>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    let Some(review) =
        queries::get_harness_config_review(&conn, &auth.org_id, &id).map_err(db_err)?
    else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Config review not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    };
    Ok(Json(review))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{Request, StatusCode};
    use axum::{
        body::Body,
        middleware,
        routing::{get, post},
        Router,
    };
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        auth::api_keys,
        db::{connection::connect, migrations, queries as q},
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/harnesses", get(list_harnesses).post(create_harness))
            .route("/v1/harnesses/:id/versions", post(publish_version))
            .route(
                "/v1/harnesses/:id/versions/:version/download",
                get(download_version),
            )
            .route(
                "/v1/harnesses/:id/versions/:version/approval",
                post(approve_install),
            )
            .route(
                "/v1/harnesses/:id/versions/:version/install-result",
                post(record_install_result),
            )
            .route("/v1/harness-recommendations", get(recommendations))
            .route("/v1/harness-config-reviews", post(create_config_review))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_org() -> (SqliteStore, String, String, String) {
        let store = make_store();
        let (admin_key, org_id, admin_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, admin, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (key, org.id, admin.id)
        };
        (store, admin_key, org_id, admin_id)
    }

    fn create_member_key(store: &SqliteStore, org_id: &str, role: &str) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = uuid::Uuid::new_v4().to_string();
        conn.execute("INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, 'u@test.com', 'User', ?3, 'active', datetime('now'))", rusqlite::params![user_id, org_id, role]).unwrap();
        let key_id = uuid::Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute("INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at) VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))", rusqlite::params![key_id, user_id, org_id, key_hash]).unwrap();
        raw_key
    }

    fn create_harness_operator_key(store: &SqliteStore, org_id: &str) -> String {
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let role_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO roles (id, org_id, name, display_name, description, extends_json, permissions, version, enabled, is_template, created_at, updated_at) VALUES (?1, ?2, 'harness-operator', 'Harness Operator', NULL, '[]', ?3, 1, 1, 0, datetime('now'), datetime('now'))",
                rusqlite::params![role_id, org_id, serde_json::json!(["harness:read", "harness:download", "harness:install"]).to_string()],
            ).unwrap();
        }
        create_member_key(store, org_id, "harness-operator")
    }

    fn manifest() -> serde_json::Value {
        serde_json::json!({ "schema_version": "1.0", "targets": ["claude"], "components": [], "compatibility": {}, "provenance": { "source": "test" }, "security": { "requires_approval": true } })
    }

    #[tokio::test]
    async fn unauthorized_download_does_not_expose_manifest() {
        let (store, _admin_key, org_id, admin_id) = setup_org();
        let member_key = create_member_key(&store, &org_id, "member");
        let (harness_id, version) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "base".into(),
                    name: "Base".into(),
                    description: None,
                    project_id: None,
                    visibility: None,
                },
            )
            .unwrap();
            let v = q::publish_harness_version(
                &conn,
                &org_id,
                &admin_id,
                &h.id,
                &PublishHarnessVersionRequest {
                    version: "1.0.0".into(),
                    manifest: manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
            (h.id, v.version)
        };
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/download"
                    ))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("manifest").is_none());
    }

    #[tokio::test]
    async fn download_requires_persisted_approval_before_manifest() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let (harness_id, version, manifest_hash) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "base".into(),
                    name: "Base".into(),
                    description: None,
                    project_id: None,
                    visibility: None,
                },
            )
            .unwrap();
            let v = q::publish_harness_version(
                &conn,
                &org_id,
                &admin_id,
                &h.id,
                &PublishHarnessVersionRequest {
                    version: "1.0.0".into(),
                    manifest: manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
            (h.id, v.version, v.manifest_hash)
        };
        let app = app(store);
        let before_approval = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/download"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(before_approval.status(), StatusCode::FORBIDDEN);
        let blocked_body = axum::body::to_bytes(before_approval.into_body(), usize::MAX)
            .await
            .unwrap();
        let blocked_json: serde_json::Value = serde_json::from_slice(&blocked_body).unwrap();
        assert!(blocked_json.get("manifest").is_none());

        let approval_body = serde_json::json!({ "target_tool": "claude", "target_scope": "project", "manifest_hash": manifest_hash });
        let approval = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/approval"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(approval_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approval.status(), StatusCode::CREATED);

        let after_approval = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/download"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(after_approval.status(), StatusCode::OK);
        let body = axum::body::to_bytes(after_approval.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["manifest_hash"], manifest_hash);
        assert!(json.get("manifest").is_some());
    }

    #[tokio::test]
    async fn project_scoped_harness_cannot_be_approved_or_downloaded_by_non_member() {
        let (store, _admin_key, org_id, admin_id) = setup_org();
        let operator_key = create_harness_operator_key(&store, &org_id);
        let (harness_id, version, manifest_hash) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let project = q::create_project(&conn, &org_id, "hidden", None, None).unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "hidden-base".into(),
                    name: "Hidden Base".into(),
                    description: None,
                    project_id: Some(project.id),
                    visibility: None,
                },
            )
            .unwrap();
            let v = q::publish_harness_version(
                &conn,
                &org_id,
                &admin_id,
                &h.id,
                &PublishHarnessVersionRequest {
                    version: "1.0.0".into(),
                    manifest: manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
            (h.id, v.version, v.manifest_hash)
        };
        let app = app(store);
        let approval_body = serde_json::json!({ "target_tool": "claude", "target_scope": "project", "manifest_hash": manifest_hash });
        let approval = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/approval"
                    ))
                    .header("Authorization", format!("Bearer {operator_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(approval_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approval.status(), StatusCode::NOT_FOUND);

        let download = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/download"
                    ))
                    .header("Authorization", format!("Bearer {operator_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(download.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(download.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json.get("manifest").is_none());
    }

    #[tokio::test]
    async fn install_result_records_status_without_local_file_contents() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let (harness_id, version, approval_id, manifest_hash) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "base".into(),
                    name: "Base".into(),
                    description: None,
                    project_id: None,
                    visibility: None,
                },
            )
            .unwrap();
            let v = q::publish_harness_version(
                &conn,
                &org_id,
                &admin_id,
                &h.id,
                &PublishHarnessVersionRequest {
                    version: "1.0.0".into(),
                    manifest: manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
            let approval = q::create_harness_approval(
                &conn,
                &org_id,
                &admin_id,
                None,
                &h.id,
                &v.version,
                &HarnessApprovalRequest {
                    target_tool: "claude".into(),
                    target_scope: "project".into(),
                    manifest_hash: v.manifest_hash.clone(),
                    metadata: None,
                },
            )
            .unwrap();
            (h.id, v.version, approval.id, v.manifest_hash)
        };
        let body = serde_json::json!({ "approval_id": approval_id, "manifest_hash": manifest_hash, "status": "installed", "metadata": { "changed_files_count": 2 } });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/install-result"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["metadata"]["install_result"]["status"], "installed");
        assert_eq!(json["metadata"]["install_result"]["changed_files_count"], 2);
        assert!(json["metadata"]["install_result"]
            .get("raw_file_contents")
            .is_none());
    }

    #[tokio::test]
    async fn recommendations_return_metadata_only() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "base".into(),
                    name: "Base".into(),
                    description: Some("Reusable harness".into()),
                    project_id: None,
                    visibility: None,
                },
            )
            .unwrap();
            q::publish_harness_version(
                &conn,
                &org_id,
                &admin_id,
                &h.id,
                &PublishHarnessVersionRequest {
                    version: "1.0.0".into(),
                    manifest: manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
        }
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/harness-recommendations?target=claude")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let first = &json.as_array().unwrap()[0];
        assert_eq!(first["approval_required"], true);
        assert!(
            first.get("manifest").is_none(),
            "recommendations must not include installable manifest content"
        );
        assert_eq!(first["targets"].as_array().unwrap()[0], "claude");
    }

    #[tokio::test]
    async fn secret_bearing_config_snapshot_is_rejected() {
        let (store, admin_key, _, _) = setup_org();
        let body = serde_json::json!({ "source_tool": "claude", "redacted_config": { "env": { "value": "raw-secret" } }, "redaction_report": { "secret_scan_status": "failed" }, "content_hash": "sha256:x" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harness-config-reviews")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
}
