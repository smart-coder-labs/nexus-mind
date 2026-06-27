use axum::async_trait;
use axum::extract::rejection::JsonRejection;
use axum::extract::{FromRequest, Request};
use axum::http::StatusCode;
use axum::Json;
use rusqlite::Connection;
use serde::de::DeserializeOwned;

use crate::models::types::{ApiError, AuthContext};

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
    if auth.role.is_admin() {
        return Ok(());
    }

    let effective_role = if let Some(p_name) = project {
        match crate::db::queries::get_project_member_role(conn, &auth.org_id, p_name, &auth.user_id) {
            Ok(Some(role_str)) => {
                role_str.parse::<crate::models::types::UserRole>()
                    .map_err(|_| (
                        StatusCode::FORBIDDEN,
                        Json(ApiError {
                            error: "Access denied to this project".to_string(),
                            code: "forbidden".to_string(),
                        }),
                    ))?
            }
            Ok(None) => {
                // Only enforce membership if the project already exists.
                // If it doesn't exist yet it will be auto-created on write,
                // so fall back to the global role for this request.
                let project_exists = crate::db::queries::project_name_exists(conn, &auth.org_id, p_name)
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

    let permissions = crate::db::queries::get_role_permissions(conn, &auth.org_id, effective_role.as_str())
        .map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database error resolving permissions".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;

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
        ).unwrap();
        // Insert custom role "custom-operator" with memory:read and user:invite
        crate::db::queries::create_role(
            &conn,
            "org1",
            "custom-operator",
            "Custom Operator",
            &["memory:read".to_string(), "user:invite".to_string()],
            None
        ).unwrap();

        let auth = make_custom_auth("custom-operator");
        assert!(require_permission(&conn, &auth, None, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, None, "user:invite").is_ok());
        assert!(require_permission(&conn, &auth, None, "memory:write").is_err());
    }

    #[test]
    fn project_role_override_permissions() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES ('org1', 'Acme', 'acme')",
            [],
        ).unwrap();
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
