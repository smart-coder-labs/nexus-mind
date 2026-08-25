use axum::async_trait;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::Connection;
use serde::de::DeserializeOwned;

use crate::models::types::{ApiError, AuthContext};

/// Default page size used only when the caller opts into pagination by
/// supplying `offset` without `limit`. See [`resolve_list_pagination`].
pub const DEFAULT_LIST_LIMIT: i64 = 100;
/// Hard ceiling on `limit` — requests above this are clamped, never rejected.
pub const MAX_LIST_LIMIT: i64 = 500;

/// Returns the non-enumerating response used after an org-local resource exists
/// but is outside the caller's viewer scope. Audit failures never alter 404.
pub fn hidden_resource_not_found(
    conn: &Connection,
    auth: &AuthContext,
    resource_type: &str,
    resource_id: &str,
    method: &str,
    endpoint_family: &str,
) -> (StatusCode, Json<ApiError>) {
    let _ = crate::db::queries::log_audit(
        conn,
        &auth.org_id,
        &auth.user_id,
        "resource.hidden_access_denied",
        resource_type,
        Some(resource_id),
        serde_json::json!({ "method": method, "endpoint_family": endpoint_family }),
    );
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Resource not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

/// Resolves `limit`/`offset` query params for list endpoints using an
/// opt-in pagination contract:
///
/// - Neither `limit` nor `offset` provided → pagination is NOT applied; the
///   full result set is returned, matching behavior from before pagination
///   support existed.
/// - Either is provided → `limit` is clamped to `[0, MAX_LIST_LIMIT]`
///   (defaulting to `DEFAULT_LIST_LIMIT` when only `offset` was given) and
///   `offset` is clamped to be non-negative.
pub fn resolve_list_pagination(limit: Option<i64>, offset: Option<i64>) -> (i64, i64) {
    if limit.is_none() && offset.is_none() {
        return (i64::MAX, 0);
    }
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(0, MAX_LIST_LIMIT);
    let offset = offset.unwrap_or(0).max(0);
    (limit, offset)
}

/// JSON extractor that maps all body errors (syntax, shape, missing) to 422.
///
/// Axum's built-in `Json` returns 400 for syntax/parse failures and 422 for shape
/// mismatches, making the error model inconsistent. This extractor normalises all
/// JSON body failures to 422 Unprocessable Entity.
pub struct JsonBody<T>(pub T);

#[axum::async_trait]
impl<S, T> FromRequest<S> for JsonBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(Json(value)) => Ok(JsonBody(value)),
            Err(rejection) => Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    error: rejection.to_string(),
                    code: "invalid_body".to_string(),
                }),
            )),
        }
    }
}

/// Custom JSON extractor that returns a structured JSON error instead of plain text
/// when the request body is missing, malformed, or has the wrong Content-Type.
pub struct AppJson<T>(pub T);

#[async_trait]
impl<T, S> FromRequest<S> for AppJson<T>
where
    T: DeserializeOwned + Send,
    S: Send + Sync,
{
    type Rejection = (StatusCode, Json<ApiError>);

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        match Json::<T>::from_request(req, state).await {
            Ok(value) => Ok(AppJson(value.0)),
            Err(rejection) => {
                let (status, code) = match &rejection {
                    JsonRejection::JsonDataError(_) => {
                        (StatusCode::UNPROCESSABLE_ENTITY, "invalid_json")
                    }
                    JsonRejection::JsonSyntaxError(_) => (StatusCode::BAD_REQUEST, "invalid_json"),
                    JsonRejection::MissingJsonContentType(_) => {
                        (StatusCode::UNSUPPORTED_MEDIA_TYPE, "invalid_content_type")
                    }
                    _ => (StatusCode::BAD_REQUEST, "invalid_json"),
                };
                Err((
                    status,
                    Json(ApiError {
                        error: rejection.to_string(),
                        code: code.to_string(),
                    }),
                ))
            }
        }
    }
}

