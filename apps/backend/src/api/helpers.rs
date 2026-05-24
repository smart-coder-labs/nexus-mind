use axum::http::StatusCode;
use axum::Json;
use rusqlite::Connection;

use crate::models::types::{ApiError, AuthContext};

/// Returns `Ok(())` if `auth.role` has the required `permission`. Otherwise `Err(403)`.
pub fn require_permission(
    conn: &Connection,
    auth: &AuthContext,
    permission: &str,
) -> Result<(), (StatusCode, Json<ApiError>)> {
    if auth.role.is_admin() {
        return Ok(());
    }

    let permissions = crate::db::queries::get_role_permissions(conn, &auth.org_id, auth.role.as_str())
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
        assert!(require_permission(&conn, &auth, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, "user:invite").is_ok());
        assert!(require_permission(&conn, &auth, "nonexistent:permission").is_ok());
    }

    #[test]
    fn member_permissions() {
        let conn = setup_db();
        let auth = make_auth(Role::Member);
        assert!(require_permission(&conn, &auth, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, "memory:write").is_ok());
        assert!(require_permission(&conn, &auth, "memory:delete").is_ok());
        assert!(require_permission(&conn, &auth, "memory:search").is_ok());
        assert!(require_permission(&conn, &auth, "user:invite").is_err());
        assert!(require_permission(&conn, &auth, "audit:read").is_err());
    }

    #[test]
    fn viewer_permissions() {
        let conn = setup_db();
        let auth = make_auth(Role::Viewer);
        assert!(require_permission(&conn, &auth, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, "memory:search").is_ok());
        assert!(require_permission(&conn, &auth, "memory:write").is_err());
        assert!(require_permission(&conn, &auth, "user:invite").is_err());
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
        assert!(require_permission(&conn, &auth, "memory:read").is_ok());
        assert!(require_permission(&conn, &auth, "user:invite").is_ok());
        assert!(require_permission(&conn, &auth, "memory:write").is_err());
    }
}
