use axum::{extract::State, extract::Path, extract::Query, http::StatusCode, Extension, Json};
use crate::api::helpers::AppJson;
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
    config::Config,
    db::queries,
    models::types::{AgentActivity, ApiError, ApiKeyWithUser, AssignCollectionRequest, AuthContext, BulkTagRequest, BulkTagResponse, Collection, ContributorStat, CreateCollectionRequest, CreateInviteLinkRequest, HeatmapDay, ImportConfigResponse, ImportMemoriesRequest, ImportMemoriesResponse, InviteLinkResponse, Memory, MemoryFacets, MergeMemoriesRequest, MemoryTrends, NameCount, NotificationItem, Org, OrgSettings, OrgStats, OnboardingStatus, RenameTagRequest, RenameTagResponse, ResetKeyResponse, RetentionPreview, ScheduleDeleteRequest, StoreMemoryRequest, UpdateAnnouncementRequest, UpdateNoteRequest, UpdateOrgLogoRequest, UpdateUserNoteRequest, UsageStats, User, CustomRole, Project, ProjectMember, ProjectEventOverrides, UpdateProjectEventOverridesRequest, ProjectStats},
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
    AppJson(input): AppJson<CreateOrgInput>,
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

pub async fn list_org_users(
    State(store): State<SqliteStore>,
    Extension(superuser_key): Extension<Option<String>>,
    headers: axum::http::HeaderMap,
    Path(org_id): Path<String>,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
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
    let users = queries::list_users(&conn, &org_id).map_err(db_err)?;
    Ok(Json(users))
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

pub async fn usage_stats(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<UsageStats>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let stats = queries::get_usage_stats(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(stats))
}

pub async fn memory_facets(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<MemoryFacets>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let facets = queries::get_memory_facets(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(facets))
}

#[derive(Deserialize, Default)]
pub struct DaysParam {
    pub days: Option<i64>,
}

pub async fn get_memory_trends_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<DaysParam>,
) -> Result<Json<MemoryTrends>, (StatusCode, Json<ApiError>)> {
    let days = params.days.unwrap_or(30).clamp(1, 365);
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let trends = queries::get_memory_trends(&conn, &auth.org_id, days).map_err(db_err)?;
    Ok(Json(trends))
}

pub async fn get_tag_stats_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<NameCount>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let tags = queries::get_tag_stats(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(tags))
}

pub async fn get_onboarding(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<OnboardingStatus>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let status = queries::get_onboarding_status(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(status))
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
    AppJson(input): AppJson<UpdateOrgInput>,
) -> Result<Json<Org>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let org = queries::update_org_name(&conn, &auth.org_id, &input.name).map_err(db_err)?;
    Ok(Json(org))
}

pub async fn get_org_settings_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<OrgSettings>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let settings = queries::get_org_settings(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(settings))
}

pub async fn update_org_settings_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<OrgSettings>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Read current settings first, then merge only the fields present in the
    // request body — standard PATCH semantics (absent keys are preserved).
    let mut settings = queries::get_org_settings(&conn, &auth.org_id).map_err(db_err)?;

    if let Some(val) = body.get("retention_days") {
        settings.retention_days = if val.is_null() { None } else { val.as_i64() };
    }
    if let Some(val) = body.get("custom_instructions") {
        settings.custom_instructions = val.as_str().map(|s| s.to_string());
    }
    if let Some(val) = body.get("min_password_length") {
        settings.min_password_length = val.as_i64();
    }
    if let Some(val) = body.get("announcement") {
        settings.announcement = val.as_str().map(|s| s.to_string());
    }
    if let Some(val) = body.get("announcement_type") {
        settings.announcement_type = val.as_str().map(|s| s.to_string());
    }
    if let Some(val) = body.get("logo_url") {
        settings.logo_url = val.as_str().map(|s| s.to_string());
    }
    if let Some(val) = body.get("events") {
        if let Ok(parsed) = serde_json::from_value::<crate::models::types::AgentEventSettings>(val.clone()) {
            settings.events = parsed;
        }
    }

    let updated = queries::update_org_settings(&conn, &auth.org_id, &settings).map_err(db_err)?;
    Ok(Json(updated))
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
    AppJson(input): AppJson<CreateRoleInput>,
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

const PROJECT_NAME_MAX_LEN: usize = 100;

fn validate_project_name(name: &str) -> Result<(), (StatusCode, Json<ApiError>)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Project name must not be empty or whitespace-only".to_string(),
                code: "validation_error".to_string(),
            }),
        ));
    }
    if trimmed.len() > PROJECT_NAME_MAX_LEN {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: format!("Project name must not exceed {} characters", PROJECT_NAME_MAX_LEN),
                code: "validation_error".to_string(),
            }),
        ));
    }
    if trimmed.chars().any(|c| c.is_control()) {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Project name must not contain control characters".to_string(),
                code: "validation_error".to_string(),
            }),
        ));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct CreateProjectInput {
    pub name: String,
    pub description: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateProjectInput {
    pub parent_id: Option<String>,
}

#[derive(Deserialize)]
pub struct UpsertProjectMemberInput {
    pub user_id: String,
    pub role: String,
}

#[derive(Deserialize, Default)]
pub struct ListProjectsParams {
    #[serde(default)]
    pub include_archived: bool,
}

pub async fn list_projects_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListProjectsParams>,
) -> Result<Json<Vec<Project>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let projects = queries::list_projects_filtered(&conn, &auth.org_id, params.include_archived).map_err(db_err)?;
    Ok(Json(projects))
}

pub async fn create_project_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<CreateProjectInput>,
) -> Result<(StatusCode, Json<Project>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    validate_project_name(&input.name)?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let project = queries::create_project(&conn, &auth.org_id, &input.name, input.description.as_deref(), input.parent_id.as_deref())
        .map_err(|e| {
            if e.to_string().contains("UNIQUE constraint failed") {
                (
                    StatusCode::CONFLICT,
                    Json(ApiError {
                        error: "Project name already exists in this organization".to_string(),
                        code: "project_conflict".to_string(),
                    }),
                )
            } else {
                db_err(e)
            }
        })?;
    Ok((StatusCode::CREATED, Json(project)))
}

pub async fn delete_project_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let deleted = queries::delete_project(&conn, &auth.org_id, &id).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

pub async fn archive_project_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let found = queries::archive_project(&conn, &auth.org_id, &id).map_err(db_err)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

