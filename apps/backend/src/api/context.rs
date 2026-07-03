use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    db::queries as db_queries,
    models::types::{ApiError, AuthContext, MemoryPreview},
    store::sqlite::SqliteStore,
    api::helpers::require_permission,
};

// Re-export the return type alias for clarity.
type ContextResponse = serde_json::Value;

/// Max embedded conventions on `get_project_context` / `get_global_context`
/// responses, highest `weight` first. Embedded memories are already capped at
/// 20 by the underlying queries; conventions previously had no cap at all.
const MAX_CONTEXT_CONVENTIONS: i64 = 50;

fn db_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

/// Project-membership visibility scope: admins see all org memories (`None`),
/// non-admins are restricted to memories they may see (`Some(user_id)`).
fn viewer_scope(auth: &AuthContext) -> Option<&str> {
    if auth.role.is_admin() {
        None
    } else {
        Some(&auth.user_id)
    }
}

#[derive(Deserialize)]
pub struct ContextParams {
    /// When true, embedded memories use the compact `MemoryPreview` shape
    /// (see `api::memory`) instead of the full `Memory` row.
    #[serde(default)]
    pub compact: Option<bool>,
}

pub async fn get_project_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(project): Path<String>,
    Query(params): Query<ContextParams>,
) -> Result<Json<ContextResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;

    require_permission(&conn, &auth, Some(&project), "memory:read")?;

    let ctx = db_queries::get_project_context(&conn, &auth.org_id, &project)
        .map_err(db_err)?;

    // Scope to org-wide + this project's conventions (project scoping ADDS to org-wide).
    let conventions = db_queries::list_conventions(&conn, &auth.org_id, None, Some(false), Some(&project), MAX_CONTEXT_CONVENTIONS, 0)
        .map_err(db_err)?;

    let mut ctx_json = if params.compact.unwrap_or(false) {
        let previews: Vec<MemoryPreview> = ctx.recent_memories.iter().map(MemoryPreview::from).collect();
        serde_json::json!({
            "project": ctx.project,
            "recent_memories": previews,
            "tools": ctx.tools,
            "last_activity": ctx.last_activity,
        })
    } else {
        serde_json::to_value(&ctx).map_err(|e| db_err(e.into()))?
    };
    if let serde_json::Value::Object(ref mut map) = ctx_json {
        map.insert(
            "conventions".to_string(),
            serde_json::to_value(&conventions).unwrap_or(serde_json::json!([])),
        );
    }

    Ok(Json(ctx_json))
}

fn build_context_response(
    memories: Vec<serde_json::Value>,
    label_key: &str,
    label_val: serde_json::Value,
) -> serde_json::Value {
    let tools: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        memories
            .iter()
            .filter_map(|m| m["tool"].as_str().map(|s| s.to_string()))
            .filter(|t| seen.insert(t.clone()))
            .collect()
    };
    let last_activity = memories
        .iter()
        .filter_map(|m| m["created_at"].as_str())
        .max()
        .map(|s| serde_json::Value::String(s.to_string()))
        .unwrap_or(serde_json::Value::Null);

    serde_json::json!({
        label_key: label_val,
        "recent_memories": memories,
        "tools": tools,
        "last_activity": last_activity,
    })
}

