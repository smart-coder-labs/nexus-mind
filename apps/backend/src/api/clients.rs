//! Client CRUD and membership.
//!
//! Every read goes through [`queries::user_can_view_client`] and every denial
//! returns 404 via [`hidden_resource_not_found`], never 403 — a 403 would
//! confirm the resource exists, which is exactly what a competing client must
//! not learn.
//!
//! The visibility discriminator is `is_super_user()`, never `is_privileged()`:
//! admin is privileged for permission checks but stays membership-scoped for
//! reads, matching `api::context::viewer_scope`.

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
        validate_slug, AddClientMemberRequest, ApiError, AuthContext, Client, ClientMember,
        CreateClientRequest, UpdateClientRequest, CLIENT_STATUSES,
    },
    store::sqlite::SqliteStore,
};

fn lock_err() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "db lock poisoned".to_string(),
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

fn bad_request(msg: &str, code: &str) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::BAD_REQUEST,
        Json(ApiError {
            error: msg.to_string(),
            code: code.to_string(),
        }),
    )
}

/// `None` for super_user (no restriction), `Some(user_id)` for everyone else.
/// Deliberately mirrors `api::context::viewer_scope` — see the module note.
fn viewer_scope(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_super_user() {
        None
    } else {
        Some(&auth.user_id)
    }
}

fn validate_status(status: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    if CLIENT_STATUSES.contains(&status) {
        Ok(())
    } else {
        Err(bad_request(
            &format!("status must be one of {}", CLIENT_STATUSES.join(", ")),
            "invalid_status",
        ))
    }
}

#[derive(Deserialize)]
pub struct ListClientsParams {
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn list_clients(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListClientsParams>,
) -> Result<Json<Vec<Client>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:read")?;
    let clients = queries::list_clients_visible(
        &conn,
        &auth.org_id,
        params.include_archived,
        viewer_scope(&auth),
    )
    .map_err(db_err)?;
    Ok(Json(clients))
}

pub async fn create_client(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateClientRequest>,
) -> Result<(StatusCode, Json<Client>), (StatusCode, Json<ApiError>)> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 128 {
        return Err(bad_request("name must be 1–128 characters", "invalid_name"));
    }
    validate_slug(&input.slug).map_err(|e| bad_request(&e, "invalid_slug"))?;
    validate_status(&input.status)?;

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;

    let client = queries::insert_client(&conn, &auth.org_id, name, &input.slug, &input.status)
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "A client with that slug already exists".to_string(),
                        code: "client_conflict".to_string(),
                    }),
                )
            } else {
                db_err(e)
            }
        })?;
    Ok((StatusCode::CREATED, Json(client)))
}

/// Rejects a `slug` field outright rather than ignoring it: silently dropping a
/// field the caller believed they were changing is worse than an error.
pub async fn update_client(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(raw): AppJson<serde_json::Value>,
) -> Result<Json<Client>, (StatusCode, Json<ApiError>)> {
    if raw.get("slug").is_some() {
        return Err(bad_request(
            "slug is immutable; create a new client instead",
            "slug_immutable",
        ));
    }
    let input: UpdateClientRequest =
        serde_json::from_value(raw).map_err(|e| bad_request(&e.to_string(), "invalid_body"))?;
    if let Some(status) = input.status.as_deref() {
        validate_status(status)?;
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;
    require_visible_client(&conn, &auth, &id, "PATCH")?;

    match queries::update_client(
        &conn,
        &auth.org_id,
        &id,
        input.name.as_deref(),
        input.status.as_deref(),
    )
    .map_err(db_err)?
    {
        Some(client) => Ok(Json(client)),
        None => Err(not_found()),
    }
}

pub async fn archive_client(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;
    require_visible_client(&conn, &auth, &id, "POST")?;
    if queries::archive_client(&conn, &auth.org_id, &id).map_err(db_err)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

/// Deleting a client that still owns projects is refused with 422. Offboarding
/// is a status transition (`status = 'offboarded'`), never a cascade that would
/// take a client's project history with it.
pub async fn delete_client(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;
    require_visible_client(&conn, &auth, &id, "DELETE")?;

    let owned = queries::count_client_projects(&conn, &auth.org_id, &id).map_err(db_err)?;
    if owned > 0 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: format!(
                    "client still owns {owned} project(s); archive or reassign them first"
                ),
                code: "client_has_projects".to_string(),
            }),
        ));
    }
    conn.execute(
        "DELETE FROM clients WHERE org_id = ?1 AND id = ?2",
        rusqlite::params![auth.org_id, id],
    )
    .map_err(|e| db_err(e.into()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_members(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Vec<ClientMember>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:read")?;
    require_visible_client(&conn, &auth, &id, "GET")?;
    let members = queries::list_client_members(&conn, &id).map_err(db_err)?;
    Ok(Json(members))
}

pub async fn add_member(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<AddClientMemberRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;
    require_visible_client(&conn, &auth, &id, "POST")?;
    queries::add_client_member(&conn, &id, &input.user_id, &input.role).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_member(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    require_permission(&conn, &auth, None, "client:write")?;
    require_visible_client(&conn, &auth, &id, "DELETE")?;
    if queries::remove_client_member(&conn, &id, &user_id).map_err(db_err)? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Client not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

/// 404 (never 403) when the client is hidden from this caller, with an audit
/// row recording the denied attempt.
fn require_visible_client(
    conn: &rusqlite::Connection,
    auth: &AuthContext,
    client_id: &str,
    method: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    let visible = queries::user_can_view_client(conn, &auth.org_id, client_id, viewer_scope(auth))
        .map_err(db_err)?;
    if visible {
        Ok(())
    } else {
        Err(hidden_resource_not_found(
            conn, auth, "client", client_id, method, "clients",
        ))
    }
}