/// Returns `Ok(())` if `auth.role` (or project-level override) has the required `permission`. Otherwise `Err(403)`.
pub fn require_permission(
    conn: &Connection,
    auth: &AuthContext,
    project: Option<&str>,
    permission: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    require_permission_inner(conn, auth, project, permission, true)
}

/// Checks an exact permission without the legacy privileged-role bypass.
/// New permission-driven domains should use this helper so authorization is
/// determined by grants rather than by role names.
pub fn require_explicit_permission(
    conn: &Connection,
    auth: &AuthContext,
    project: Option<&str>,
    permission: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    require_permission_inner(conn, auth, project, permission, false)
}

fn require_permission_inner(
    conn: &Connection,
    auth: &AuthContext,
    project: Option<&str>,
    permission: &str,
    privileged_bypass: bool,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if privileged_bypass && auth.role.is_privileged() {
        return Ok(());
    }

    let effective_role = if let Some(p_name) = project {
        match crate::db::queries::get_project_member_role(conn, &auth.org_id, p_name, &auth.user_id)
        {
            Ok(Some(role_str)) => {
                role_str
                    .parse::<crate::models::types::UserRole>()
                    .map_err(|_| {
                        (
                            StatusCode::FORBIDDEN,
                            Json(ApiError {
                                error: "Access denied to this project".to_string(),
                                code: "forbidden".to_string(),
                            }),
                        )
                    })?
            }
            Ok(None) => {
                // Only enforce membership if the project already exists.
                // If it doesn't exist yet it will be auto-created on write,
                // so fall back to the global role for this request.
                let project_exists =
                    crate::db::queries::project_name_exists(conn, &auth.org_id, p_name)
                        .unwrap_or(false);
                if project_exists {
                    return Err((
                        StatusCode::FORBIDDEN,
                        Json(ApiError {
                            error: "Access denied to this project".to_string(),
                            code: "forbidden".to_string(),
                        }),
                    ));
                }
                auth.role.clone()
            }
            Err(_) => auth.role.clone(),
        }
    } else {
        auth.role.clone()
    };

    let permissions =
        crate::db::queries::get_role_permissions(conn, &auth.org_id, effective_role.as_str())
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ApiError {
                        error: "Database error resolving permissions".to_string(),
                        code: "internal_error".to_string(),
                    }),
                )
            })?;

    // Autonomous-agent endpoints deliberately do not inherit the legacy
    // privileged-role bypass. Their built-in grants live in persisted role
    // templates so an operator can remove a grant and the exact permission
    // check will fail closed even when the actor's role is named `admin`.
    if !privileged_bypass
        && permission.starts_with("autonomous_agent:")
        && matches!(effective_role.as_str(), "admin" | "super_user")
    {
        let template_id = if effective_role.as_str() == "admin" {
            "admin_template"
        } else {
            "super_user_template"
        };
        let granted = conn
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM roles, json_each(roles.permissions)
                    WHERE roles.id=?1 AND roles.enabled=1 AND json_each.value=?2
                )",
                rusqlite::params![template_id, permission],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            != 0;
        return if granted {
            Ok(())
        } else {
            Err((
                StatusCode::FORBIDDEN,
                Json(ApiError {
                    error: "Insufficient permissions".to_string(),
                    code: "forbidden".to_string(),
                }),
            ))
        };
    }

    if permissions.iter().any(|p| p == permission) {
        Ok(())
    } else {
        Err((
            StatusCode::FORBIDDEN,
            Json(ApiError {
                error: "Insufficient permissions".to_string(),
                code: "forbidden".to_string(),
            }),
        ))
    }
}

/// A non-super-user may administer a user only when they share a project.
/// This keeps organization administration from becoming an org-wide visibility bypass.
pub fn user_is_visible_to_actor(
    conn: &Connection,
    auth: &AuthContext,
    user_id: &str,
) -> rusqlite::Result<bool> {
    if auth.role.is_super_user() || auth.user_id == user_id {
        return Ok(true);
    }

    conn.query_row(
        "SELECT EXISTS(
            SELECT 1
            FROM project_members actor_members
            JOIN project_members target_members
              ON target_members.project_id = actor_members.project_id
            JOIN projects p ON p.id = actor_members.project_id
            WHERE actor_members.user_id = ?1
              AND target_members.user_id = ?2
              AND p.org_id = ?3
        )",
        rusqlite::params![auth.user_id, user_id, auth.org_id],
        |row| row.get(0),
    )
}

