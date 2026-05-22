use axum::{extract::State, http::StatusCode, Extension, Json};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

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
    models::types::{ApiError, AuthContext, Org, OrgStats},
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
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(superuser_key): Extension<Option<String>>,
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

    let conn = db.lock().map_err(|_| lock_err())?;

    match queries::create_org(
        &conn,
        &input.org_name,
        &input.org_slug,
        &input.admin_email,
        &input.admin_name,
    ) {
        Ok((org, user, api_key)) => {
            let body = serde_json::json!({
                "org": org,
                "user": user,
                "api_key": api_key,
            });
            Ok((StatusCode::CREATED, Json(body)))
        }
        Err(e) if e.to_string().contains("UNIQUE constraint failed") => Err((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "Organization slug already exists".to_string(),
                code: "slug_conflict".to_string(),
            }),
        )),
        Err(e) => Err(db_err(e)),
    }
}

pub async fn list_orgs(
    State(db): State<Arc<Mutex<Connection>>>,
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

    let conn = db.lock().map_err(|_| lock_err())?;
    let orgs = queries::list_orgs(&conn).map_err(db_err)?;
    Ok(Json(orgs))
}

pub async fn stats(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<OrgStats>, (StatusCode, Json<ApiError>)> {
    if auth.role != "admin" {
        return Err(forbidden());
    }
    let conn = db.lock().map_err(|_| lock_err())?;
    let s = queries::get_stats(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(s))
}

pub async fn get_org(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
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
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<UpdateOrgInput>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
    if auth.role != "admin" {
        return Err(forbidden());
    }
    let conn = db.lock().map_err(|_| lock_err())?;
    let org = queries::update_org_name(&conn, &auth.org_id, &input.name).map_err(db_err)?;
    Ok(Json(org))
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
    };

    fn make_db() -> Arc<Mutex<Connection>> {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn app(db: Arc<Mutex<Connection>>) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());

        let protected = Router::new()
            .route("/v1/admin/stats", get(stats))
            .route("/v1/admin/org", get(get_org).patch(update_org))
            .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth));

        Router::new()
            .route("/v1/orgs", post(create_org))
            .merge(protected)
            .layer(Extension(superuser_key))
            .with_state(db)
    }

    fn setup_with_admin_key() -> (Arc<Mutex<Connection>>, String) {
        let db = make_db();
        let raw_key = {
            let conn = db.lock().unwrap();
            let (_, _, key) =
                q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (db, raw_key)
    }

    #[tokio::test]
    async fn stats_returns_200_for_admin() {
        let (db, key) = setup_with_admin_key();

        let resp = app(db)
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
        let (db, _admin_key) = setup_with_admin_key();
        let member_key = {
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let (_, key) =
                q::invite_user(&conn, &org, "m@acme.com", "M", "member").unwrap();
            key
        };

        let resp = app(db)
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
        let (db, key) = setup_with_admin_key();

        let resp = app(db)
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
        let (db, key) = setup_with_admin_key();
        let body = serde_json::json!({ "name": "Acme Updated" });

        let resp = app(db)
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
        let db = make_db();
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
        let db = make_db();
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
        let db = make_db();
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