pub async fn restore_project_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let found = queries::restore_project(&conn, &auth.org_id, &id).map_err(db_err)?;
    if found {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

pub async fn update_project_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project_id): Path<String>,
    AppJson(input): AppJson<UpdateProjectInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let found = queries::update_project(&conn, &auth.org_id, &project_id, input.parent_id.as_deref())
        .map_err(db_err)?;

    if !found {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn list_project_members_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project_id): Path<String>,
) -> Result<Json<Vec<ProjectMember>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    
    // Security check: ensure the project belongs to the user's org!
    let project_belongs = conn.query_row(
        "SELECT count(*) FROM projects WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![project_id, auth.org_id],
        |row| row.get::<_, i32>(0),
    ).map_err(|_| lock_err())? > 0;

    if !project_belongs {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let members = queries::list_project_members(&conn, &auth.org_id, &project_id).map_err(db_err)?;
    Ok(Json(members))
}

pub async fn upsert_project_member_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project_id): Path<String>,
    AppJson(input): AppJson<UpsertProjectMemberInput>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Security check: ensure the project belongs to the user's org!
    let project_belongs = conn.query_row(
        "SELECT count(*) FROM projects WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![project_id, auth.org_id],
        |row| row.get::<_, i32>(0),
    ).map_err(|_| lock_err())? > 0;

    if !project_belongs {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    // Security check: ensure the user to add belongs to the user's org!
    let user_belongs = conn.query_row(
        "SELECT count(*) FROM users WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![input.user_id, auth.org_id],
        |row| row.get::<_, i32>(0),
    ).map_err(|_| lock_err())? > 0;

    if !user_belongs {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "User not found in this organization".to_string(),
                code: "user_not_found".to_string(),
            }),
        ));
    }

    // Validate the role name (should be standard role or custom role)
    let role_valid = match input.role.parse::<crate::models::types::Role>() {
        Ok(_) => true,
        Err(_) => {
            conn.query_row(
                "SELECT count(*) FROM roles WHERE name = ?1 AND (org_id = ?2 OR org_id IS NULL)",
                rusqlite::params![&input.role, &auth.org_id],
                |row| row.get::<_, i32>(0),
            ).unwrap_or(0) > 0
        }
    };
    if !role_valid {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: format!("Role '{}' is not valid", input.role),
                code: "invalid_role".to_string(),
            }),
        ));
    }

    queries::upsert_project_member(&conn, &project_id, &input.user_id, &input.role).map_err(db_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn delete_project_member_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path((project_id, user_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    // Security check: ensure the project belongs to the user's org!
    let project_belongs = conn.query_row(
        "SELECT count(*) FROM projects WHERE id = ?1 AND org_id = ?2",
        rusqlite::params![project_id, auth.org_id],
        |row| row.get::<_, i32>(0),
    ).map_err(|_| lock_err())? > 0;

    if !project_belongs {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let deleted = queries::delete_project_member(&conn, &project_id, &user_id).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project membership not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// GET /v1/admin/keys — list all non-revoked API keys in the org (admin-only).
pub async fn list_org_keys(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<ApiKeyWithUser>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let keys = queries::list_all_org_keys(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(keys))
}

/// DELETE /v1/admin/keys/:key_id — admin revokes any key in the org.
pub async fn revoke_org_key(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(key_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let revoked = queries::revoke_key_admin(&conn, &auth.org_id, &key_id).map_err(db_err)?;
    if revoked {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "API key not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

// ── Per-project agent event override handlers ─────────────────────────────────

/// `GET /v1/projects/:id/settings` — returns current event overrides for a project.
/// Admin-only.
pub async fn get_project_settings_api(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectEventOverrides>, (StatusCode, Json<ApiError>)> {
    if !ctx.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let overrides = queries::get_project_event_overrides(&conn, &ctx.org_id, &project_id)
        .map_err(db_err)?;
    Ok(Json(overrides))
}

/// `PATCH /v1/projects/:id/settings` — updates event overrides for a project.
/// Admin-only.
pub async fn update_project_settings_api(
    State(store): State<SqliteStore>,
    Extension(ctx): Extension<AuthContext>,
    Path(project_id): Path<String>,
    AppJson(body): AppJson<UpdateProjectEventOverridesRequest>,
) -> Result<Json<ProjectEventOverrides>, (StatusCode, Json<ApiError>)> {
    if !ctx.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let saved = queries::update_project_event_overrides(&conn, &ctx.org_id, &project_id, body.overrides)
        .map_err(|e| {
            if e.to_string().contains("project not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Project not found".to_string(),
                        code: "not_found".to_string(),
                    }),
                )
            } else {
                db_err(e)
            }
        })?;
    Ok(Json(saved))
}

/// `POST /v1/admin/users/:user_id/reset-key`
/// Admin-only. Revokes the user's current key and issues a new one.
/// Returns the new raw key — only shown once, caller must display it immediately.
/// Returns 404 if the user doesn't exist; 400 if the key is a protected demo key.
pub async fn reset_user_key(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<Json<ResetKeyResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    match queries::reset_user_key(&conn, &auth.org_id, &user_id).map_err(db_err)? {
        Ok(new_key) => {
            let _ = queries::log_audit(
                &conn,
                &auth.org_id,
                &auth.user_id,
                "reset_key",
                "user",
                Some(&user_id),
                serde_json::json!({}),
            );
            Ok(Json(ResetKeyResponse { new_key }))
        }
        Err("not_found") => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "User not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Err("demo_key") => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "demo key cannot be reset".to_string(),
                code: "demo_key".to_string(),
            }),
        )),
        Err(_) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Unexpected error".to_string(),
                code: "internal_error".to_string(),
            }),
        )),
    }
}

/// `GET /v1/admin/stats/duplicates` — returns groups of memories that share the same
/// normalized hash (exact duplicate content). Admin-only.
pub async fn get_duplicates(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<Vec<Memory>>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let groups = queries::get_duplicate_groups(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(groups))
}

// ── Notification handler ──────────────────────────────────────────────────────

