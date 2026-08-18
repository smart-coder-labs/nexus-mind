use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::{hidden_resource_not_found, require_permission, AppJson},
    db::queries,
    models::types::{
        ApiError, AuthContext, CreateHarnessConfigReviewCommentRequest,
        CreateHarnessConfigReviewRequest, CreateHarnessRequest, Harness, HarnessApproval,
        HarnessApprovalRequest, HarnessConfigReview, HarnessConfigReviewComment,
        HarnessDownloadResponse, HarnessInstallResultRequest, HarnessRecommendation,
        HarnessVersion, PublishHarnessVersionRequest,
    },
    store::sqlite::SqliteStore,
};

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    let (status, code) = if msg.contains("not_found") || msg.contains("version_not_found") {
        (StatusCode::NOT_FOUND, "not_found")
    } else if msg.contains("approval_required") || msg.contains("warning_acknowledgement_required")
    {
        (StatusCode::FORBIDDEN, "approval_required")
    } else if msg.contains("validation")
        || msg.contains("missing_")
        || msg.contains("mismatch")
        || msg.contains("secret_scan")
        || msg.contains("raw_local_content")
        || msg.contains("empty_comment")
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
    if auth.role.is_super_user() {
        None
    } else {
        Some(auth.user_id.as_str())
    }
}

fn load_visible_harness(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    id: &str,
    method: &str,
) -> Result<Harness, (StatusCode, Json<ApiError>)> {
    if let Some(harness) =
        queries::get_harness(conn, &auth.org_id, id, viewer_user_id(auth)).map_err(db_err)?
    {
        return Ok(harness);
    }
    if queries::get_harness(conn, &auth.org_id, id, None)
        .map_err(db_err)?
        .is_some()
    {
        return Err(hidden_resource_not_found(
            conn,
            auth,
            "harness",
            id,
            method,
            "harnesses",
        ));
    }
    Err((
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Harness not found".to_string(),
            code: "not_found".to_string(),
        }),
    ))
}

fn load_visible_config_review(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    id: &str,
    method: &str,
) -> Result<HarnessConfigReview, (StatusCode, Json<ApiError>)> {
    if let Some(review) =
        queries::get_harness_config_review_visible(conn, &auth.org_id, id, viewer_user_id(auth))
            .map_err(db_err)?
    {
        return Ok(review);
    }
    if queries::get_harness_config_review(conn, &auth.org_id, id)
        .map_err(db_err)?
        .is_some()
    {
        return Err(hidden_resource_not_found(
            conn,
            auth,
            "harness_config_review",
            id,
            method,
            "harnesses",
        ));
    }
    Err((
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Config review not found".to_string(),
            code: "not_found".to_string(),
        }),
    ))
}

#[derive(Debug, Deserialize)]
pub struct HarnessListQuery {
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub owner_user_id: Option<String>,
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
        params.owner_user_id.as_deref(),
    )
    .map_err(db_err)?;
    Ok(Json(rows))
}

pub async fn create_harness(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(mut input): AppJson<CreateHarnessRequest>,
) -> Result<(StatusCode, Json<Harness>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:write")?;
    if !auth.role.is_privileged() {
        if let Some(owner_user_id) = input.owner_user_id.as_deref() {
            if owner_user_id != auth.user_id {
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ApiError {
                        error: "Only privileged users can assign a different harness owner"
                            .to_string(),
                        code: "forbidden".to_string(),
                    }),
                ));
            }
        }
        input.owner_user_id = Some(auth.user_id.clone());
    }
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
    let harness = load_visible_harness(&conn, &auth, &id, "GET")?;
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
    load_visible_harness(&conn, &auth, &id, "POST")?;
    let version = queries::publish_harness_version(&conn, &auth.org_id, &auth.user_id, &id, &input)
        .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(version)))
}

pub async fn get_version(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, version)): Path<(String, String)>,
) -> Result<Json<HarnessVersion>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:read")?;
    let Some(version) = queries::get_visible_harness_version(
        &conn,
        &auth.org_id,
        &id,
        &version,
        viewer_user_id(&auth),
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
    Ok(Json(version))
}

pub async fn archive_harness(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Harness>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:write")?;
    load_visible_harness(&conn, &auth, &id, "POST")?;
    let harness = queries::archive_harness(&conn, &auth.org_id, &id, viewer_user_id(&auth))
        .map_err(db_err)?
        .expect("visible harness remains visible during the locked update");
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness.archived",
        "harness",
        Some(&id),
        serde_json::json!({ "status": harness.status }),
    );
    Ok(Json(harness))
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

