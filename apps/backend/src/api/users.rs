use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    api::helpers::{require_permission, user_is_visible_to_actor, AppJson},
    db::queries,
    models::types::{ApiError, AuthContext, User},
    store::sqlite::SqliteStore,
};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProjectAccess {
    All,
    Specific { project_ids: Vec<String> },
}

#[derive(Deserialize)]
pub struct UpdateUserRoleInput {
    pub role: String,
}

#[derive(Deserialize)]
pub struct InviteInput {
    pub email: String,
    pub name: Option<String>,
    pub role: Option<String>,
    pub project_access: Option<ProjectAccess>,
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
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let mut users = queries::list_users(&conn, &auth.org_id).map_err(db_err)?;
    if !auth.role.is_super_user() {
        users.retain(|user| user_is_visible_to_actor(&conn, &auth, &user.id).unwrap_or(false));
    }
    Ok(Json(users))
}

pub async fn invite(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<InviteInput>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "user:invite")?;

    let role = input.role.as_deref().unwrap_or("member");
    let name_fallback = input.email.split('@').next().unwrap_or("").to_string();
    let name = input.name.as_deref().unwrap_or(&name_fallback);

    let (user, api_key) =
        queries::invite_user(&conn, &auth.org_id, &input.email, name, role).map_err(db_err)?;

    // Resolve which project IDs to grant access to
    let project_ids: Vec<String> = match &input.project_access {
        None => Vec::new(),
        Some(ProjectAccess::All) if auth.role.is_super_user() => {
            queries::list_project_ids_for_org(&conn, &auth.org_id).map_err(db_err)?
        }
        Some(ProjectAccess::All) => {
            queries::list_project_ids_for_user(&conn, &auth.org_id, &auth.user_id)
                .map_err(db_err)?
        }
        Some(ProjectAccess::Specific { project_ids }) => {
            if !auth.role.is_super_user()
                && project_ids.iter().any(|id| {
                    !queries::user_is_project_member(&conn, &auth.org_id, id, &auth.user_id)
                        .unwrap_or(false)
                })
            {
                return Err(forbidden());
            }
            project_ids.clone()
        }
    };

    // Insert project_members rows for each resolved project
    for project_id in &project_ids {
        let member_id = Uuid::new_v4().to_string();
        let _ = conn.execute(
            "INSERT OR IGNORE INTO project_members (id, project_id, user_id, role, created_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            rusqlite::params![member_id, project_id, user.id, role],
        );
    }

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
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "user:revoke")?;
    if queries::get_user_by_id(&conn, &user_id)
        .map_err(db_err)?
        .is_some()
        && !user_is_visible_to_actor(&conn, &auth, &user_id).map_err(|e| db_err(e.into()))?
    {
        return Err(forbidden());
    }

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
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // Any role can rotate their own key; admin can rotate any key.
    if !auth.role.is_privileged() && auth.user_id != user_id {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    if queries::get_user_by_id(&conn, &user_id)
        .map_err(db_err)?
        .is_some()
        && !user_is_visible_to_actor(&conn, &auth, &user_id).map_err(|e| db_err(e.into()))?
    {
        return Err(forbidden());
    }
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

pub async fn update_role(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
    AppJson(input): AppJson<UpdateUserRoleInput>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_privileged() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    if queries::get_user_by_id(&conn, &user_id)
        .map_err(db_err)?
        .is_some()
        && !user_is_visible_to_actor(&conn, &auth, &user_id).map_err(|e| db_err(e.into()))?
    {
        return Err(forbidden());
    }

    let updated =
        queries::update_user_role(&conn, &auth.org_id, &user_id, &input.role).map_err(db_err)?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "User or role not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "update_role",
        "user",
        Some(&user_id),
        serde_json::json!({ "new_role": input.role }),
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
        routing::{delete, get, post},
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
        Router::new()
            .route("/v1/users", get(list))
            .route("/v1/users/invite", post(invite))
            .route("/v1/users/:id", delete(remove))
            .route("/v1/users/:id/rotate-key", post(rotate_key))
            .route("/v1/users/:id/role", axum::routing::patch(update_role))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
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
    async fn list_users_requires_admin() {
        let (store, _admin_key) = setup_with_admin_key();
        // Create a member user and use their key
        let member_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let (_, key) =
                q::invite_user(&conn, &org, "member@acme.com", "Member", "member").unwrap();
            key
        };

        let resp = app(store)
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
        let (store, admin_key) = setup_with_admin_key();

        let resp = app(store)
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
        let (store, admin_key) = setup_with_admin_key();
        let body = serde_json::json!({ "email": "new@acme.com", "name": "New User" });

        let resp = app(store)
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
    async fn invite_user_without_name_returns_201() {
        let (store, admin_key) = setup_with_admin_key();
        let body = serde_json::json!({ "email": "noname@acme.com" });

        let resp = app(store)
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
    async fn invite_user_omitted_project_access_grants_no_project_memberships() {
        let (store, admin_key) = setup_with_admin_key();
        let org_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            q::create_project(&conn, &org, "p1", None, None).unwrap();
            q::create_project(&conn, &org, "p2", None, None).unwrap();
            org
        };
        let body = serde_json::json!({ "email": "limited@acme.com", "name": "Limited User" });

        let resp = app(store.clone())
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
        let membership_count: i64 = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let invited_user_id: String = conn
                .query_row(
                    "SELECT id FROM users WHERE org_id = ?1 AND email = 'limited@acme.com'",
                    [&org_id],
                    |r| r.get(0),
                )
                .unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM project_members WHERE user_id = ?1",
                [invited_user_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(
            membership_count, 0,
            "omitted project_access must not imply all projects"
        );
    }

    #[tokio::test]
    async fn assigned_admin_all_project_access_only_grants_assigned_projects() {
        let (store, _bootstrap_key) = setup_with_admin_key();
        let (assigned_admin_key, project_a_id, project_b_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |row| row.get(0))
                .unwrap();
            let (admin, key) =
                q::invite_user(&conn, &org_id, "assigned@acme.com", "Assigned", "admin").unwrap();
            let project_a = q::create_project(&conn, &org_id, "assigned", None, None).unwrap();
            let project_b = q::create_project(&conn, &org_id, "unassigned", None, None).unwrap();
            q::upsert_project_member(&conn, &project_a.id, &admin.id, "admin").unwrap();
            (key, project_a.id, project_b.id)
        };

        let response = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users/invite")
                    .header("Authorization", format!("Bearer {assigned_admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "email": "scoped@acme.com",
                            "name": "Scoped",
                            "project_access": { "type": "all" }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);

        let db = store.conn();
        let conn = db.lock().unwrap();
        let invited_id: String = conn
            .query_row(
                "SELECT id FROM users WHERE email = 'scoped@acme.com'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let project_ids: Vec<String> = conn
            .prepare(
                "SELECT project_id FROM project_members WHERE user_id = ?1 ORDER BY project_id",
            )
            .unwrap()
            .query_map([invited_id], |row| row.get(0))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        assert_eq!(project_ids, vec![project_a_id]);
        assert!(!project_ids.contains(&project_b_id));
    }

    #[tokio::test]
    async fn invite_user_bad_json_returns_json_error() {
        let (store, admin_key) = setup_with_admin_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/users/invite")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert!(
            ct.contains("application/json"),
            "expected JSON content-type, got: {ct}"
        );
    }

    #[tokio::test]
    async fn remove_user_not_found_returns_404() {
        let (store, admin_key) = setup_with_admin_key();

        let resp = app(store)
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

    #[tokio::test]
    async fn scoped_admin_cannot_mutate_hidden_user_but_super_user_can() {
        let (store, super_user_key) = setup_with_admin_key();
        let (scoped_admin_key, hidden_user_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE users SET role = 'super_user' WHERE role = 'admin'",
                [],
            )
            .unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |row| row.get(0))
                .unwrap();
            let (scoped_admin, key) =
                q::invite_user(&conn, &org_id, "scoped@acme.com", "Scoped", "admin").unwrap();
            let (hidden, _) =
                q::invite_user(&conn, &org_id, "hidden@acme.com", "Hidden", "member").unwrap();
            let visible = q::create_project(&conn, &org_id, "visible", None, None).unwrap();
            q::upsert_project_member(&conn, &visible.id, &scoped_admin.id, "admin").unwrap();
            (key, hidden.id)
        };

        for (method, uri, body) in [
            ("DELETE", format!("/v1/users/{hidden_user_id}"), None),
            (
                "POST",
                format!("/v1/users/{hidden_user_id}/rotate-key"),
                None,
            ),
            (
                "PATCH",
                format!("/v1/users/{hidden_user_id}/role"),
                Some(serde_json::json!({"role": "viewer"})),
            ),
        ] {
            assert_eq!(
                app(store.clone())
                    .oneshot(
                        Request::builder()
                            .method(method)
                            .uri(uri)
                            .header("Authorization", format!("Bearer {scoped_admin_key}"))
                            .header("Content-Type", "application/json")
                            .body(
                                body.map(|value| Body::from(value.to_string()))
                                    .unwrap_or_else(Body::empty)
                            )
                            .unwrap()
                    )
                    .await
                    .unwrap()
                    .status(),
                StatusCode::FORBIDDEN
            );
        }

        let allowed = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/users/{hidden_user_id}/role"))
                    .header("Authorization", format!("Bearer {super_user_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"{"role":"viewer"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(allowed.status(), StatusCode::NO_CONTENT);
    }
}