/// `GET /v1/admin/notifications?limit=15` — admin-only.
/// Derives notifications from recent audit log events (no new table needed).
/// Returns the most recent `limit` events that map to notable admin actions.
pub async fn get_notifications(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<NotificationsParams>,
) -> Result<Json<Vec<NotificationItem>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let limit = params.limit.unwrap_or(15).clamp(1, 50);

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let actions = [
        "user.created", "user.suspended", "user.deleted",
        "key.revoked", "key.reset",
        "project.deleted", "project.archived",
        "memory.bulk_deleted", "memory.imported",
        "webhook.created", "webhook.deleted",
        "invite.created", "invite.redeemed",
    ];

    // Build IN clause placeholders.
    let placeholders: Vec<String> = (1..=actions.len()).map(|i| format!("?{}", i + 2)).collect();
    let in_clause = placeholders.join(", ");

    let sql = format!(
        "SELECT al.id, al.action, al.resource_type, al.resource_id, al.user_id,
                al.timestamp, u.name, u.email
         FROM audit_logs al
         LEFT JOIN users u ON al.user_id = u.id AND al.org_id = u.org_id
         WHERE al.org_id = ?1
           AND al.action IN ({in_clause})
         ORDER BY al.timestamp DESC
         LIMIT ?2"
    );

    let mut stmt = conn.prepare(&sql).map_err(|e| db_err(e.into()))?;

    // Bind all parameters: org_id, limit, then each action string.
    let mut raw_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(auth.org_id.clone()),
        Box::new(limit),
    ];
    for action in &actions {
        raw_params.push(Box::new(action.to_string()));
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = raw_params.iter().map(|b| b.as_ref()).collect();

    let items = stmt
        .query_map(params_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,   // id
                row.get::<_, String>(1)?,   // action
                row.get::<_, Option<String>>(2)?, // resource_type
                row.get::<_, Option<String>>(3)?, // resource_id
                row.get::<_, String>(4)?,   // user_id
                row.get::<_, String>(5)?,   // timestamp
                row.get::<_, Option<String>>(6)?, // user name
                row.get::<_, Option<String>>(7)?, // user email
            ))
        })
        .map_err(|e| db_err(e.into()))?
        .filter_map(|r| r.ok())
        .map(|(id, action, resource_type, _resource_id, _user_id, timestamp, user_name, user_email)| {
            let message = action_to_message(&action);
            let actor = user_name.or(user_email);
            NotificationItem {
                id,
                message: message.to_string(),
                action,
                resource_type,
                created_at: timestamp,
                actor,
            }
        })
        .collect::<Vec<_>>();

    Ok(Json(items))
}

#[derive(serde::Deserialize)]
pub struct NotificationsParams {
    pub limit: Option<i64>,
}

fn action_to_message(action: &str) -> &'static str {
    match action {
        "user.created"        => "New user joined",
        "user.suspended"      => "User suspended",
        "user.deleted"        => "User deleted",
        "key.revoked"         => "API key revoked",
        "key.reset"           => "API key reset",
        "project.deleted"     => "Project deleted",
        "project.archived"    => "Project archived",
        "memory.bulk_deleted" => "Bulk memory delete",
        "memory.imported"     => "Memories imported",
        "webhook.created"     => "Webhook created",
        "webhook.deleted"     => "Webhook deleted",
        "invite.created"      => "Invite link created",
        "invite.redeemed"     => "Invite accepted",
        _                     => "Admin action",
    }
}

// ── Invite link handlers ──────────────────────────────────────────────────────

/// POST /v1/admin/invites — admin-only: create a one-time invite link (7-day expiry).
pub async fn create_invite_link(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Extension(config): Extension<Arc<Config>>,
    AppJson(input): AppJson<CreateInviteLinkRequest>,
) -> Result<(StatusCode, Json<InviteLinkResponse>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let role = input.role.unwrap_or_else(|| "user".to_string());

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let invite = queries::create_invite_link(&conn, &auth.org_id, &role, &auth.user_id)
        .map_err(db_err)?;

    let base = config.app_base_url.trim_end_matches('/');
    let invite_url = format!("{}/set-password?invite={}", base, invite.token);

    Ok((
        StatusCode::CREATED,
        Json(InviteLinkResponse {
            token: invite.token,
            invite_url,
            expires_at: invite.expires_at,
            role,
        }),
    ))
}

/// GET /v1/invites/:token — public (no auth required): validate an invite token.
/// Always returns 200 with { valid: bool, role?, org_name?, reason? } for easy UI handling.
pub async fn get_invite_link(
    State(store): State<SqliteStore>,
    Path(token): Path<String>,
) -> Json<serde_json::Value> {
    let db = store.conn();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return Json(serde_json::json!({ "valid": false, "reason": "server_error" })),
    };

    match queries::get_invite_link(&conn, &token) {
        Ok(invite) => {
            // Look up the org name for the welcome message
            let org_name = queries::get_org(&conn, &invite.org_id)
                .ok()
                .flatten()
                .map(|o| o.name)
                .unwrap_or_default();
            Json(serde_json::json!({
                "valid": true,
                "role": invite.role,
                "org_id": invite.org_id,
                "org_name": org_name,
            }))
        }
        Err(e) => {
            let msg = e.to_string();
            let reason = if msg.contains("invite_not_found") {
                "not_found"
            } else if msg.contains("invite_already_used") {
                "used"
            } else if msg.contains("invite_expired") {
                "expired"
            } else {
                "server_error"
            };
            Json(serde_json::json!({ "valid": false, "reason": reason }))
        }
    }
}

#[derive(Deserialize)]
pub struct RedeemInviteInput {
    pub password: String,
    pub name: String,
}

/// POST /v1/invites/:token/redeem — public (no auth required): accept an invite.
/// Creates a new user with the given name and password, returns { api_key }.
pub async fn redeem_invite(
    State(store): State<SqliteStore>,
    Path(token): Path<String>,
    AppJson(input): AppJson<RedeemInviteInput>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    use crate::auth::password::hash_password;

    if input.name.trim().is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Name is required".to_string(),
                code: "name_required".to_string(),
            }),
        ));
    }
    if input.password.len() < 8 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Password must be at least 8 characters".to_string(),
                code: "password_too_short".to_string(),
            }),
        ));
    }

    let password_hash = hash_password(&input.password).map_err(db_err)?;

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    match queries::redeem_invite(&conn, &token, &input.name, &password_hash) {
        Ok((_user, raw_key)) => Ok(Json(serde_json::json!({ "api_key": raw_key }))),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("invite_not_found") {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Invite not found".to_string(),
                        code: "invite_not_found".to_string(),
                    }),
                ))
            } else if msg.contains("invite_already_used") {
                Err((
                    StatusCode::GONE,
                    Json(ApiError {
                        error: "Invite link has already been used".to_string(),
                        code: "invite_already_used".to_string(),
                    }),
                ))
            } else if msg.contains("invite_expired") {
                Err((
                    StatusCode::GONE,
                    Json(ApiError {
                        error: "Invite link has expired".to_string(),
                        code: "invite_expired".to_string(),
                    }),
                ))
            } else {
                Err(db_err(e))
            }
        }
    }
}

