use axum::{extract::State, http::StatusCode, Extension, Json};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

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
pub struct BootstrapInput {
    pub org_name: String,
    pub org_slug: String,
    pub admin_email: String,
    pub admin_name: String,
}

pub async fn bootstrap(
    State(db): State<Arc<Mutex<Connection>>>,
    Json(input): Json<BootstrapInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    let conn = db.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        )
    })?;

    match queries::bootstrap(
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
        Err(e) if e.to_string().contains("already_bootstrapped") => Err((
            StatusCode::CONFLICT,
            Json(ApiError {
                error: "Organization already exists".to_string(),
                code: "already_bootstrapped".to_string(),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: e.to_string(),
                code: "internal_error".to_string(),
            }),
        )),
    }
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
        Router::new()
            .route("/v1/admin/stats", get(stats))
            .route("/v1/admin/org", get(get_org).patch(update_org))
            .layer(middleware::from_fn_with_state(db.clone(), auth_mw::auth))
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
}
