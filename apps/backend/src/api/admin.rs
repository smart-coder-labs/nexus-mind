use axum::{extract::State, extract::Path, http::StatusCode, Extension, Json};
use serde::Deserialize;
use std::sync::Arc;

use crate::email::{send_password_setup, EmailConfig};

fn unauthorized() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::UNAUTHORIZED,
        Json(ApiError {
            error: "Valid superuser key required".to_string(),
            code: "unauthorized".to_string(),
        }),
    )
}

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, Org, OrgStats, CustomRole},
    store::sqlite::SqliteStore,
};

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

fn forbidden() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::FORBIDDEN,
        Json(ApiError {
            error: "Admin role required".to_string(),
            code: "forbidden".to_string(),
        }),
    )
}

#[derive(Deserialize)]
pub struct UpdateOrgInput {
    pub name: String,
}

#[derive(Deserialize)]
pub struct CreateOrgInput {
    pub org_name: String,
    pub org_slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

pub async fn create_org(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    Extension(email_config): Extension<Option<Arc<EmailConfig>>>,
    headers: axum::http::HeaderMap,
    Json(input): Json<CreateOrgInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    let expected = superuser_key.ok_or_else(unauthorized)?;
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    if provided != expected {
        return Err(unauthorized());
    }

    let (org, user, api_key, raw_token) = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| lock_err())?;
        let (org, user, api_key) = queries::create_org(
            &conn,
            &input.org_name,
            &input.org_slug,
            &input.admin_email,
            &input.admin_name,
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "Organization slug already exists".to_string(),
                        code: "slug_conflict".to_string(),
                    }),
                )
            } else {
                db_err(e)
            }
        })?;

        let (raw_token, _) = queries::create_password_reset_token(&conn, &user.id)
            .map_err(db_err)?;

        (org, user, api_key, raw_token)
    };

    // Send setup email asynchronously — non-fatal if SMTP is not configured
    if let Some(cfg) = email_config {
        let cfg = cfg.clone();
        let name = user.name.clone();
        let email = user.email.clone();
        let token = raw_token.clone();
        tokio::spawn(async move {
            if let Err(e) = send_password_setup(&cfg, &email, &name, &token).await {
                tracing::warn!("Failed to send org setup email to {email}: {e}");
            }
        });
    } else {
        tracing::warn!(
            "SMTP not configured — password setup token for {} (not sent): {}",
            user.email,
            raw_token
        );
    }

    let body = serde_json::json!({
        "org": org,
        "user": user,
        "api_key": api_key,
    });
    Ok((StatusCode::CREATED, Json(body)))
}

pub async fn list_orgs(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<Org>>, (StatusCode, Json<ApiError>)> {
    let expected = superuser_key.ok_or_else(unauthorized)?;
    let provided = headers
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(unauthorized)?;
    if provided != expected {
        return Err(unauthorized());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let orgs = queries::list_orgs(&conn).map_err(db_err)?;
    Ok(Json(orgs))
}

pub async fn stats(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<OrgStats>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let s = queries::get_stats(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(s))
}

pub async fn get_org(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let org = queries::get_org(&conn, &auth.org_id)
        .map_err(db_err)?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "Organization not found".to_string(),
                    code: "not_found".to_string(),
                }),
            )
        })?;
    Ok(Json(org))
}

pub async fn update_org(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<UpdateOrgInput>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let org = queries::update_org_name(&conn, &auth.org_id, &input.name).map_err(db_err)?;
    Ok(Json(org))
}

#[derive(Deserialize)]
pub struct CreateRoleInput {
    pub name: String,
    pub display_name: String,
    pub permissions: Vec<String>,
    pub description: Option<String>,
}

pub async fn list_roles_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<CustomRole>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let roles = queries::list_roles(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(roles))
}

pub async fn create_role_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<CreateRoleInput>,
) -> Result<(StatusCode, Json<CustomRole>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let role = queries::create_role(
        &conn,
        &auth.org_id,
        &input.name,
        &input.display_name,
        &input.permissions,
        input.description.as_deref(),
    )
    .map_err(|e| {
        if e.to_string().contains("UNIQUE constraint failed") {
            (
                StatusCode::CONFLICT,
                Json(ApiError {
                    error: "Role name already exists".to_string(),
                    code: "role_conflict".to_string(),
                }),
            )
        } else {
            db_err(e)
        }
    })?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "create",
        "role",
        Some(&role.id),
        serde_json::json!({ "name": role.name, "permissions": role.permissions }),
    );

    Ok((StatusCode::CREATED, Json(role)))
}

pub async fn delete_role_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(role_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let deleted = queries::delete_role(&conn, &auth.org_id, &role_id).map_err(db_err)?;

    if !deleted {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Role not found or cannot be deleted".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "delete",
        "role",
        Some(&role_id),
        serde_json::json!({}),
    );

    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    use crate::{
        api::middleware as auth_mw,
        db::{connection::connect, migrations, queries as q},
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/stats", get(stats))
            .route("/v1/admin/org", get(get_org).patch(update_org))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .route("/v1/orgs", post(create_org))
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_admin_key() -> (SqliteStore, String) {
        let store = make_store();
        let raw_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (store, raw_key)
    }

    #[tokio::test]
    async fn stats_returns_200_for_admin() {
        let (store, key) = setup_with_admin_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/stats")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stats_returns_403_for_member() {
        let (store, _admin_key) = setup_with_admin_key();
        let member_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let (_, key) =
                q::invite_user(&conn, &org, "m@acme.com", "M", "member").unwrap();
            key
        };

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/stats")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn get_org_returns_200() {
        let (store, key) = setup_with_admin_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/org")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn update_org_returns_200_for_admin() {
        let (store, key) = setup_with_admin_key();
        let body = serde_json::json!({ "name": "Acme Updated" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/admin/org")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn create_org_returns_201_with_valid_superuser_key() {
        let db = make_store();
        let body = serde_json::json!({
            "org_name": "New Corp",
            "org_slug": "new-corp",
            "admin_email": "admin@new.com",
            "admin_name": "Admin"
        });

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/orgs")
                    .header("Authorization", "Bearer test-superuser-key")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn create_org_returns_401_with_wrong_key() {
        let db = make_store();
        let body = serde_json::json!({
            "org_name": "New Corp",
            "org_slug": "new-corp",
            "admin_email": "admin@new.com",
            "admin_name": "Admin"
        });

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/orgs")
                    .header("Authorization", "Bearer wrong-key")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn create_org_returns_401_without_auth_header() {
        let db = make_store();
        let body = serde_json::json!({
            "org_name": "New Corp",
            "org_slug": "new-corp",
            "admin_email": "admin@new.com",
            "admin_name": "Admin"
        });

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/orgs")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