/// `GET /v1/admin/stats/memory-heatmap` — admin-only.
/// Returns per-day memory creation counts for the requested period (default 90 days).
pub async fn get_memory_heatmap(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Vec<HeatmapDay>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let days = params.days.unwrap_or(90).clamp(1, 365);
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let heatmap = queries::get_memory_heatmap(&conn, &auth.org_id, days).map_err(db_err)?;
    Ok(Json(heatmap))
}

/// `GET /v1/admin/stats/top-contributors` — admin-only.
/// Returns the top contributing agents by memory count for the requested period (default 30 days).
pub async fn get_top_contributors(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Vec<ContributorStat>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let days = params.days.unwrap_or(30).clamp(1, 365);
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let contributors = queries::get_top_contributors(&conn, &auth.org_id, days).map_err(db_err)?;
    Ok(Json(contributors))
}

/// `GET /v1/admin/stats/agent-activity` — admin-only.
/// Returns the list of active tools for the requested period (default 30 days).
pub async fn get_agent_activity(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<DaysParam>,
) -> Result<Json<Vec<AgentActivity>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let days = params.days.unwrap_or(30).clamp(1, 365);
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let activity = queries::get_agent_activity(&conn, &auth.org_id, days).map_err(db_err)?;
    Ok(Json(activity))
}

/// `POST /v1/admin/memories/merge` — admin-only.
/// Appends `merge_id`'s content to `keep_id`'s content, then deletes `merge_id`.
/// Both memories must belong to the authenticated org.
pub async fn merge_memories(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<MergeMemoriesRequest>,
) -> Result<Json<Memory>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    if body.keep_id == body.merge_id {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "keep_id and merge_id must be different".to_string(),
                code: "same_id".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    queries::merge_memories(&conn, &auth.org_id, &body.keep_id, &body.merge_id)
        .map(Json)
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: msg,
                        code: "not_found".to_string(),
                    }),
                )
            } else {
                db_err(e)
            }
        })
}

/// `POST /v1/admin/memories/import` — admin-only batch import.
/// Accepts a raw JSON array `[...]` or the wrapper form `{ memories: [...] }`.
/// Returns imported/skipped/errors counts.
pub async fn import_memories(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<ImportMemoriesRequest>,
) -> Result<(StatusCode, Json<ImportMemoriesResponse>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    if body.memories.len() > 1000 {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Batch size exceeds limit of 1000 memories".to_string(),
                code: "batch_too_large".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let user_id = auth.user_id.clone();

    let mut imported = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for (idx, mem) in body.memories.into_iter().enumerate() {
        if mem.content.trim().is_empty() {
            skipped += 1;
            continue;
        }

        let req = StoreMemoryRequest {
            project: mem.project,
            tool: "admin-import".to_string(),
            content: mem.content,
            tags: mem.tags,
            title: None,
            memory_type: mem.memory_type,
            scope: mem.scope,
            topic_key: None,
            session_id: mem.session_id,
        };

        let conn = match db.lock() {
            Ok(c) => c,
            Err(_) => {
                errors.push(format!("memory[{}]: database lock error", idx));
                continue;
            }
        };

        match queries::upsert_memory(&conn, &auth.org_id, &user_id, &req) {
            Ok(_) => imported += 1,
            Err(e) => errors.push(format!("memory[{}]: {}", idx, e)),
        }
    }

    Ok((
        StatusCode::OK,
        Json(ImportMemoriesResponse { imported, skipped, errors }),
    ))
}

/// `POST /v1/admin/memories/bulk-tag` — admin-only.
/// Body: `{ ids: [..], action: "add" | "remove", tag: "..." }`.
/// Adds or removes the given tag from all specified memories (org-scoped).
/// Returns `{ updated: <count> }`.
pub async fn bulk_tag_memories(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<BulkTagRequest>,
) -> Result<Json<BulkTagResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let tag = body.tag.trim().to_string();
    if tag.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "tag must not be empty".to_string(),
                code: "invalid_tag".to_string(),
            }),
        ));
    }

    if body.action != "add" && body.action != "remove" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "action must be 'add' or 'remove'".to_string(),
                code: "invalid_action".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let updated = queries::bulk_tag_memories(&conn, &auth.org_id, &body.ids, &body.action, &tag)
        .map_err(db_err)?;

    Ok(Json(BulkTagResponse { updated }))
}

/// `GET /v1/projects/:id/stats` — returns memory statistics for a project. Admin-only.
pub async fn get_project_stats_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project_id): Path<String>,
) -> Result<Json<ProjectStats>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    match queries::get_project_stats(&conn, &auth.org_id, &project_id) {
        Ok(stats) => Ok(Json(stats)),
        Err(e) if e.to_string().contains("project_not_found") => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Project not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Err(e) => Err(db_err(e)),
    }
}

/// `POST /v1/admin/tags/rename` — admin-only.
/// Renames a tag across all memories in the org.
/// Body: `{ from: String, to: String }`.
/// Returns `{ updated_count: <i64> }`.
pub async fn rename_tag(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<RenameTagRequest>,
) -> Result<Json<RenameTagResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let from = body.from.trim().to_string();
    let to = body.to.trim().to_string();

    if from.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "'from' must not be empty".to_string(),
                code: "invalid_from".to_string(),
            }),
        ));
    }
    if to.is_empty() {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "'to' must not be empty".to_string(),
                code: "invalid_to".to_string(),
            }),
        ));
    }
    if from == to {
        return Ok(Json(RenameTagResponse { updated_count: 0 }));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let updated_count = queries::rename_tag(&conn, &auth.org_id, &from, &to).map_err(db_err)?;

    Ok(Json(RenameTagResponse { updated_count }))
}

