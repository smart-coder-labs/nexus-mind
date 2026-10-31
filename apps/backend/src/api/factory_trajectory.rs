//! Read-only trajectory inspection and explicit human outcome/edit annotations.
//! This module has no connection to policy or execution decisions.

use axum::{extract::{Path, Query, State}, http::StatusCode, Extension, Json};
use serde::Deserialize;

use crate::{
    api::helpers::{require_explicit_permission, AppJson},
    db::factory_trajectory::{self, HumanEditMetadata, TrajectoryEntry, TrajectoryReviewItem},
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

type ApiResult<T> = Result<T, (StatusCode, Json<ApiError>)>;

#[derive(Deserialize)]
pub struct FeedbackQueueQuery {
    pub limit: Option<i64>,
}

fn error(status: StatusCode, code: &str, message: impl Into<String>) -> (StatusCode, Json<ApiError>) {
    (status, Json(ApiError { error: message.into(), code: code.to_string() }))
}

fn internal(error_value: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", error_value.to_string())
}

fn lock_error() -> (StatusCode, Json<ApiError>) {
    error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", "Database lock error")
}

pub async fn list_feedback_queue(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<FeedbackQueueQuery>,
) -> ApiResult<Json<Vec<TrajectoryReviewItem>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    let limit = query.limit.unwrap_or(50).clamp(1, 200);
    Ok(Json(
        factory_trajectory::list_needing_feedback(&conn, &auth.org_id, limit).map_err(internal)?,
    ))
}

pub async fn list_run_trajectory(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
) -> ApiResult<Json<Vec<TrajectoryEntry>>> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:read")?;
    Ok(Json(factory_trajectory::list_for_run(&conn, &auth.org_id, &run_id).map_err(internal)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanFeedbackRequest {
    /// The person's assessment of the completed change, independent of the
    /// agent's own specialist verdict.
    pub outcome: String,
    /// Optional diff metadata for code a person changed after the run. Raw
    /// file contents and patch hunks are intentionally not accepted.
    pub human_edit: Option<HumanEditMetadata>,
}

pub async fn record_human_feedback(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(run_id): Path<String>,
    AppJson(input): AppJson<HumanFeedbackRequest>,
) -> ApiResult<(StatusCode, Json<TrajectoryEntry>)> {
    if !matches!(input.outcome.as_str(), "accepted" | "rejected") {
        return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "outcome must be accepted or rejected"));
    }
    if let Some(edit) = input.human_edit.as_ref() {
        if edit.changed_files.len() > 500
            || edit.changed_files.iter().any(|path| {
                path.trim().is_empty()
                    || path.chars().count() > 1024
                    || path.chars().any(char::is_control)
            })
            || edit.additions > 10_000_000
            || edit.deletions > 10_000_000
        {
            return Err(error(StatusCode::UNPROCESSABLE_ENTITY, "validation_error", "human_edit must contain at most 500 valid paths and bounded diff counts"));
        }
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_explicit_permission(&conn, &auth, None, "factory_policy:write")?;
    let entry = match factory_trajectory::record_human_feedback(
        &conn,
        &auth.org_id,
        &run_id,
        &auth.user_id,
        &input.outcome,
        input.human_edit.as_ref(),
    ) {
        Ok(Some(entry)) => entry,
        Ok(None) => return Err(error(StatusCode::NOT_FOUND, "not_found", "Autonomous run not found")),
        Err(error_value) if error_value.to_string() == "run_not_terminal" => {
            return Err(error(StatusCode::CONFLICT, "run_not_terminal", "Human feedback requires a completed, blocked, or failed run"));
        }
        Err(error_value) => return Err(internal(error_value)),
    };
    Ok((StatusCode::CREATED, Json(entry)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_contract_rejects_arbitrary_patch_content() {
        assert!(serde_json::from_value::<HumanFeedbackRequest>(serde_json::json!({
            "outcome":"accepted",
            "human_edit":{"changed_files":["src/a.rs"],"additions":2,"deletions":1,"patch":"secret source"}
        })).is_err());
    }
}