#[derive(Debug, Deserialize)]
pub struct ConfigReviewListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

pub async fn list_config_reviews(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ConfigReviewListQuery>,
) -> Result<Json<Vec<HarnessConfigReview>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    let reviews = queries::list_harness_config_reviews_visible(
        &conn,
        &auth.org_id,
        params.status.as_deref(),
        viewer_user_id(&auth),
    )
    .map_err(db_err)?;
    Ok(Json(reviews))
}

pub async fn get_config_review(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<HarnessConfigReview>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    let review = load_visible_config_review(&conn, &auth, &id, "GET")?;
    Ok(Json(review))
}

pub async fn list_config_review_comments(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<HarnessConfigReviewComment>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    load_visible_config_review(&conn, &auth, &id, "GET")?;
    let comments =
        queries::list_harness_config_review_comments(&conn, &auth.org_id, &id).map_err(db_err)?;
    Ok(Json(comments))
}

pub async fn create_config_review_comment(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<CreateHarnessConfigReviewCommentRequest>,
) -> Result<(StatusCode, Json<HarnessConfigReviewComment>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "harness:review_config")?;
    load_visible_config_review(&conn, &auth, &id, "POST")?;
    let comment = queries::create_harness_config_review_comment(
        &conn,
        &auth.org_id,
        &auth.user_id,
        &id,
        &input.body,
    )
    .map_err(db_err)?;
    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "harness_config_review.commented",
        "harness_config_review",
        Some(&id),
        serde_json::json!({ "comment_id": comment.id }),
    );
    Ok((StatusCode::CREATED, Json(comment)))
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
    use sha2::Digest;
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
            .route("/v1/harnesses/:id/archive", post(archive_harness))
            .route("/v1/harnesses/:id/versions", post(publish_version))
            .route("/v1/harnesses/:id/versions/:version", get(get_version))
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
            .route(
                "/v1/harness-config-reviews",
                get(list_config_reviews).post(create_config_review),
            )
            .route("/v1/harness-config-reviews/:id", get(get_config_review))
            .route(
                "/v1/harness-config-reviews/:id/comments",
                get(list_config_review_comments).post(create_config_review_comment),
            )
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
            conn.execute(
                "UPDATE users SET role = 'super_user' WHERE id = ?1",
                [&admin.id],
            )
            .unwrap();
            (key, org.id, admin.id)
        };
        (store, admin_key, org_id, admin_id)
    }

    fn create_member_key(store: &SqliteStore, org_id: &str, role: &str) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = uuid::Uuid::new_v4().to_string();
        conn.execute("INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, ?2, ?3, 'User', ?4, 'active', datetime('now'))", rusqlite::params![user_id, org_id, format!("{user_id}@test.com"), role]).unwrap();
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

    fn create_harness_writer_key(store: &SqliteStore, org_id: &str) -> String {
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let role_id = uuid::Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO roles (id, org_id, name, display_name, description, extends_json, permissions, version, enabled, is_template, created_at, updated_at) VALUES (?1, ?2, 'harness-writer', 'Harness Writer', NULL, '[]', ?3, 1, 1, 0, datetime('now'), datetime('now'))",
                rusqlite::params![role_id, org_id, serde_json::json!(["harness:read", "harness:write"]).to_string()],
            ).unwrap();
        }
        create_member_key(store, org_id, "harness-writer")
    }

    fn lookup_user_id_for_key(store: &SqliteStore, raw_key: &str) -> String {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let key_hash = api_keys::hash_key(raw_key);
        conn.query_row(
            "SELECT user_id FROM api_keys WHERE key_hash = ?1",
            [key_hash],
            |row| row.get(0),
        )
        .unwrap()
    }

    fn manifest() -> serde_json::Value {
        serde_json::json!({ "schema_version": "1.0", "targets": ["claude"], "components": [], "compatibility": {}, "provenance": { "source": "test" }, "security": { "requires_approval": true } })
    }

    fn executable_manifest() -> serde_json::Value {
        let content = "{\"name\":\"reviewer\"}";
        serde_json::json!({
            "schema_version": "1.1",
            "format": "claude_code_plugin",
            "targets": ["claude"],
            "components": [{
                "kind": "plugin_marketplace",
                "path": "plugins/reviewer.json",
                "media_type": "application/json",
                "size_bytes": content.as_bytes().len(),
                "sha256": format!("sha256:{}", hex::encode(sha2::Sha256::digest(content.as_bytes()))),
                "content": content
            }],
            "provenance": { "source": "test" },
            "security": { "requires_approval": true, "executable": true, "secret_scan_status": "passed" }
        })
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
                    owner_user_id: None,
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
    async fn create_defaults_owner_accepts_admin_owner_and_filters_by_owner() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let owner_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::invite_user(&conn, &org_id, "owner@acme.com", "Owner User", "member")
                .unwrap()
                .0
                .id
        };
        let app = app(store);
        let default_body = serde_json::json!({ "slug": "default-owner", "name": "Default Owner" });
        let default_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harnesses")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(default_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(default_resp.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(default_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["owner_user_id"], admin_id);
        assert_eq!(json["owner"]["name"], "Admin");

        let assigned_body = serde_json::json!({ "slug": "assigned-owner", "name": "Assigned Owner", "owner_user_id": owner_id });
        let assigned_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harnesses")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(assigned_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(assigned_resp.status(), StatusCode::CREATED);

        let list = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/harnesses?owner_user_id={owner_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(list.status(), StatusCode::OK);
        let body = axum::body::to_bytes(list.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(rows.as_array().unwrap().len(), 1);
        assert_eq!(rows[0]["owner_user_id"], owner_id);
    }

    #[tokio::test]
    async fn non_privileged_harness_writer_cannot_assign_a_different_owner() {
        let (store, _admin_key, org_id, _admin_id) = setup_org();
        let writer_key = create_harness_writer_key(&store, &org_id);
        let writer_user_id = lookup_user_id_for_key(&store, &writer_key);
        let owner_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::invite_user(&conn, &org_id, "owner@acme.com", "Owner User", "member")
                .unwrap()
                .0
                .id
        };

        let app = app(store);
        let rejected_body = serde_json::json!({
            "slug": "writer-owned",
            "name": "Writer Owned",
            "owner_user_id": owner_id,
        });
        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harnesses")
                    .header("Authorization", format!("Bearer {writer_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(rejected_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let self_owned = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harnesses")
                    .header("Authorization", format!("Bearer {writer_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "slug": "writer-self-owned",
                            "name": "Writer Self Owned"
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(self_owned.status(), StatusCode::CREATED);

        let body = axum::body::to_bytes(self_owned.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["owner_user_id"], writer_user_id);
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
                    owner_user_id: None,
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
    async fn executable_approval_requires_warning_acknowledgement_metadata() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let (harness_id, version, manifest_hash) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let h = q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "plugin-base".into(),
                    name: "Plugin Base".into(),
                    description: None,
                    project_id: None,
                    visibility: None,
                    owner_user_id: None,
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
                    manifest: executable_manifest(),
                    manifest_hash: None,
                },
            )
            .unwrap();
            (h.id, v.version, v.manifest_hash)
        };
        let app = app(store);
        let missing_ack = serde_json::json!({
            "target_tool": "claude",
            "target_scope": "project",
            "manifest_hash": manifest_hash,
            "metadata": { "source": "admin-ui" }
        });
        let rejected = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/approval"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(missing_ack.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

        let acknowledged = serde_json::json!({
            "target_tool": "claude",
            "target_scope": "project",
            "manifest_hash": manifest_hash,
            "metadata": { "source": "admin-ui", "warning_acknowledged": true }
        });
        let approved = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!(
                        "/v1/harnesses/{harness_id}/versions/{version}/approval"
                    ))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(acknowledged.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(approved.status(), StatusCode::CREATED);
        let body = axum::body::to_bytes(approved.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["metadata"]["warning_acknowledged"], true);
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
                    owner_user_id: None,
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
                    owner_user_id: None,
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
                    owner_user_id: None,
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
    async fn archiving_a_harness_sets_status_and_requires_write() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let harness_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::create_harness(
                &conn,
                &org_id,
                &admin_id,
                &CreateHarnessRequest {
                    slug: "base".into(),
                    name: "Base".into(),
                    description: None,
                    project_id: None,
                    visibility: None,
                    owner_user_id: None,
                },
            )
            .unwrap()
            .id
        };

        let member_key = create_member_key(&store, &org_id, "member");
        let denied = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/harnesses/{harness_id}/archive"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(denied.status(), StatusCode::FORBIDDEN);

        let ok = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/harnesses/{harness_id}/archive"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ok.status(), StatusCode::OK);
        let body = axum::body::to_bytes(ok.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "archived");
    }

    #[tokio::test]
    async fn version_manifest_is_readable_for_preview_without_approval() {
        let (store, admin_key, org_id, admin_id) = setup_org();
        let harness_id = {
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
                    owner_user_id: None,
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
            h.id
        };
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/harnesses/{harness_id}/versions/1.0.0"))
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
        assert_eq!(json["version"], "1.0.0");
        assert!(
            json["manifest"]["components"].is_array(),
            "preview must expose manifest components with their content"
        );
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

    #[tokio::test]
    async fn shared_config_reviews_are_listed_for_authorized_reviewers() {
        let (store, admin_key, _, _) = setup_org();
        let create_body = serde_json::json!({
            "source_tool": "claude",
            "redacted_config": { "env": { "NEXUSMIND_API_KEY": "[REDACTED:secret]" } },
            "redaction_report": { "secret_scan_status": "passed", "secret_count": 1, "categories": { "secret": 1 } },
            "content_hash": "sha256:abc",
            "status": "shared"
        });
        let created = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harness-config-reviews")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(create_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);

        let listed = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/harness-config-reviews?status=shared")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listed.status(), StatusCode::OK);
        let body = axum::body::to_bytes(listed.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let rows = rows.as_array().expect("expected an array of reviews");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["source_tool"], "claude");
        assert_eq!(rows[0]["status"], "shared");
        assert_eq!(rows[0]["content_hash"], "sha256:abc");
        // Only the redacted snapshot is returned — never a raw secret.
        assert_eq!(
            rows[0]["redacted_config"]["env"]["NEXUSMIND_API_KEY"],
            "[REDACTED:secret]"
        );
        // The author identity is joined so reviewers see whose config it is.
        assert_eq!(rows[0]["author"]["name"], "Admin");
        assert_eq!(rows[0]["author"]["email"], "admin@acme.com");
    }

    #[tokio::test]
    async fn config_review_hidden_from_other_admin_returns_404_and_one_denial_audit() {
        let (store, super_user_key, org_id, _) = setup_org();
        let admin_key = create_member_key(&store, &org_id, "admin");
        let review_id = create_shared_review(&store, &super_user_key).await;
        let response = app(store.clone())
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/harness-config-reviews/{review_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM audit_logs WHERE org_id = ?1 AND action = 'resource.hidden_access_denied' AND resource_id = ?2",
            rusqlite::params![org_id, review_id], |row| row.get(0),
        ).unwrap();
        assert_eq!(count, 1);
    }

    async fn create_shared_review(store: &SqliteStore, key: &str) -> String {
        let body = serde_json::json!({
            "source_tool": "claude",
            "redacted_config": { "env": { "NEXUSMIND_API_KEY": "[REDACTED:secret]" } },
            "redaction_report": { "secret_scan_status": "passed", "secret_count": 1, "categories": { "secret": 1 } },
            "content_hash": "sha256:abc",
            "status": "shared"
        });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/harness-config-reviews")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        json["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn reviewers_can_comment_on_a_config_review_and_list_with_author() {
        let (store, admin_key, _, _) = setup_org();
        let review_id = create_shared_review(&store, &admin_key).await;

        let posted = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/harness-config-reviews/{review_id}/comments"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "body": "Looks safe to me." }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(posted.status(), StatusCode::CREATED);

        let listed = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/harness-config-reviews/{review_id}/comments"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(listed.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(listed.into_body(), usize::MAX)
            .await
            .unwrap();
        let rows: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["body"], "Looks safe to me.");
        assert_eq!(rows[0]["review_id"], review_id);
        assert_eq!(rows[0]["author"]["name"], "Admin");
    }

    #[tokio::test]
    async fn empty_config_review_comment_is_rejected() {
        let (store, admin_key, _, _) = setup_org();
        let review_id = create_shared_review(&store, &admin_key).await;
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/harness-config-reviews/{review_id}/comments"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(serde_json::json!({ "body": "   " }).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn commenting_requires_review_permission() {
        let (store, admin_key, org_id, _) = setup_org();
        let review_id = create_shared_review(&store, &admin_key).await;
        let member_key = create_member_key(&store, &org_id, "member");
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/harness-config-reviews/{review_id}/comments"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({ "body": "sneaky" }).to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn listing_config_reviews_requires_review_permission() {
        let (store, _, org_id, _) = setup_org();
        let member_key = create_member_key(&store, &org_id, "member");
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/harness-config-reviews")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }
}