/// `GET /v1/admin/export` — admin-only. Exports org config as a downloadable JSON file.
/// Includes: org name, settings, webhook list (url + events only, no secrets), project names.
pub async fn export_org_config(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<impl axum::response::IntoResponse, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let org = queries::get_org(&conn, &auth.org_id)
        .map_err(db_err)?
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ApiError { error: "Org not found".to_string(), code: "not_found".to_string() }),
        ))?;

    let settings = queries::get_org_settings(&conn, &auth.org_id).map_err(db_err)?;
    let webhooks = queries::list_webhooks(&conn, &auth.org_id).map_err(db_err)?;
    let projects = queries::list_projects(&conn, &auth.org_id).map_err(db_err)?;

    let webhook_export: Vec<serde_json::Value> = webhooks.iter().map(|w| {
        serde_json::json!({
            "url": w.target_url,
            "events": w.events,
        })
    }).collect();

    let project_names: Vec<String> = projects.iter().map(|p| p.name.clone()).collect();

    let payload = serde_json::json!({
        "org_name": org.name,
        "custom_instructions": settings.custom_instructions,
        "retention_days": settings.retention_days,
        "min_password_length": settings.min_password_length,
        "agent_events": settings.events,
        "webhooks": webhook_export,
        "projects": project_names,
    });

    let body = serde_json::to_string_pretty(&payload)
        .map_err(|e| db_err(e.into()))?;

    use axum::http::header;
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"nexusmind-config.json\""),
        ],
        body,
    ))
}