pub async fn get_global_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ContextParams>,
) -> Result<Json<ContextResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;

    require_permission(&conn, &auth, None, "memory:read")?;

    let viewer = viewer_scope(&auth);
    let memories = db_queries::list_memories_visible(
        &conn,
        &auth.org_id,
        None, None, None, None, None, None,
        20, 0, false, None, None, None,
        viewer,
    )
    .map_err(db_err)?;

    let compact = params.compact.unwrap_or(false);

    // Derive tools/last_activity from the FULL memories before converting to the
    // compact preview shape — MemoryPreview has no `tool` field, so deriving from
    // the post-conversion values would always yield an empty tools list.
    let full_values: Vec<serde_json::Value> = memories
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
        .collect();

    let memory_values: Vec<serde_json::Value> = if compact {
        memories
            .iter()
            .map(|m| serde_json::to_value(MemoryPreview::from(m)).unwrap_or(serde_json::Value::Null))
            .collect()
    } else {
        full_values.clone()
    };

    // Global context has no project in scope — admin listing (everything for the org).
    let conventions = db_queries::list_conventions(&conn, &auth.org_id, None, Some(false), None, MAX_CONTEXT_CONVENTIONS, 0)
        .map_err(db_err)?;

    let mut resp = build_context_response(full_values, "scope", serde_json::json!("global"));
    if let serde_json::Value::Object(ref mut map) = resp {
        map.insert("recent_memories".to_string(), serde_json::Value::Array(memory_values));
    }
    if let serde_json::Value::Object(ref mut map) = resp {
        map.insert(
            "conventions".to_string(),
            serde_json::to_value(&conventions).unwrap_or(serde_json::json!([])),
        );
    }

    Ok(Json(resp))
}

pub async fn get_type_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(memory_type): Path<String>,
) -> Result<Json<ContextResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;

    require_permission(&conn, &auth, None, "memory:read")?;

    let viewer = viewer_scope(&auth);
    let memories = db_queries::list_memories_visible(
        &conn,
        &auth.org_id,
        None, None, None, Some(&memory_type), None, None,
        20, 0, false, None, None, None,
        viewer,
    )
    .map_err(db_err)?;

    let memory_values: Vec<serde_json::Value> = memories
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
        .collect();

    Ok(Json(build_context_response(
        memory_values,
        "type",
        serde_json::Value::String(memory_type),
    )))
}

