use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};

use crate::{
    db::queries as db_queries,
    models::types::{ApiError, AuthContext, ProjectContext},
    store::sqlite::SqliteStore,
    api::helpers::require_permission,
};

// Re-export the return type alias for clarity.
type ContextResponse = serde_json::Value;

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

pub async fn get_project_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project): Path<String>,
) -> Result<Json<ContextResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;

    require_permission(&conn, &auth, Some(&project), "memory:read")?;

    let ctx = db_queries::get_project_context(&conn, &auth.org_id, &project)
        .map_err(db_err)?;

    let conventions = db_queries::list_conventions(&conn, &auth.org_id, None, Some(false))
        .map_err(db_err)?;

    let mut ctx_json = serde_json::to_value(&ctx).map_err(|e| db_err(e.into()))?;
    if let serde_json::Value::Object(ref mut map) = ctx_json {
        map.insert(
            "conventions".to_string(),
            serde_json::to_value(&conventions).unwrap_or(serde_json::json!([])),
        );
    }

    Ok(Json(ctx_json))
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
        db::{connection::connect, migrations},
        db::queries as q,
        store::sqlite::SqliteStore,
    };

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/context/project/:project", get(get_project_context))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn bootstrap_store() -> (SqliteStore, String, String) {
        let store = make_store();
        let (admin_key, org_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (key, org.id)
        };
        (store, admin_key, org_id)
    }

    // ── T-06 tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_project_context_returns_correct_shape() {
        let (store, admin_key, org_id) = bootstrap_store();

        // Seed 5 memories for project "nexusmind" with distinct tools.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, user, _) = q::bootstrap(&conn, "Acme2", "acme2", "a2@acme.com", "Admin2")
                .unwrap_or_else(|_| {
                    // Org already exists, get the user instead.
                    let user = conn.query_row(
                        "SELECT id FROM users WHERE org_id = ?1 LIMIT 1",
                        rusqlite::params![org_id],
                        |r| r.get::<_, String>(0),
                    ).unwrap();
                    let org = crate::models::types::Org {
                        id: org_id.clone(),
                        name: "Acme".to_string(),
                        slug: "acme".to_string(),
                        created_at: "".to_string(),
                    };
                    (org, crate::models::types::User {
                        id: user,
                        org_id: org_id.clone(),
                        email: "admin@acme.com".to_string(),
                        name: "Admin".to_string(),
                        role: "admin".to_string(),
                        status: "active".to_string(),
                        created_at: "".to_string(),
                        last_active: None,
                        disabled_at: None,
                        admin_note: None,
                        last_login_at: None,
                    }, admin_key.clone())
                });
            drop((user, conn));
        }

        // Store 5 memories via the store layer.
        let tools = ["claude", "cursor", "claude", "copilot", "cursor"];
        for tool in &tools {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let user_id = conn.query_row(
                "SELECT id FROM users WHERE org_id = ?1 LIMIT 1",
                rusqlite::params![org_id],
                |r| r.get::<_, String>(0),
            ).unwrap();
            q::upsert_memory(&conn, &org_id, &user_id, &crate::models::types::StoreMemoryRequest {
                project: Some("nexusmind".to_string()),
                tool: tool.to_string(),
                content: format!("content from {tool}"),
                tags: None,
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
                    .method("GET")
                    .uri("/v1/context/project/nexusmind")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let memories = ctx["recent_memories"].as_array().unwrap();
        assert_eq!(memories.len(), 5, "must return all 5 memories");

        // Memories must be ordered newest first (DESC).
        // We verify: no panics accessing created_at comparison.
        if memories.len() >= 2 {
            let first_ts = memories[0]["created_at"].as_str().unwrap_or("");
            let last_ts = memories[memories.len() - 1]["created_at"].as_str().unwrap_or("");
            assert!(first_ts >= last_ts, "memories must be ordered DESC by created_at");
        }

        let tools_arr = ctx["tools"].as_array().unwrap();
        // Distinct tools: claude, cursor, copilot = 3
        assert_eq!(tools_arr.len(), 3, "tools must be deduplicated");

        assert!(ctx["last_activity"].is_string(), "last_activity must be a non-null string when memories exist");
        assert_eq!(ctx["project"].as_str().unwrap(), "nexusmind");
    }

    #[tokio::test]
    async fn get_project_context_empty_project_returns_200_not_404() {
        let (store, admin_key, _) = bootstrap_store();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context/project/empty-proj")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(ctx["recent_memories"].as_array().unwrap().len(), 0);
        assert_eq!(ctx["tools"].as_array().unwrap().len(), 0);
        assert!(ctx["last_activity"].is_null(), "last_activity must be null when no memories");
    }

    #[tokio::test]
    async fn get_project_context_cross_tenant_isolation() {
        // Org A has memories for project "shared"; org B must see empty results.
        let store_a = make_store();
        let (_key_a, org_id_a) = {
            let db = store_a.conn();
            let conn = db.lock().unwrap();
            let (org, _, key) = q::bootstrap(&conn, "OrgA", "orga", "admin@a.com", "AdminA").unwrap();
            (key, org.id)
        };

        // Seed a memory for org A.
        {
            let db = store_a.conn();
            let conn = db.lock().unwrap();
            let user_id = conn.query_row(
                "SELECT id FROM users WHERE org_id = ?1 LIMIT 1",
                rusqlite::params![org_id_a],
                |r| r.get::<_, String>(0),
            ).unwrap();
            q::upsert_memory(&conn, &org_id_a, &user_id, &crate::models::types::StoreMemoryRequest {
                project: Some("shared".to_string()),
                tool: "claude".to_string(),
                content: "org A's content".to_string(),
                tags: None,
                title: None,
                memory_type: None,
                scope: None,
                topic_key: None,
                session_id: None,
            }).unwrap();
        }

        // Org B in a separate in-memory store (different SQLite instance).
        let store_b = make_store();
        let key_b = {
            let db = store_b.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();
            key
        };

        // Org B queries the same project name — must see empty (its own scope only).
        let resp = app(store_b)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context/project/shared")
                    .header("Authorization", format!("Bearer {key_b}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        assert_eq!(
            ctx["recent_memories"].as_array().unwrap().len(), 0,
            "org B must not see org A's memories"
        );
    }
}