/// `POST /v1/admin/import` — admin-only. Imports org settings from a JSON body
/// matching the export format. Applies: custom_instructions, retention_days,
/// min_password_length, agent_events. Skips webhooks, projects, and users.
/// Returns `{ applied_fields, skipped_fields }`.
pub async fn import_org_config(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<serde_json::Value>,
) -> Result<Json<ImportConfigResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let mut applied_fields: Vec<String> = Vec::new();
    let mut skipped_fields: Vec<String> = Vec::new();

    // Fetch current settings to merge into
    let mut settings = queries::get_org_settings(&conn, &auth.org_id).map_err(db_err)?;

    // Support both flat format (from export) and nested { org: { ... } } format
    let source = if let Some(org_obj) = body.get("org").and_then(|v| v.as_object()) {
        serde_json::Value::Object(org_obj.clone())
    } else {
        body.clone()
    };

    // custom_instructions
    if let Some(val) = source.get("custom_instructions") {
        if val.is_string() || val.is_null() {
            settings.custom_instructions = val.as_str().map(|s| s.to_string());
            applied_fields.push("custom_instructions".to_string());
        }
    }

    // retention_days
    if let Some(val) = source.get("retention_days") {
        if val.is_number() || val.is_null() {
            settings.retention_days = val.as_i64();
            applied_fields.push("retention_days".to_string());
        }
    }

    // min_password_length
    if let Some(val) = source.get("min_password_length") {
        if val.is_number() || val.is_null() {
            settings.min_password_length = val.as_i64();
            applied_fields.push("min_password_length".to_string());
        }
    }

    // agent_events (exported as "agent_events", may also appear as "events")
    let events_val = source.get("agent_events").or_else(|| source.get("event_captures"));
    if let Some(ev) = events_val {
        if let Ok(parsed) = serde_json::from_value::<crate::models::types::AgentEventSettings>(ev.clone()) {
            settings.events = parsed;
            applied_fields.push("agent_events".to_string());
        }
    }

    // Explicitly note skipped top-level keys
    for skip_key in &["webhooks", "projects", "org_name"] {
        if body.get(skip_key).is_some() {
            skipped_fields.push(skip_key.to_string());
        }
    }

    if applied_fields.is_empty() {
        return Ok(Json(ImportConfigResponse { applied_fields, skipped_fields }));
    }

    queries::update_org_settings(&conn, &auth.org_id, &settings).map_err(db_err)?;

    Ok(Json(ImportConfigResponse { applied_fields, skipped_fields }))
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
            .route("/v1/admin/stats/tags", get(get_tag_stats_handler))
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

    #[tokio::test]
    async fn tag_stats_returns_empty_when_no_tags() {
        let (store, key) = setup_with_admin_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/stats/tags")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let tags: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(tags, serde_json::json!([]));
    }

    #[tokio::test]
    async fn tag_stats_returns_counts_for_overlapping_tags() {
        use crate::models::types::StoreMemoryRequest;

        let (store, key) = setup_with_admin_key();

        // Insert two memories with overlapping tags
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let user_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", [&org_id], |r| r.get(0))
                .unwrap();

            q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                project: Some("p1".to_string()),
                tool: "test".to_string(),
                content: "memory one".to_string(),
                tags: Some(vec!["rust".to_string(), "backend".to_string()]),
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();

            q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                project: Some("p1".to_string()),
                tool: "test".to_string(),
                content: "memory two".to_string(),
                tags: Some(vec!["rust".to_string(), "frontend".to_string()]),
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/stats/tags")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let tags: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        // "rust" should appear first with count 2
        assert!(!tags.is_empty(), "expected at least one tag");
        let rust_tag = tags.iter().find(|t| t["name"] == "rust");
        assert!(rust_tag.is_some(), "expected 'rust' tag");
        assert_eq!(rust_tag.unwrap()["count"], 2);

        // "backend" and "frontend" each appear once
        let backend = tags.iter().find(|t| t["name"] == "backend");
        assert!(backend.is_some());
        assert_eq!(backend.unwrap()["count"], 1);
    }

    // ── reset_user_key tests ─────────────────────────────────────────────────

    fn app_with_reset(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/users/:user_id/reset-key", post(reset_user_key))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn reset_user_key_returns_new_key_different_from_original() {
        let (store, admin_key) = setup_with_admin_key();

        // Invite a member user and capture their original key.
        let (member_id, original_key) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let (user, key) =
                q::invite_user(&conn, &org, "reset@acme.com", "Reset User", "member").unwrap();
            (user.id, key)
        };

        let resp = app_with_reset(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/admin/users/{member_id}/reset-key"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let new_key = json["new_key"].as_str().expect("new_key must be a string");
        assert!(new_key.starts_with("nm_"), "key must have nm_ prefix");
        assert_ne!(new_key, original_key, "new key must differ from the original");
    }

    #[tokio::test]
    async fn reset_user_key_returns_400_for_demo_key() {
        use crate::auth::api_keys;
        let (store, admin_key) = setup_with_admin_key();

        // Insert a user with a demo-labelled key directly.
        let demo_user_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            // Create the user.
            let (user, _) =
                q::invite_user(&conn, &org, "demo@acme.com", "Demo User", "member").unwrap();
            // Replace their key with a demo-labelled one.
            conn.execute(
                "UPDATE api_keys SET revoked = 1 WHERE user_id = ?1",
                rusqlite::params![user.id],
            ).unwrap();
            let hash = api_keys::hash_key("nm_demo_acme_demo");
            conn.execute(
                "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
                 VALUES ('demo-key-id', ?1, ?2, ?3, 'demo', datetime('now'))",
                rusqlite::params![user.id, org, hash],
            ).unwrap();
            user.id
        };

        let resp = app_with_reset(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/admin/users/{demo_user_id}/reset-key"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "demo_key");
    }

    // ── import_memories tests ─────────────────────────────────────────────────

    fn app_with_import(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/memories/import", post(import_memories))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn import_3_valid_memories_returns_imported_3() {
        let (store, key) = setup_with_admin_key();

        let body = serde_json::json!({
            "memories": [
                { "content": "First memory content" },
                { "content": "Second memory content", "project": "myproject" },
                { "content": "Third memory content", "type": "decision" }
            ]
        });

        let resp = app_with_import(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/memories/import")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["imported"], 3);
        assert_eq!(json["skipped"], 0);
        assert_eq!(json["errors"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn import_with_empty_content_skips_and_counts() {
        let (store, key) = setup_with_admin_key();

        let body = serde_json::json!({
            "memories": [
                { "content": "Valid memory one" },
                { "content": "" },
                { "content": "Valid memory two" }
            ]
        });

        let resp = app_with_import(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/memories/import")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["imported"], 2);
        assert_eq!(json["skipped"], 1);
    }

    #[tokio::test]
    async fn import_raw_array_accepted() {
        let (store, key) = setup_with_admin_key();

        let body = serde_json::json!([
            { "content": "Raw array memory one" },
            { "content": "Raw array memory two", "project": "myproject" }
        ]);

        let resp = app_with_import(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/memories/import")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["imported"], 2);
        assert_eq!(json["skipped"], 0);
        assert_eq!(json["errors"], serde_json::json!([]));
    }

    // ── agent_activity tests ──────────────────────────────────────────────────

    fn app_with_agent_activity(store: SqliteStore) -> Router {
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/stats/agent-activity", get(get_agent_activity))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    // ── merge_memories tests ─────────────────────────────────────────────────

    fn app_with_merge(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/memories/merge", post(merge_memories))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn merge_combines_content_and_deletes_merge_id() {
        use crate::models::types::StoreMemoryRequest;

        let (store, key) = setup_with_admin_key();

        let (keep_id, merge_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let user_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", [&org_id], |r| r.get(0))
                .unwrap();

            let keep = q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                project: Some("p1".to_string()),
                tool: "test".to_string(),
                content: "Content A".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();

            let merge = q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                project: Some("p1".to_string()),
                tool: "test".to_string(),
                content: "Content B".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();

            (keep.id, merge.id)
        };

        let body = serde_json::json!({ "keep_id": keep_id, "merge_id": merge_id });

        let resp = app_with_merge(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/memories/merge")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        let content = json["content"].as_str().unwrap();
        assert!(content.contains("Content A"), "merged content must contain keep content");
        assert!(content.contains("Content B"), "merged content must contain merge content");
        assert!(content.contains("---"), "merged content must include separator");

        // The merge_id memory must be gone
        let db = store.conn();
        let conn = db.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM memories WHERE id = ?1", [&merge_id], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0, "merge_id memory must be deleted");
    }

    #[tokio::test]
    async fn merge_with_wrong_org_returns_404() {
        use crate::models::types::StoreMemoryRequest;

        let (store, key) = setup_with_admin_key();

        // Create a second org with its own memories
        let foreign_memory_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let superuser_key = Some("test-superuser-key".to_string());
            let _ = superuser_key;
            let (other_org, other_user, _) =
                q::create_org(&conn, "Other Corp", "other-corp", "admin@other.com", "Admin Other").unwrap();
            let mem = q::upsert_memory(&conn, &other_org.id, &other_user.id, &StoreMemoryRequest {
                project: Some("p2".to_string()),
                tool: "test".to_string(),
                content: "Foreign content".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();
            mem.id
        };

        // Try to merge a foreign memory as the keep_id — should 404
        let body = serde_json::json!({ "keep_id": foreign_memory_id, "merge_id": "nonexistent" });

        let resp = app_with_merge(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/memories/merge")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn agent_activity_returns_correct_counts_per_tool() {
        use crate::models::types::StoreMemoryRequest;

        let (store, key) = setup_with_admin_key();

        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let user_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", [&org_id], |r| r.get(0))
                .unwrap();

            // Insert 3 memories with tool "claude-code" and 1 with "cursor"
            for i in 0..3 {
                q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                    project: Some("p1".to_string()),
                    tool: "claude-code".to_string(),
                    content: format!("claude memory {i}"),
                    tags: None,
                    title: None,
                    memory_type: None,
                    scope: None,
                    topic_key: None,
                    session_id: None,
                }).unwrap();
            }
            q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                project: Some("p1".to_string()),
                tool: "cursor".to_string(),
                content: "cursor memory".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();
        }

        let resp = app_with_agent_activity(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/stats/agent-activity")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let items: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        // Should have 2 tools
        assert_eq!(items.len(), 2, "expected 2 tools");

        // claude-code should be first (highest 7d count = 3)
        assert_eq!(items[0]["tool"], "claude-code");
        assert_eq!(items[0]["total_memories"], 3);
        assert_eq!(items[0]["memories_last_7d"], 3);

        // cursor second
        assert_eq!(items[1]["tool"], "cursor");
        assert_eq!(items[1]["total_memories"], 1);
    }

    // ── notifications tests ──────────────────────────────────────────────────

    fn app_with_notifications(store: SqliteStore) -> Router {
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/notifications", get(get_notifications))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn notifications_returns_relevant_events_only() {
        let (store, key) = setup_with_admin_key();

        // Insert audit events: some that should appear in notifications, some that should not.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let user_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", [&org_id], |r| r.get(0))
                .unwrap();

            // Relevant events (should appear in response).
            q::log_audit(&conn, &org_id, &user_id, "user.created", "user", None, serde_json::json!({})).unwrap();
            q::log_audit(&conn, &org_id, &user_id, "key.revoked", "api_key", None, serde_json::json!({})).unwrap();
            q::log_audit(&conn, &org_id, &user_id, "invite.redeemed", "invite", None, serde_json::json!({})).unwrap();

            // Non-relevant event (should NOT appear).
            q::log_audit(&conn, &org_id, &user_id, "memory.search", "memory", None, serde_json::json!({})).unwrap();
            q::log_audit(&conn, &org_id, &user_id, "memory.store", "memory", None, serde_json::json!({})).unwrap();
        }

        let resp = app_with_notifications(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/admin/notifications?limit=15")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let items: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        // Should have exactly 3 relevant events (not the 2 non-relevant ones).
        assert_eq!(items.len(), 3, "expected 3 notification items, got {}", items.len());

        // All returned items must have a non-empty message.
        for item in &items {
            assert!(item["message"].is_string(), "message must be a string");
            assert!(!item["message"].as_str().unwrap().is_empty(), "message must not be empty");
            assert!(item["created_at"].is_string(), "created_at must be a string");
        }

        // Verify expected actions are present.
        let actions: Vec<&str> = items.iter()
            .filter_map(|i| i["action"].as_str())
            .collect();
        assert!(actions.contains(&"user.created"), "user.created must be in notifications");
        assert!(actions.contains(&"key.revoked"), "key.revoked must be in notifications");
        assert!(actions.contains(&"invite.redeemed"), "invite.redeemed must be in notifications");
        assert!(!actions.contains(&"memory.search"), "memory.search must NOT be in notifications");
        assert!(!actions.contains(&"memory.store"), "memory.store must NOT be in notifications");
    }

    // ── rename_tag tests ─────────────────────────────────────────────────────

    fn app_with_rename_tag(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/tags/rename", post(rename_tag))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    // ── import_org_config tests ──────────────────────────────────────────────

    fn app_with_import_config(store: SqliteStore) -> Router {
        use axum::routing::post;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/import", post(import_org_config))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn import_org_config_applies_retention_days() {
        let (store, key) = setup_with_admin_key();

        let body = serde_json::json!({ "org": { "retention_days": 60 } });

        let resp = app_with_import_config(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/import")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert!(json["applied_fields"].as_array().unwrap().contains(&serde_json::json!("retention_days")),
            "retention_days must be in applied_fields");

        // Verify DB was updated
        let db = store.conn();
        let conn = db.lock().unwrap();
        let org_id: String = conn
            .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
            .unwrap();
        let retention: Option<i64> = conn
            .query_row("SELECT retention_days FROM organizations WHERE id = ?1", [&org_id], |r| r.get(0))
            .unwrap();
        assert_eq!(retention, Some(60), "org retention_days must be 60");
    }

    #[tokio::test]
    async fn rename_tag_updates_all_memories_with_that_tag() {
        use crate::models::types::StoreMemoryRequest;

        let (store, key) = setup_with_admin_key();

        // Insert 3 memories all tagged "foo", plus 1 extra "bar" on two of them.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();
            let user_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", [&org_id], |r| r.get(0))
                .unwrap();

            for i in 0..3 {
                q::upsert_memory(&conn, &org_id, &user_id, &StoreMemoryRequest {
                    project: Some("p1".to_string()),
                    tool: "test".to_string(),
                    content: format!("memory {i}"),
                    tags: Some(vec!["foo".to_string()]),
                    title: None,
                    memory_type: None,
                    scope: None,
                    topic_key: None,
                    session_id: None,
                }).unwrap();
            }
        }

        // Rename "foo" → "bar"
        let body = serde_json::json!({ "from": "foo", "to": "bar" });
        let resp = app_with_rename_tag(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/admin/tags/rename")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["updated_count"], 3, "must have updated all 3 memories");

        // Verify in the DB: "bar" appears on all 3, "foo" is gone
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let org_id: String = conn
                .query_row("SELECT id FROM organizations LIMIT 1", [], |r| r.get(0))
                .unwrap();

            let bar_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories
                 WHERE org_id = ?1
                   AND EXISTS (SELECT 1 FROM json_each(tags) WHERE value = 'bar')",
                rusqlite::params![org_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(bar_count, 3, "'bar' must appear in all 3 memories");

            let foo_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM memories
                 WHERE org_id = ?1
                   AND EXISTS (SELECT 1 FROM json_each(tags) WHERE value = 'foo')",
                rusqlite::params![org_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(foo_count, 0, "'foo' must be gone from all memories");
        }
    }

    // ── update_org_settings_api PATCH partial-update tests ───────────────────

    fn app_with_org_settings(store: SqliteStore) -> Router {
        use axum::routing::get;
        let superuser_key: Option<String> = Some("test-superuser-key".to_string());
        let email_config: Option<Arc<EmailConfig>> = None;

        let protected = Router::new()
            .route("/v1/admin/org/settings", get(get_org_settings_api).patch(update_org_settings_api))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth));

        Router::new()
            .merge(protected)
            .layer(Extension(email_config))
            .layer(Extension(superuser_key))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn patch_org_settings_partial_preserves_other_fields() {
        let (store, key) = setup_with_admin_key();

        // Send only retention_days — min_password_length must NOT become null.
        let body = serde_json::json!({ "retention_days": 365 });
        let resp = app_with_org_settings(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri("/v1/admin/org/settings")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "partial PATCH must not 500");
        let raw = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(json["retention_days"], 365, "retention_days must be updated");
        // min_password_length was not in the request — DB default is 8, must still be 8.
        assert_eq!(json["min_password_length"], 8, "min_password_length must be preserved");
    }
}

// ── Collections ───────────────────────────────────────────────────────────────

/// GET /v1/admin/collections — list all collections for the org.
pub async fn list_collections_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<Collection>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let conn = store.conn();
    let conn = conn.lock().map_err(|_| lock_err())?;
    let collections = queries::list_collections(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(collections))
}

/// POST /v1/admin/collections — create a new collection.
pub async fn create_collection_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<Collection>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let conn = store.conn();
    let conn = conn.lock().map_err(|_| lock_err())?;
    let collection = queries::create_collection(&conn, &auth.org_id, &req.name, req.description.as_deref())
        .map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(collection)))
}

/// DELETE /v1/admin/collections/:id — delete a collection.
pub async fn delete_collection_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let conn = store.conn();
    let conn = conn.lock().map_err(|_| lock_err())?;
    let deleted = queries::delete_collection(&conn, &auth.org_id, &id).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Collection not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// POST /v1/memories/:id/collection — assign or unassign a memory to a collection.
pub async fn assign_memory_collection_api(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(memory_id): Path<String>,
    AppJson(req): AppJson<AssignCollectionRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let conn = store.conn();
    let conn = conn.lock().map_err(|_| lock_err())?;
    let updated = queries::assign_memory_collection(
        &conn,
        &auth.org_id,
        &memory_id,
        req.collection_id.as_deref(),
    )
    .map_err(db_err)?;
    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

/// `GET /v1/admin/settings/retention-preview`
///
/// Returns how many memories would be deleted given the current retention policy.
/// If no retention policy is set (retention_days = NULL), returns would_delete = 0.
/// `GET /v1/admin/memories/health` — admin-only.
/// Returns a health summary for the org's memory corpus: total, duplicates, stale, and untagged.
pub async fn get_memory_health_handler(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<crate::models::types::MemoryHealth>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let health = queries::get_memory_health(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(health))
}

pub async fn get_retention_preview(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<RetentionPreview>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let conn = store.conn();
    let conn = conn.lock().map_err(|_| lock_err())?;

    // Fetch retention_days from org settings
    let retention_days: Option<i64> = conn
        .query_row(
            "SELECT retention_days FROM organizations WHERE id = ?1",
            rusqlite::params![auth.org_id],
            |r| r.get(0),
        )
        .map_err(|e| db_err(anyhow::anyhow!(e)))?;

    let Some(days) = retention_days else {
        return Ok(Json(RetentionPreview {
            would_delete: 0,
            retention_days: None,
        }));
    };

    let would_delete: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memories
             WHERE org_id = ?1
               AND archived_at IS NULL
               AND datetime(created_at) < datetime('now', '-' || ?2 || ' days')",
            rusqlite::params![auth.org_id, days],
            |r| r.get(0),
        )
        .map_err(|e| db_err(anyhow::anyhow!(e)))?;

    Ok(Json(RetentionPreview {
        would_delete,
        retention_days: Some(days),
    }))
}

// ── User disable / enable ─────────────────────────────────────────────────────

/// POST /v1/admin/users/:id/disable
/// Sets disabled_at = datetime('now') for the target user.
/// Admins cannot disable themselves.
pub async fn disable_user(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    if auth.user_id == user_id {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Cannot disable your own account".to_string(),
                code: "self_disable".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let changed = queries::disable_user(&conn, &auth.org_id, &user_id).map_err(db_err)?;

    if !changed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "User not found or already disabled".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "user.disable",
        "user",
        Some(&user_id),
        serde_json::json!({}),
    );

    Ok(StatusCode::NO_CONTENT)
}

/// POST /v1/admin/users/:id/enable
/// Clears disabled_at for the target user, restoring access.
pub async fn enable_user(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let changed = queries::enable_user(&conn, &auth.org_id, &user_id).map_err(db_err)?;

    if !changed {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "User not found or not disabled".to_string(),
                code: "not_found".to_string(),
            }),
        ));
    }

    let _ = queries::log_audit(
        &conn,
        &auth.org_id,
        &auth.user_id,
        "user.enable",
        "user",
        Some(&user_id),
        serde_json::json!({}),
    );

    Ok(StatusCode::NO_CONTENT)
}

