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

    require_permission(&conn, &auth, None, "memory:search")?;

    let q = params.q.trim();
    if q.is_empty() {
        return Ok(Json(GlobalSearchResult {
            memories: vec![],
            users: vec![],
            projects: vec![],
            policies: vec![],
            conventions: vec![],
        }));
    }

    let limit = params.limit.clamp(1, 50);

    let memories = queries::search_memories(&conn, &auth.org_id, q, limit)
        .map_err(db_err)?;

    let users = if auth.role.is_admin() {
        queries::search_users_by_query(&conn, &auth.org_id, q, limit)
            .map_err(db_err)?
    } else {
        vec![]
    };

    let projects = queries::search_projects_by_query(&conn, &auth.org_id, q, limit)
        .map_err(db_err)?;

    let policies = queries::search_policies_by_query(&conn, &auth.org_id, q, limit)
        .map_err(db_err)?;

    let conventions = queries::search_conventions_by_query(&conn, &auth.org_id, q, limit)
        .map_err(db_err)?;

    Ok(Json(GlobalSearchResult {
        memories,
        users,
        projects,
        policies,
        conventions,
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

    #[test]
    fn search_policies_by_query_matches_name() {
        let conn = setup_db();
        let org_id = seed_org(&conn);
        conn.execute(
            "INSERT INTO policies (id, org_id, name, rule_type, config, enabled, created_at, updated_at)
             VALUES ('pol1', ?1, 'Block GPT Models', 'model_whitelist', '{\"allowed_models\":[]}', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [&org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, org_id, name, rule_type, config, enabled, created_at, updated_at)
             VALUES ('pol2', ?1, 'Budget Cap', 'budget_limit', '{\"max_tokens_per_day\":100000}', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [&org_id],
        ).unwrap();

        let results = queries::search_policies_by_query(&conn, &org_id, "gpt", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "Block GPT Models");

        let results_budget = queries::search_policies_by_query(&conn, &org_id, "budget", 10).unwrap();
        assert_eq!(results_budget.len(), 1);
        assert_eq!(results_budget[0].name, "Budget Cap");

        let results_none = queries::search_policies_by_query(&conn, &org_id, "nonexistent", 10).unwrap();
        assert!(results_none.is_empty());
    }

    #[test]
    fn search_conventions_by_query_matches_title_and_content() {
        let conn = setup_db();
        let org_id = seed_org(&conn);
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags)
             VALUES (?1, 'Use snake_case', 'All identifiers must use snake_case naming', 'naming', 100, '[]')",
            [&org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags)
             VALUES (?1, 'Error handling', 'Always use anyhow for error propagation', 'patterns', 90, '[]')",
            [&org_id],
        ).unwrap();

        // match by title
        let by_title = queries::search_conventions_by_query(&conn, &org_id, "snake", 10).unwrap();
        assert_eq!(by_title.len(), 1);
        assert_eq!(by_title[0].title, "Use snake_case");

        // match by content
        let by_content = queries::search_conventions_by_query(&conn, &org_id, "anyhow", 10).unwrap();
        assert_eq!(by_content.len(), 1);
        assert_eq!(by_content[0].title, "Error handling");

        // no match
        let no_match = queries::search_conventions_by_query(&conn, &org_id, "typescript", 10).unwrap();
        assert!(no_match.is_empty());
    }

    #[test]
    fn search_conventions_excludes_archived() {
        let conn = setup_db();
        let org_id = seed_org(&conn);
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags, archived_at)
             VALUES (?1, 'Old convention', 'This was archived', 'naming', 50, '[]', '2026-01-01T00:00:00Z')",
            [&org_id],
        ).unwrap();
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags)
             VALUES (?1, 'Old active convention', 'This is active', 'naming', 50, '[]')",
            [&org_id],
        ).unwrap();

        let results = queries::search_conventions_by_query(&conn, &org_id, "old", 10).unwrap();
        assert_eq!(results.len(), 1, "archived convention must be excluded");
        assert_eq!(results[0].title, "Old active convention");
    }

    #[test]
    fn search_policies_org_isolation() {
        let conn = setup_db();
        let org_a = seed_org(&conn);
        let org_b = "org_b".to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org B', 'org-b')",
            [&org_b],
        ).unwrap();
        conn.execute(
            "INSERT INTO policies (id, org_id, name, rule_type, config, enabled, created_at, updated_at)
             VALUES ('pol_b', ?1, 'B Policy', 'model_whitelist', '{}', 1, '2026-01-01T00:00:00.000Z', '2026-01-01T00:00:00.000Z')",
            [&org_b],
        ).unwrap();

        let results = queries::search_policies_by_query(&conn, &org_a, "policy", 10).unwrap();
        assert!(results.is_empty(), "org_a must not see org_b policies");
    }

    #[test]
    fn search_conventions_org_isolation() {
        let conn = setup_db();
        let org_a = seed_org(&conn);
        let org_b = "org_b".to_string();
        conn.execute(
            "INSERT INTO organizations (id, name, slug) VALUES (?1, 'Org B', 'org-b')",
            [&org_b],
        ).unwrap();
        conn.execute(
            "INSERT INTO conventions (org_id, title, content, category, weight, tags)
             VALUES (?1, 'B Convention', 'some content', 'general', 100, '[]')",
            [&org_b],
        ).unwrap();

        let results = queries::search_conventions_by_query(&conn, &org_a, "convention", 10).unwrap();
        assert!(results.is_empty(), "org_a must not see org_b conventions");
    }
}
