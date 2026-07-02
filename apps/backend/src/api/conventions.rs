use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;

use crate::{
    api::helpers::AppJson,
    db::queries,
    models::types::{ApiError, AuthContext, Convention, CreateConventionRequest, UpdateConventionRequest},
    store::sqlite::SqliteStore,
};

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

fn not_found() -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::NOT_FOUND,
        Json(ApiError {
            error: "Convention not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

#[derive(Deserialize)]
pub struct ListParams {
    pub category: Option<String>,
    pub include_archived: Option<bool>,
    /// When set, scopes the result to org-wide conventions UNION this project's
    /// conventions. When absent, returns every convention for the org regardless
    /// of project_id (admin listing behavior).
    pub project: Option<String>,
}

pub async fn list_conventions(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<Convention>>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let conventions = queries::list_conventions(
        &conn,
        &auth.org_id,
        params.category.as_deref(),
        params.include_archived,
        params.project.as_deref(),
    ).map_err(db_err)?;
    Ok(Json(conventions))
}

pub async fn get_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<Json<Convention>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::get_convention(&conn, &auth.org_id, id)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(convention))
}

pub async fn create_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(req): AppJson<CreateConventionRequest>,
) -> Result<(StatusCode, Json<Convention>), (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::create_convention(&conn, &auth.org_id, &req).map_err(db_err)?;
    Ok((StatusCode::CREATED, Json(convention)))
}

pub async fn update_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
    AppJson(req): AppJson<UpdateConventionRequest>,
) -> Result<Json<Convention>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let convention = queries::update_convention(&conn, &auth.org_id, id, &req)
        .map_err(db_err)?
        .ok_or_else(not_found)?;
    Ok(Json(convention))
}

pub async fn delete_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let deleted = queries::delete_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn archive_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let ok = queries::archive_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

pub async fn restore_convention(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<i64>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_admin() {
        return Err(forbidden());
    }
    let db = store.conn();
    let conn = db.lock().map_err(|_| db_err(anyhow::anyhow!("db lock poisoned")))?;
    let ok = queries::restore_convention(&conn, &auth.org_id, id).map_err(db_err)?;
    if ok {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::util::ServiceExt;

    use crate::api::middleware as auth_mw;
    use crate::db::{connection::connect, migrations, queries};
    use crate::db::queries::bootstrap;
    use crate::store::sqlite::SqliteStore;

    fn make_store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/conventions", get(super::list_conventions).post(super::create_convention))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn admin_key(store: &SqliteStore) -> (String, String) {
        let db = store.conn();
        let conn = db.lock().unwrap();
        let (org, _, key) = bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
        (org.id, key)
    }

    async fn body_json(resp: axum::response::Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn list_scoped_to_project_returns_org_wide_union_project() {
        let store = make_store();
        let (org_id, key) = admin_key(&store);

        let project_a_id = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let project_a = queries::create_project(&conn, &org_id, "proj-a", None, None).unwrap();
            let project_q = queries::create_project(&conn, &org_id, "proj-q", None, None).unwrap();

            queries::create_convention(&conn, &org_id, &crate::models::types::CreateConventionRequest {
                title: "Org-wide".to_string(),
                content: "content".to_string(),
                category: None,
                weight: None,
                tags: None,
                project_id: None,
            }).unwrap();
            queries::create_convention(&conn, &org_id, &crate::models::types::CreateConventionRequest {
                title: "Proj A".to_string(),
                content: "content".to_string(),
                category: None,
                weight: None,
                tags: None,
                project_id: Some(project_a.id.clone()),
            }).unwrap();
            queries::create_convention(&conn, &org_id, &crate::models::types::CreateConventionRequest {
                title: "Proj Q".to_string(),
                content: "content".to_string(),
                category: None,
                weight: None,
                tags: None,
                project_id: Some(project_q.id.clone()),
            }).unwrap();
            project_a.id
        };

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri(format!("/v1/conventions?project={project_a_id}"))
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let titles: Vec<&str> = body.as_array().unwrap().iter()
            .map(|c| c["title"].as_str().unwrap())
            .collect();
        assert_eq!(titles.len(), 2, "must return org-wide UNION project A, not project Q");
        assert!(titles.contains(&"Org-wide"));
        assert!(titles.contains(&"Proj A"));
        assert!(!titles.contains(&"Proj Q"));
    }

    #[tokio::test]
    async fn list_without_project_returns_everything_for_org() {
        let store = make_store();
        let (org_id, key) = admin_key(&store);

        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let project_a = queries::create_project(&conn, &org_id, "proj-a", None, None).unwrap();
            queries::create_convention(&conn, &org_id, &crate::models::types::CreateConventionRequest {
                title: "Org-wide".to_string(),
                content: "content".to_string(),
                category: None,
                weight: None,
                tags: None,
                project_id: None,
            }).unwrap();
            queries::create_convention(&conn, &org_id, &crate::models::types::CreateConventionRequest {
                title: "Proj A".to_string(),
                content: "content".to_string(),
                category: None,
                weight: None,
                tags: None,
                project_id: Some(project_a.id.clone()),
            }).unwrap();
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/conventions")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 2, "no project param must return everything for the org (admin listing)");
    }

    // ── pagination tests ──────────────────────────────────────────────────────

    fn create_convention_with_weight(conn: &rusqlite::Connection, org_id: &str, title: &str, weight: i64) {
        queries::create_convention(conn, org_id, &crate::models::types::CreateConventionRequest {
            title: title.to_string(),
            content: "content".to_string(),
            category: None,
            weight: Some(weight),
            tags: None,
            project_id: None,
        }).unwrap();
    }

    #[tokio::test]
    async fn list_default_returns_everything_under_the_default_limit() {
        let store = make_store();
        let (org_id, key) = admin_key(&store);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..3 {
                create_convention_with_weight(&conn, &org_id, &format!("C{i}"), i);
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/conventions")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 3, "no limit/offset must still return everything under the default cap");
    }

    #[tokio::test]
    async fn list_respects_explicit_limit_and_offset() {
        let store = make_store();
        let (org_id, key) = admin_key(&store);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            // Weight DESC ordering: W50, W40, W30, W20, W10
            create_convention_with_weight(&conn, &org_id, "W50", 50);
            create_convention_with_weight(&conn, &org_id, "W40", 40);
            create_convention_with_weight(&conn, &org_id, "W30", 30);
            create_convention_with_weight(&conn, &org_id, "W20", 20);
            create_convention_with_weight(&conn, &org_id, "W10", 10);
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/conventions?limit=2&offset=1")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = body_json(resp).await;
        let titles: Vec<&str> = body.as_array().unwrap().iter().map(|c| c["title"].as_str().unwrap()).collect();
        assert_eq!(titles, vec!["W40", "W30"], "limit=2&offset=1 must return the 2nd and 3rd highest-weight conventions");
    }

    #[tokio::test]
    async fn list_limit_is_clamped_to_500_not_rejected() {
        let store = make_store();
        let (org_id, key) = admin_key(&store);
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            for i in 0..505 {
                create_convention_with_weight(&conn, &org_id, &format!("C{i}"), 505 - i);
            }
        }

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/conventions?limit=10000")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "an over-max limit must be clamped, never rejected");
        let body = body_json(resp).await;
        assert_eq!(body.as_array().unwrap().len(), 500, "limit must be clamped to the 500 max, not the requested 10000 or the full 505 rows");
    }
}