/// `PATCH /v1/admin/memories/:id/note` — set or clear the admin note on a memory.
/// Body: `{ "note": "..." }`. Empty string clears the note.
/// Admin-only. The note is never returned to agents or non-admin callers.
pub async fn update_memory_note(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(body): AppJson<UpdateNoteRequest>,
) -> Result<Json<Memory>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    match queries::update_memory_admin_note(&conn, &auth.org_id, &id, &body.note)
        .map_err(db_err)?
    {
        Some(memory) => Ok(Json(memory)),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
    }
}

/// `PATCH /v1/admin/org/announcement` — set or clear the org announcement banner (admin-only).
/// Body: `{ announcement: String, announcement_type?: "info"|"warning"|"error" }`.
/// Empty `announcement` string clears the banner.
pub async fn update_org_announcement(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<UpdateAnnouncementRequest>,
) -> Result<Json<OrgSettings>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let ann_type = body.announcement_type.as_deref().unwrap_or("info");
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let settings = queries::update_announcement(&conn, &auth.org_id, &body.announcement, ann_type)
        .map_err(db_err)?;
    Ok(Json(settings))
}

/// `PATCH /v1/admin/org/logo` — set or clear the org logo URL (admin-only).
/// Body: `{ logo_url: Option<String> }`.
/// None `logo_url` clears the logo.
pub async fn update_org_logo(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(body): AppJson<UpdateOrgLogoRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    queries::update_org_logo(&conn, &auth.org_id, body.logo_url.as_deref())
        .map_err(db_err)?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

/// `PATCH /v1/admin/memories/:id/schedule-delete` — set or clear per-memory scheduled deletion (admin-only).
/// Body: `{ delete_at: Option<String> }` — ISO datetime string or null to clear.
pub async fn schedule_memory_delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(body): AppJson<ScheduleDeleteRequest>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    queries::schedule_memory_delete(&conn, &auth.org_id, &id, body.delete_at.as_deref())
        .map_err(|e| {
            if e.to_string() == "memory_not_found" {
                (
                    StatusCode::NOT_FOUND,
                    Json(ApiError { error: "Memory not found".to_string(), code: "not_found".to_string() }),
                )
            } else {
                db_err(e)
            }
        })?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /v1/admin/users` — list all users in the org, including admin_note (admin-only).
pub async fn list_users_admin(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Result<Json<Vec<User>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;
    let users = queries::list_users(&conn, &auth.org_id).map_err(db_err)?;
    Ok(Json(users))
}

/// `PATCH /v1/admin/users/:id/note` — set or clear a private admin note on a user (admin-only).
/// Body: `{ note: Option<String> }` — null or absent clears the note.
pub async fn update_user_note(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(user_id): Path<String>,
    AppJson(body): AppJson<UpdateUserNoteRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| lock_err())?;

    let note_str = body.note.as_deref().filter(|s| !s.is_empty());
    let found = queries::update_user_admin_note(&conn, &auth.org_id, &user_id, note_str)
        .map_err(db_err)?;

    if !found {
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
        "user.note_updated",
        "user",
        Some(&user_id),
        serde_json::json!({}),
    );

    Ok(Json(serde_json::json!({ "ok": true })))
}
