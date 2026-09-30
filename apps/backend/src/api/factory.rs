//! Software factory control plane: per-action policies (docs/factory/PLAN.md §4).
//!
//! Policies decide how much autonomy agents get, so they are edited only here,
//! behind the explicit `factory_policy:*` permissions (no admin-role bypass), and
//! are never exposed through MCP.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    db::factory_queries::{self, PolicyWriteError, StoredPolicy},
    factory::contracts::ActionPolicy,
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

fn error(
    status: StatusCode,
    code: &str,
    message: impl Into<String>,
) -> (StatusCode, Json<ApiError>) {
    (
        status,
        Json(ApiError {
            error: message.into(),
            code: code.to_string(),
        }),
    )
}

fn internal(error_value: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        error_value.to_string(),
    )
}

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "Database lock error",
    )
}

pub async fn list_policies(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> ApiResult<Json<Vec<StoredPolicy>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(
        factory_queries::list_factory_policies(&conn, &auth.org_id).map_err(internal)?,
    ))
}

/// Upserts the policy for `(action, scope)` and its audit record atomically. `version` must be 1 for a new policy
/// and exactly `current + 1` for an existing one; anything else is a 409, so two
/// admins editing the same policy cannot silently overwrite each other.
pub async fn put_policy(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(policy): AppJson<ActionPolicy>,
) -> ApiResult<Json<StoredPolicy>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let stored =
        match factory_queries::upsert_factory_policy(&conn, &auth.org_id, &auth.user_id, &policy) {
            Ok((stored, _previous)) => stored,
            Err(PolicyWriteError::Invalid(reason)) => {
                return Err(error(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_policy",
                    reason,
                ))
            }
            Err(PolicyWriteError::VersionConflict { current }) => {
                return Err(error(
                    StatusCode::CONFLICT,
                    "policy_version_conflict",
                    match current {
                        Some(current) => format!(
                            "Policy is at version {current}; send version {}",
                            current + 1
                        ),
                        None => "New policies start at version 1".to_string(),
                    },
                ))
            }
            Err(PolicyWriteError::Db(db_error)) => return Err(internal(db_error)),
        };
    Ok(Json(stored))
}

pub async fn delete_policy(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> ApiResult<StatusCode> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    match factory_queries::delete_factory_policy(&conn, &auth.org_id, &auth.user_id, &id)
        .map_err(internal)?
    {
        Some(_) => Ok(StatusCode::NO_CONTENT),
        None => Err(error(
            StatusCode::NOT_FOUND,
            "policy_not_found",
            "Policy not found",
        )),
    }
}