/// Project-named resources without a canonical project row are organization-shared.
/// Registered projects require membership unless the caller is a super user.
pub fn project_is_visible_to_actor(
    conn: &Connection,
    auth: &AuthContext,
    project_name: &str,
) -> anyhow::Result<bool> {
    if auth.role.is_super_user() {
        return Ok(true);
    }
    let Some(project_id) =
        crate::db::queries::get_project_id_by_name(conn, &auth.org_id, project_name)?
    else {
        return Ok(true);
    };
    crate::db::queries::user_is_project_member(conn, &auth.org_id, &project_id, &auth.user_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};
    use crate::models::types::{Role, UserRole};

    fn setup_db() -> Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn
    }

    fn make_auth(role: Role) -> AuthContext {
        AuthContext {
            org_id: "org1".to_string(),
            user_id: "u1".to_string(),
            role: UserRole::Standard(role),
        }
    }

    #[test]
    fn visible_member_requires_a_shared_project_unless_super_user() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org', 'Org', 'org')",
            [],
        )
        .unwrap();
        for (id, role) in [
            ("admin", "admin"),
            ("shared", "member"),
            ("hidden", "member"),
        ] {
            conn.execute(
                "INSERT INTO users (id, org_id, email, name, role, status, created_at) VALUES (?1, 'org', ?2, ?1, ?3, 'active', datetime('now'))",
                rusqlite::params![id, format!("{id}@example.com"), role],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO projects (id, org_id, name, created_at) VALUES ('project-a', 'org', 'A', datetime('now'))",
            [],
        )
        .unwrap();
        for user_id in ["admin", "shared"] {
            conn.execute(
                "INSERT INTO project_members (id, project_id, user_id, role, created_at) VALUES (?1, 'project-a', ?2, 'member', datetime('now'))",
                rusqlite::params![format!("membership-{user_id}"), user_id],
            )
            .unwrap();
        }

        let admin = AuthContext {
            org_id: "org".into(),
            user_id: "admin".into(),
            role: UserRole::Standard(Role::Admin),
        };
        let super_user = AuthContext {
            org_id: "org".into(),
            user_id: "admin".into(),
            role: UserRole::Custom("super_user".into()),
        };

        assert!(user_is_visible_to_actor(&conn, &admin, "shared").unwrap());
        assert!(!user_is_visible_to_actor(&conn, &admin, "hidden").unwrap());
        assert!(user_is_visible_to_actor(&conn, &super_user, "hidden").unwrap());
    }

    fn make_custom_auth(role: &str) -> AuthContext {
        AuthContext {
            org_id: "org1".to_string(),
            user_id: "u1".to_string(),
            role: UserRole::Custom(role.to_string()),
        }
    }

    #[test]
    fn admin_has_all_permissions() {
        let conn = setup_db();
        let auth = make_auth(Role::Admin);
        assert!(require_permission(&conn, &auth, None, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, None, "user:invite").is_ok());
        assert!(require_permission(&conn, &auth, None, "nonexistent:permission").is_ok());
    }

    #[test]
    fn explicit_permission_does_not_bypass_admin_role() {
        let conn = setup_db();
        let auth = make_auth(Role::Admin);
        assert!(require_explicit_permission(&conn, &auth, None, "nonexistent:permission").is_err());
    }

    #[test]
    fn autonomous_agent_permissions_are_explicit_for_admin() {
        let conn = setup_db();
        let auth = make_auth(Role::Admin);
        for permission in [
            "autonomous_agent:read",
            "autonomous_agent:create",
            "autonomous_agent:update",
            "autonomous_agent:enable",
            "autonomous_agent:run",
            "autonomous_agent:cancel",
            "autonomous_agent:manage_connectors",
        ] {
            assert!(require_explicit_permission(&conn, &auth, None, permission).is_ok());
        }
    }

    #[test]
    fn admin_is_denied_when_the_exact_persisted_grant_is_removed() {
        let conn = setup_db();
        let raw: String = conn
            .query_row(
                "SELECT permissions FROM roles WHERE id='admin_template'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut permissions: Vec<String> = serde_json::from_str(&raw).unwrap();
        permissions.retain(|permission| permission != "autonomous_agent:read");
        conn.execute(
            "UPDATE roles SET permissions=?1 WHERE id='admin_template'",
            [serde_json::to_string(&permissions).unwrap()],
        )
        .unwrap();

        let auth = make_auth(Role::Admin);
        assert!(require_explicit_permission(&conn, &auth, None, "autonomous_agent:read").is_err());
        assert!(require_explicit_permission(&conn, &auth, None, "autonomous_agent:create").is_ok());
    }

    #[test]
    fn member_permissions() {
        let conn = setup_db();
        let auth = make_auth(Role::Member);
        assert!(require_permission(&conn, &auth, None, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:write").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:delete").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:search").is_ok());
        assert!(require_permission(&conn, &auth, None, "user:invite").is_err());
        assert!(require_permission(&conn, &auth, None, "audit:read").is_err());
    }

    #[test]
    fn viewer_permissions() {
        let conn = setup_db();
        let auth = make_auth(Role::Viewer);
        assert!(require_permission(&conn, &auth, None, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:search").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:write").is_err());
        assert!(require_permission(&conn, &auth, None, "user:invite").is_err());
    }

    #[test]
    fn custom_role_permissions() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        // Insert custom role "custom-operator" with memory:read and user:invite
        crate::db::queries::create_role(
            &conn,
            "org1",
            "custom-operator",
            "Custom Operator",
            &["memory:read".to_string(), "user:invite".to_string()],
            None,
        )
        .unwrap();

        let auth = make_custom_auth("custom-operator");
        assert!(require_permission(&conn, &auth, None, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, None, "user:invite").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:write").is_err());
    }

    #[test]
    fn custom_role_can_operate_autonomous_agents_only_with_exact_grant() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO organizations (id,name,slug) VALUES ('org1','Acme','acme')",
            [],
        )
        .unwrap();
        crate::db::queries::create_role(
            &conn,
            "org1",
            "agent-operator",
            "Agent operator",
            &[
                "autonomous_agent:read".to_string(),
                "autonomous_agent:run".to_string(),
            ],
            None,
        )
        .unwrap();
        let auth = make_custom_auth("agent-operator");
        assert!(require_explicit_permission(&conn, &auth, None, "autonomous_agent:read").is_ok());
        assert!(require_explicit_permission(&conn, &auth, None, "autonomous_agent:run").is_ok());
        assert!(
            require_explicit_permission(&conn, &auth, None, "autonomous_agent:enable").is_err()
        );
    }

    #[test]
    fn project_role_override_permissions() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role) VALUES ('u1', 'org1', 'dev@acme.com', 'Dev', 'viewer')",
            [],
        ).unwrap();

        // Add a project
        let p_id = crate::db::queries::get_or_create_project(&conn, "org1", "payments").unwrap();

        // Check that Dev (viewer globally) fails memory:write in project "payments"
        let auth = make_auth(Role::Viewer);
        assert!(require_permission(&conn, &auth, Some("payments"), "memory:write").is_err());

        // Now override Dev's role to dev-senior in "payments" project
        // Note: dev-senior template has memory:write permission
        crate::db::queries::upsert_project_member(&conn, &p_id, "u1", "dev-senior").unwrap();

        // Check that Dev now succeeds memory:write in project "payments"
        assert!(require_permission(&conn, &auth, Some("payments"), "memory:write").is_ok());

        // But still fails memory:write in another project "other-project"
        assert!(require_permission(&conn, &auth, Some("other-project"), "memory:write").is_err());
    }
}