pub async fn get_session_context(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(session_id): Path<String>,
) -> Result<Json<ContextResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;

    require_permission(&conn, &auth, None, "memory:read")?;

    let viewer = viewer_scope(&auth);
    let memories = db_queries::list_memories_visible(
        &conn,
        &auth.org_id,
        None, None, None, None, None, Some(&session_id),
        20, 0, false, None, None, None,
        viewer,
    )
    .map_err(db_err)?;

    let memory_values: Vec<serde_json::Value> = memories
        .iter()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
        .collect();

    Ok(Json(build_context_response(
        memory_values,
        "session_id",
        serde_json::Value::String(session_id),
    )))
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
            .route("/v1/context", get(get_global_context))
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

    fn create_member_with_id(store: &SqliteStore, org_id: &str) -> (String, String) {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', 'member', 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{}-member@test.com", &user_id[..8])],
        ).unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        ).unwrap();
        (raw_key, user_id)
    }

    /// Non-admins must not see a non-member project's memory via GET /v1/context (global).
    #[tokio::test]
    async fn global_context_hides_non_member_project_memories() {
        let (store, _admin_key, org_id) = bootstrap_store();

        // Seed the secret memory (creating proj-secret) BEFORE the member exists, so the
        // member is not auto-enrolled by get_or_create_project.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let admin_id: String = conn
                .query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", rusqlite::params![org_id], |r| r.get(0))
                .unwrap();
            // create_project does NOT auto-enroll members, so the member (created below)
            // is never a member of proj-secret.
            q::create_project(&conn, &org_id, "proj-secret", None, None).unwrap();
            q::upsert_memory(&conn, &org_id, &admin_id, &crate::models::types::StoreMemoryRequest {
                project: Some("proj-secret".to_string()),
                tool: "claude".to_string(),
                content: "SECRETALPHA in context".to_string(),
                tags: None, title: None, memory_type: None, scope: None, topic_key: None, session_id: None,
            }).unwrap();
        }

        let (member_key, _member_id) = create_member_with_id(&store, &org_id);

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/context")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        assert!(!body.contains("SECRETALPHA"), "global context must not leak a non-member project's memory");
    }

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
            q::get_or_create_project(&conn, &org_id, "nexusmind").unwrap();
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
            q::get_or_create_project(&conn, &org_id_a, "shared").unwrap();
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

    // ── Change 5: context payload caps ────────────────────────────────────────

    fn create_convention_with_weight(conn: &rusqlite::Connection, org_id: &str, title: &str, weight: i64) {
        q::create_convention(conn, org_id, &crate::models::types::CreateConventionRequest {
            title: title.to_string(),
            content: "content".to_string(),
            category: None,
            weight: Some(weight),
            tags: None,
            project_id: None,
        }).unwrap();
    }

    #[tokio::test]
    async fn get_project_context_caps_conventions_at_50_ordered_by_weight() {
        let (store, admin_key, org_id) = bootstrap_store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..55 {
                create_convention_with_weight(&conn, &org_id, &format!("C{i}"), i);
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context/project/any-project")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let conventions = ctx["conventions"].as_array().unwrap();
        assert_eq!(conventions.len(), 50, "embedded conventions must be capped at 50");
        assert_eq!(conventions[0]["title"].as_str().unwrap(), "C54", "must be ordered by weight DESC — highest weight first");
    }

    #[tokio::test]
    async fn get_global_context_caps_conventions_at_50_ordered_by_weight() {
        let (store, admin_key, org_id) = bootstrap_store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..55 {
                create_convention_with_weight(&conn, &org_id, &format!("C{i}"), i);
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let conventions = ctx["conventions"].as_array().unwrap();
        assert_eq!(conventions.len(), 50, "embedded conventions must be capped at 50");
        assert_eq!(conventions[0]["title"].as_str().unwrap(), "C54", "must be ordered by weight DESC — highest weight first");
    }

    #[tokio::test]
    async fn get_project_context_compact_true_applies_preview_to_memories() {
        let (store, admin_key, org_id) = bootstrap_store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::get_or_create_project(&conn, &org_id, "proj-compact").unwrap();
            q::upsert_memory(&conn, &org_id, &{
                conn.query_row("SELECT id FROM users WHERE org_id = ?1 LIMIT 1", rusqlite::params![org_id], |r| r.get::<_, String>(0)).unwrap()
            }, &crate::models::types::StoreMemoryRequest {
                project: Some("proj-compact".to_string()),
                tool: "claude".to_string(),
                content: "x".repeat(300),
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
                    .uri("/v1/context/project/proj-compact?compact=true")
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
        assert_eq!(memories.len(), 1);
        let obj = memories[0].as_object().unwrap();
        assert!(obj.contains_key("preview"), "compact=true must apply the preview shape to embedded memories");
        assert!(obj.get("content").is_none(), "compact=true must not include full content");
        assert_eq!(obj["preview"].as_str().unwrap().chars().count(), 200);
    }

    #[tokio::test]
    async fn get_global_context_without_compact_keeps_full_memory_shape() {
        let (store, admin_key, _) = bootstrap_store();
        // No memories needed — just confirm the default (non-compact) response shape
        // for the endpoint is unaffected by the new query param support.
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let ctx: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(ctx["recent_memories"].is_array());
    }

    #[tokio::test]
    async fn get_global_context_compact_true_still_returns_distinct_tools() {
        let (store, admin_key, org_id) = bootstrap_store();
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let user_id = conn.query_row(
                "SELECT id FROM users WHERE org_id = ?1 LIMIT 1",
                rusqlite::params![org_id],
                |r| r.get::<_, String>(0),
            ).unwrap();
            q::get_or_create_project(&conn, &org_id, "global-compact").unwrap();
            for tool in ["claude", "cursor", "claude"] {
                q::upsert_memory(&conn, &org_id, &user_id, &crate::models::types::StoreMemoryRequest {
                    project: Some("global-compact".to_string()),
                    tool: tool.to_string(),
                    content: "x".repeat(300),
                    tags: None,
                    title: None,
                    memory_type: None,
                    scope: None,
                    topic_key: None,
                    session_id: None,
                }).unwrap();
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/context?compact=true")
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
        assert_eq!(memories.len(), 3);
        assert!(
            memories[0].as_object().unwrap().contains_key("preview"),
            "compact=true must still apply the preview shape to embedded memories"
        );

        let tools_arr = ctx["tools"].as_array().unwrap();
        assert_eq!(
            tools_arr.len(), 2,
            "compact=true must not lose the distinct tool list (regression: tools derived from preview shape lacking `tool` field)"
        );
    }
}
