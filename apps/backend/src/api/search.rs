use axum::{extract::{Query, State}, http::StatusCode, Extension, Json};
use serde::Deserialize;

use crate::{
    db::queries,
    models::types::{ApiError, AuthContext, GlobalSearchResult},
    store::sqlite::SqliteStore,
    api::helpers::require_permission,
};

#[derive(Deserialize)]
pub struct GlobalSearchParams {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
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

pub async fn get_global_search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GlobalSearchParams>,
) -> Result<Json<GlobalSearchResult>, (StatusCode, Json<ApiError>)> {
    let conn_arc = store.conn();
    let conn = conn_arc.lock().map_err(|_| lock_err())?;

    require_permission(&conn, &auth, None, "memory:search")
        .map_err(|e| e)?;

    let q = params.q.trim();
    if q.is_empty() {
        return Ok(Json(GlobalSearchResult {
            memories: vec![],
            users: vec![],
            projects: vec![],
        }));
    }

    let limit = params.limit.min(50).max(1);

    let memories = queries::search_memories(&conn, &auth.org_id, q, limit)
        .map_err(|e| db_err(e))?;

    let users = if auth.role.is_admin() {
        queries::search_users_by_query(&conn, &auth.org_id, q, limit)
            .map_err(|e| db_err(e))?
    } else {
        vec![]
    };

    let projects = queries::search_projects_by_query(&conn, &auth.org_id, q, limit)
        .map_err(|e| db_err(e))?;

    Ok(Json(GlobalSearchResult {
        memories,
        users,
        projects,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations};

    fn setup_db() -> rusqlite::Connection {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        conn
    }

    fn seed_org(conn: &rusqlite::Connection) -> String {
        let org_id = "org_test".to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Test Org', 'test-org')",
            [&org_id],
        ).unwrap();
        org_id
    }

    #[test]
    fn search_users_by_query_matches_name_and_email() {
        let conn = setup_db();
        let org_id = seed_org(&conn);
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES ('u1', ?1, 'alice@acme.com', 'Alice Smith', 'member', 'active', '2026-01-01T00:00:00Z')",
            [&org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES ('u2', ?1, 'bob@acme.com', 'Bob Jones', 'admin', 'active', '2026-01-01T00:00:00Z')",
            [&org_id],
        ).unwrap();

        let results = queries::search_users_by_query(&conn, &org_id, "alice", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Alice Smith");

        let results_email = queries::search_users_by_query(&conn, &org_id, "acme.com", 10).unwrap();
        assert_eq!(results_email.len(), 2);

        let results_none = queries::search_users_by_query(&conn, &org_id, "nobody", 10).unwrap();
        assert!(results_none.is_empty());
    }

    #[test]
    fn search_projects_by_query_matches_name() {
        let conn = setup_db();
        let org_id = seed_org(&conn);
        conn.execute(
            "INSERT INTO projects (id, org_id, name, created_at) VALUES ('p1', ?1, 'payments-service', '2026-01-01T00:00:00Z')",
            [&org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO projects (id, org_id, name, created_at) VALUES ('p2', ?1, 'auth-service', '2026-01-01T00:00:00Z')",
            [&org_id],
        ).unwrap();

        let results = queries::search_projects_by_query(&conn, &org_id, "payment", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "payments-service");

        let results_all = queries::search_projects_by_query(&conn, &org_id, "service", 10).unwrap();
        assert_eq!(results_all.len(), 2);

        let results_none = queries::search_projects_by_query(&conn, &org_id, "nonexistent", 10).unwrap();
        assert!(results_none.is_empty());
    }

    #[test]
    fn search_users_org_isolation() {
        let conn = setup_db();
        let org_a = seed_org(&conn);
        let org_b = "org_b".to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org B', 'org-b')",
            [&org_b],
        ).unwrap();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES ('u_b', ?1, 'carol@b.com', 'Carol', 'member', 'active', '2026-01-01T00:00:00Z')",
            [&org_b],
        ).unwrap();

        let results = queries::search_users_by_query(&conn, &org_a, "carol", 10).unwrap();
        assert!(results.is_empty(), "org_a must not see org_b users");
    }
}
