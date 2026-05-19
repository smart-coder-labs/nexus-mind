use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use rusqlite::Connection;
use serde::Deserialize;
use std::sync::{Arc, Mutex};

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, User},
};

#[derive(Deserialize)]
pub struct InviteInput {
    pub email: String,
    pub name: String,
    pub role: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateOrgInput {
    pub name: String,
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

pub async fn list(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
    if auth.role != "admin" {
        return Err(forbidden());
    }

    let conn = db.lock().map_err(|_| lock_err())?;
    let users = queries::list_users(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(users))
}

pub async fn invite(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Json(input): Json<InviteInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    if auth.role != "admin" {
        return Err(forbidden());
    }

    let conn = db.lock().map_err(|_| lock_err())?;
    let role = input.role.as_deref().unwrap_or("member");

    let (user, api_key) =
        queries::invite_user(&conn, &auth.org_id, &input.email, &input.name, role)
            .map_err(db_err)?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "invite",
        "user",
        Some(&user.id),
        serde_json::json!({ "email": input.email, "role": role }),
    );

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "user": user, "api_key": api_key })),
    ))
}

pub async fn remove(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if auth.role != "admin" {
        return Err(forbidden());
    }

    let conn = db.lock().map_err(|_| lock_err())?;
    let suspended = queries::suspend_user(&conn, &auth.org_id, &user_id).map_err(db_err)?;

    if !suspended {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "User not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "revoke",
        "user",
        Some(&user_id),
        serde_json::json!({}),
    );

    Ok(StatusCode::NO_CONTENT)
}

pub async fn rotate_key(
    State(db): State<Arc<Mutex<Connection>>>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Any role can rotate their own key; admin can rotate any key.
    if auth.role != "admin" && auth.user_id != user_id {
        return Err(forbidden());
    }

    let conn = db.lock().map_err(|_| lock_err())?;
    let new_key = queries::rotate_key(&conn, &auth.org_id, &user_id).map_err(db_err)?;

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "rotate_key",
        "user",
        Some(&user_id),
        serde_json::json!({}),
    );

    Ok(Json(serde_json::json!({ "api_key": new_key })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{delete, get, post},
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
            .route("/v1/users", get(list))
            .route("/v1/users/invite", post(invite))
            .route("/v1/users/:id", delete(remove))
            .route("/v1/users/:id/rotate-key", post(rotate_key))
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
    async fn list_users_requires_admin() {
        let (db, _admin_key) = setup_with_admin_key();
        // Create a member user and use their key
        let member_key = {
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let (_, key) =
                q::invite_user(&conn, &org, "member@acme.com", "Member", "member").unwrap();
            key
        };

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .uri("/v1/users")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn list_users_returns_200_for_admin() {
        let (db, admin_key) = setup_with_admin_key();

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .uri("/v1/users")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn invite_user_returns_201() {
        let (db, admin_key) = setup_with_admin_key();
        let body = serde_json::json!({ "email": "new@acme.com", "name": "New User" });

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users/invite")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn remove_user_not_found_returns_404() {
        let (db, admin_key) = setup_with_admin_key();

        let resp = app(db)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/users/nonexistent-user-id")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
