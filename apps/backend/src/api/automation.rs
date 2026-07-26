use axum::{extract::State, http::StatusCode, Extension, Json};
use serde::{Deserialize, Serialize};

use crate::{
    api::helpers::{require_permission, AppJson},
    automation::{
        policy::{resolve_execution, AuthorizationRequest, AuthorizationStatus},
        profiles::{managed_profiles, CLAUDE_CODE_PROVIDER},
        provenance::ProfileProvenance,
    },
    models::types::{ApiError, AuthContext},
    store::sqlite::SqliteStore,
};

fn lock_error() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

#[derive(Serialize)]
pub struct ProfilesResponse {
    pub profiles: Vec<crate::automation::profiles::ExecutionProfileVersion>,
}

/// Repository and worker payloads cannot provide allowlists or extension
/// authority. Those values will be resolved only from persisted bindings.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorizeProfileRequest {
    pub profile: String,
    #[serde(default)]
    pub requested_capabilities: Vec<String>,
}

#[derive(Serialize)]
pub struct AuthorizeProfileResponse {
    pub status: AuthorizationStatus,
    pub reason: Option<String>,
    pub provenance: Option<ProfileProvenance>,
}

/// `GET /v1/automation/profiles` lists centrally managed, pinned profiles.
pub async fn list_profiles(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<ProfilesResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, "automation:read")?;
    Ok(Json(ProfilesResponse {
        profiles: managed_profiles(),
    }))
}

/// `POST /v1/automation/authorize` fails closed until a future lease resolver
/// supplies organization and project bindings from durable policy storage.
pub async fn authorize_profile(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(request): AppJson<AuthorizeProfileRequest>,
) -> Result<Json<AuthorizeProfileResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_error())?;
    require_permission(&conn, &auth, None, "automation:write")?;

    let decision = resolve_execution(
        &AuthorizationRequest {
            provider: CLAUDE_CODE_PROVIDER.to_string(),
            requested_profile: request.profile,
            organization_allowed_profiles: Vec::new(),
            project_allowed_profiles: Vec::new(),
            requested_capabilities: request.requested_capabilities,
            extensions: Vec::new(),
        },
        &managed_profiles(),
    );
    Ok(Json(AuthorizeProfileResponse {
        status: decision.status,
        reason: decision.reason,
        provenance: decision.provenance,
    }))
}
