use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};

use crate::{
    db::queries as db_queries,
    models::types::{ApiError, AuthContext, Memory, MemoryGraphResponse, MemoryPage, MemoryPreview, PolicyCheckRequest, StoreMemoryRequest, UpdateMemoryRequest},
    store::{sqlite::SqliteStore, MemoryFilters, MemoryStore, SearchMode},
    api::helpers::{require_permission, AppJson, JsonBody},
};

const EXPORT_HARD_CAP: i64 = 10_000;
const MAX_CONTENT_BYTES: usize = 65_536; // 64 KiB

#[derive(Deserialize)]
pub struct ExportParams {
    #[serde(default = "default_csv")]
    pub format: ExportFormat,
}

#[derive(Deserialize, Copy, Clone, Eq, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    Csv,
    Json,
}

fn default_csv() -> ExportFormat {
    ExportFormat::Csv
}

fn truncate_content(s: &str) -> String {
    const MAX: usize = 500;
    for (count, (idx, _)) in s.char_indices().enumerate() {
        if count == MAX {
            return format!("{}…", &s[..idx]);
        }
    }
    s.to_string()
}

fn memory_rows_to_csv(memories: &[Memory]) -> anyhow::Result<Vec<u8>> {
    let mut wtr = csv::WriterBuilder::new()
        .quote_style(csv::QuoteStyle::Necessary)
        .from_writer(Vec::new());

    wtr.write_record(["id", "title", "type", "scope", "project", "tool", "content", "tags", "topic_key", "session_id", "revision_count", "pinned", "created_at"])?;

    for m in memories {
        let title = m.title.as_deref().unwrap_or("");
        let memory_type = m.memory_type.as_deref().unwrap_or("");
        let content = truncate_content(&m.content);
        let tags = m.tags.join(";");
        let topic_key = m.topic_key.as_deref().unwrap_or("");
        let session_id = m.session_id.as_deref().unwrap_or("");
        let revision_count = m.revision_count.to_string();
        let pinned = m.pinned.to_string();
        wtr.write_record([
            &m.id,
            title,
            memory_type,
            &m.scope,
            &m.project,
            &m.tool,
            &content,
            &tags,
            topic_key,
            session_id,
            &revision_count,
            &pinned,
            &m.created_at,
        ])?;
    }

    Ok(wtr.into_inner()?)
}

pub async fn export(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ExportParams>,
) -> Result<Response, (StatusCode, Json<ApiError>)> {
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, None, "memory:read")?;
    }

    let filters = MemoryFilters {
        user_id: None,
        tool: None,
        project: None,
        memory_type: None,
        scope: None,
        session_id: None,
        limit: EXPORT_HARD_CAP,
        offset: 0,
        include_archived: false,
        from_date: None,
        to_date: None,
        collection_id: None,
    };
    let page = store.list(&auth.org_id, &filters).map_err(store_err)?;
    let memories = page.memories;

    let truncated = memories.len() as i64 == EXPORT_HARD_CAP;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    let (content_type, filename, body) = match params.format {
        ExportFormat::Csv => {
            let body = memory_rows_to_csv(&memories).map_err(store_err)?;
            (
                "text/csv; charset=utf-8",
                format!("memories-{today}.csv"),
                body,
            )
        }
        ExportFormat::Json => {
            let body = serde_json::to_vec_pretty(&memories)
                .map_err(|e| store_err(anyhow::anyhow!(e)))?;
            (
                "application/json; charset=utf-8",
                format!("memories-{today}.json"),
                body,
            )
        }
    };

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")).unwrap(),
    );
    if truncated {
        headers.insert("x-export-truncated", HeaderValue::from_static("true"));
    }

    Ok((StatusCode::OK, headers, body).into_response())
}

#[derive(Deserialize)]
pub struct SearchInput {
    pub query: String,
    pub limit: Option<i64>,
    /// Search mode: "hybrid" (default), "keyword", or "semantic".
    pub mode: Option<String>,
    /// When true, each returned item is a compact `MemoryPreview` instead of
    /// the full `Memory` row. Absent/false leaves the response unchanged.
    #[serde(default)]
    pub compact: Option<bool>,
}

#[derive(Deserialize)]
pub struct ListParams {
    pub user_id: Option<String>,
    pub tool: Option<String>,
    pub project: Option<String>,
    #[serde(rename = "type")]
    pub type_filter: Option<String>,
    pub scope: Option<String>,
    pub session_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    #[serde(default)]
    pub include_archived: bool,
    /// ISO 8601 date string (e.g. "2025-01-01"). When set, only memories created on or after
    /// this date are returned.
    pub from_date: Option<String>,
    /// ISO 8601 date string (e.g. "2025-01-31"). When set, only memories created on or before
    /// this date (inclusive) are returned.
    pub to_date: Option<String>,
    /// When set, only memories belonging to this collection are returned.
    pub collection_id: Option<String>,
    /// When true, each returned item is a compact `MemoryPreview` instead of
    /// the full `Memory` row. Absent/false leaves the response unchanged.
    #[serde(default)]
    pub compact: Option<bool>,
}

/// `GET /v1/memory` and `POST /v1/memory/search` return either the full
/// `MemoryPage<Memory>` shape (default) or `MemoryPage<MemoryPreview>` when
/// `compact=true` is requested. `#[serde(untagged)]` serializes whichever
/// variant is constructed as a plain object — the default shape is byte-for-byte
/// identical to before this type existed, so existing consumers are unaffected.
#[derive(Serialize)]
#[serde(untagged)]
pub enum MemoryPageResponse {
    Full(MemoryPage<Memory>),
    Compact(MemoryPage<MemoryPreview>),
}

fn store_err(e: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let msg = e.to_string();
    if msg.starts_with("invalid_session_id:") {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: msg.replacen("invalid_session_id:", "session_id '", 1) + "' not found for this org",
                code: "invalid_session_id".to_string(),
            }),
        );
    }
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: msg,
            code: "internal_error".to_string(),
        }),
    )
}

const MAX_TAG_LENGTH: usize = 100;
const MAX_TAGS: usize = 50;

fn validate_and_normalize_tags(
    tags: Option<Vec<String>>,
) -> Result<Option<Vec<String>>, (StatusCode, Json<ApiError>)> {
    let Some(tags) = tags else { return Ok(None) };

    if tags.len() > MAX_TAGS {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: format!("Too many tags — maximum is {MAX_TAGS} per memory"),
                code: "validation_error".to_string(),
            }),
        ));
    }

    let mut normalized = Vec::with_capacity(tags.len());
    for tag in tags {
        let trimmed = tag.trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_TAG_LENGTH {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    error: format!(
                        "Tag exceeds maximum length of {MAX_TAG_LENGTH} characters"
                    ),
                    code: "validation_error".to_string(),
                }),
            ));
        }
        normalized.push(trimmed);
    }

    Ok(Some(normalized))
}

pub async fn store(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    JsonBody(mut input): JsonBody<StoreMemoryRequest>,
) -> Result<(StatusCode, Json<Memory>), (StatusCode, Json<ApiError>)> {
    if input.content.len() > MAX_CONTENT_BYTES {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(ApiError {
                error: format!("content exceeds maximum allowed size of {} bytes", MAX_CONTENT_BYTES),
                code: "content_too_large".to_string(),
            }),
        ));
    }
    input.tags = validate_and_normalize_tags(input.tags.take())?;

    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        let project = input.project.as_deref().unwrap_or("default");
        require_permission(&conn, &auth, Some(project), "memory:write")?;

        // Enforce pii_redact policies against memory content before storing.
        // Scoped to org-wide + this memory's project (project ADDS to org-wide).
        let pii_policies: Vec<_> = db_queries::list_enabled_policies(&conn, &auth.org_id, input.project.as_deref())
            .unwrap_or_default()
            .into_iter()
            .filter(|p| p.rule_type == "pii_redact")
            .collect();

        if !pii_policies.is_empty() {
            let check_req = PolicyCheckRequest {
                model: String::new(),
                prompt_tokens: None,
                prompt_preview: Some(input.content.clone()),
                user_id: None,
                project: input.project.clone(),
            };
            let result = crate::policy::evaluate(&pii_policies, &check_req, 0, 0);
            if !result.allowed {
                let reasons: Vec<&str> = result.violations.iter().map(|v| v.reason.as_str()).collect();
                return Err((
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(ApiError {
                        error: format!("Memory blocked by policy: {}", reasons.join("; ")),
                        code: "policy_violation".to_string(),
                    }),
                ));
            }
        }
    }

    let is_upsert = input.topic_key.is_some();
    let memory = store.store(&auth.org_id, &auth.user_id, &input).map_err(store_err)?;

    let status = if is_upsert && memory.revision_count > 1 {
        StatusCode::OK
    } else {
        StatusCode::CREATED
    };

    Ok((status, Json(memory)))
}

pub async fn search(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<SearchInput>,
) -> Result<Json<MemoryPageResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, None, "memory:read")?;
    }

    let limit = input.limit.unwrap_or(20);
    let mode = input.mode.as_deref().unwrap_or("hybrid").parse::<SearchMode>().unwrap_or(SearchMode::Hybrid);
    // `store.search` silently downgrades semantic/hybrid to keyword search when no
    // embed service is configured (see SqliteStore::search) — surface that here so
    // callers know results are degraded rather than assuming semantic ranking.
    let degraded = if matches!(mode, SearchMode::Semantic | SearchMode::Hybrid) && store.embed_service().is_none() {
        Some("keyword-fallback".to_string())
    } else {
        None
    };
    let mut memories = store
        .search(&auth.org_id, &auth.user_id, &input.query, limit, mode)
        .map_err(store_err)?;
    // Strip admin_note — never exposed to agents or non-admin callers.
    if !auth.role.is_admin() {
        for m in &mut memories {
            m.admin_note = None;
        }
    }
    let total = memories.len() as i64;

    // Audit the search with rich detail (query + a sample of returned results,
    // grouped client-side by project) so the activity feed can render it as a
    // hierarchical tree instead of a bare "search · memory" row.
    {
        let results: Vec<serde_json::Value> = memories
            .iter()
            .take(8)
            .map(|m| {
                let title = m
                    .title
                    .clone()
                    .filter(|t| !t.trim().is_empty())
                    .unwrap_or_else(|| m.content.chars().take(60).collect::<String>());
                serde_json::json!({
                    "id": m.id,
                    "title": title,
                    "project": m.project,
                    "type": m.memory_type,
                })
            })
            .collect();
        if let Ok(conn) = store.conn().lock() {
            let _ = db_queries::log_audit(
                &conn,
                &auth.org_id,
                &auth.user_id,
                "search",
                "memory",
                None,
                serde_json::json!({
                    "query": input.query,
                    "mode": input.mode.as_deref().unwrap_or("hybrid"),
                    "result_count": total,
                    "results": results,
                }),
            );
        }
    }

    if input.compact.unwrap_or(false) {
        let previews: Vec<MemoryPreview> = memories.iter().map(MemoryPreview::from).collect();
        return Ok(Json(MemoryPageResponse::Compact(MemoryPage {
            memories: previews,
            total,
            limit,
            offset: 0,
            degraded,
        })));
    }

    Ok(Json(MemoryPageResponse::Full(MemoryPage { memories, total, limit, offset: 0, degraded })))
}

pub async fn list(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<MemoryPageResponse>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ApiError {
                error: "Database lock error".to_string(),
                code: "internal_error".to_string(),
            }),
        ))?;
        require_permission(&conn, &auth, params.project.as_deref(), "memory:read")?;
    }

    if let Some(offset) = params.offset {
        if offset < 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "offset must be non-negative".to_string(),
                    code: "validation_error".to_string(),
                }),
            ));
        }
    }
    if let Some(limit) = params.limit {
        if limit < 0 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "limit must be non-negative".to_string(),
                    code: "validation_error".to_string(),
                }),
            ));
        }
    }

    let filters = MemoryFilters {
        user_id: params.user_id.as_deref(),
        tool: params.tool.as_deref(),
        project: params.project.as_deref(),
        memory_type: params.type_filter.as_deref(),
        scope: params.scope.as_deref(),
        session_id: params.session_id.as_deref(),
        limit: params.limit.unwrap_or(50),
        offset: params.offset.unwrap_or(0),
        include_archived: params.include_archived,
        from_date: params.from_date.as_deref(),
        to_date: params.to_date.as_deref(),
        collection_id: params.collection_id.as_deref(),
    };
    let mut page = store.list(&auth.org_id, &filters).map_err(store_err)?;
    // Strip admin_note — never exposed to agents or non-admin callers.
    if !auth.role.is_admin() {
        for m in &mut page.memories {
            m.admin_note = None;
        }
    }

    if params.compact.unwrap_or(false) {
        let previews: Vec<MemoryPreview> = page.memories.iter().map(MemoryPreview::from).collect();
        return Ok(Json(MemoryPageResponse::Compact(MemoryPage {
            memories: previews,
            total: page.total,
            limit: page.limit,
            offset: page.offset,
            degraded: page.degraded,
        })));
    }

    Ok(Json(MemoryPageResponse::Full(page)))
}

pub async fn get_by_id(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Memory>, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    let memory = db_queries::get_memory_by_id_for_org(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match memory {
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some(mut m) => {
            // Permission check: requires memory:read for the memory's project.
            require_permission(&conn, &auth, Some(&m.project.clone()), "memory:read")?;
            // Strip admin_note — never exposed to agents or non-admin callers.
            if !auth.role.is_admin() {
                m.admin_note = None;
            }
            Ok(Json(m))
        }
    }
}

pub async fn delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    {
        let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;
        // Fetch owner and project to enforce project overrides and ownership
        let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id).map_err(store_err)?;

        match details {
            None => return Err((
                StatusCode::NOT_FOUND,
                Json(ApiError {
                    error: "Memory not found".to_string(),
                    code: "not_found".to_string(),
                }),
            )),
            Some((ref owner_id, ref project_name)) => {
                require_permission(&conn, &auth, Some(project_name), "memory:delete")?;

                if *owner_id != auth.user_id && !auth.role.is_admin() {
                    let is_project_admin = match db_queries::get_project_member_role(&conn, &auth.org_id, project_name, &auth.user_id) {
                        Ok(Some(role_str)) => role_str == "admin",
                        _ => false,
                    };
                    if !is_project_admin {
                        return Err((
                            StatusCode::FORBIDDEN,
                            Json(ApiError {
                                error: "Insufficient permissions".to_string(),
                                code: "forbidden".to_string(),
                            }),
                        ));
                    }
                }
            }
        }
    }

    let deleted = store
        .delete(&auth.org_id, &auth.user_id, &id)
        .map_err(store_err)?;

    if deleted {
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

// ── Bulk delete ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BulkDeleteInput {
    pub ids: Vec<String>,
}

#[derive(Serialize)]
pub struct BulkDeleteResponse {
    pub deleted: usize,
}

pub async fn bulk_delete(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(input): AppJson<BulkDeleteInput>,
) -> Result<Json<BulkDeleteResponse>, (StatusCode, Json<ApiError>)> {
    if input.ids.is_empty() {
        return Ok(Json(BulkDeleteResponse { deleted: 0 }));
    }

    const MAX_BULK: usize = 500;
    if input.ids.len() > MAX_BULK {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: format!("Too many IDs — maximum is {MAX_BULK} per request"),
                code: "too_many_ids".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    // Require memory:delete on the default project as the gate.
    // Per-item ownership is enforced inside bulk_delete_memories.
    require_permission(&conn, &auth, None, "memory:delete")?;

    let is_admin = auth.role.is_admin();
    let deleted = db_queries::bulk_delete_memories(&conn, &auth.org_id, &input.ids, is_admin, &auth.user_id)
        .map_err(|e| store_err(anyhow::anyhow!(e)))?;

    Ok(Json(BulkDeleteResponse { deleted }))
}

pub async fn update(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    AppJson(input): AppJson<UpdateMemoryRequest>,
) -> Result<Json<Memory>, (StatusCode, Json<ApiError>)> {
    // Validate content if provided
    if let Some(ref c) = input.content {
        if c.trim().is_empty() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ApiError {
                    error: "content must not be empty".to_string(),
                    code: "validation_error".to_string(),
                }),
            ));
        }
        if c.len() > MAX_CONTENT_BYTES {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                Json(ApiError {
                    error: format!("content exceeds maximum allowed size of {} bytes", MAX_CONTENT_BYTES),
                    code: "content_too_large".to_string(),
                }),
            ));
        }
    }

    // At least one field must be present
    if input.content.is_none() && input.title.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: "at least one of 'content' or 'title' must be provided".to_string(),
                code: "validation_error".to_string(),
            }),
        ));
    }

    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    // Fetch the memory to check org ownership and project permissions
    let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match details {
        None => Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some((_, ref project_name)) => {
            require_permission(&conn, &auth, Some(project_name), "memory:write")?;

            let updated = db_queries::update_memory_fields(
                &conn,
                &auth.org_id,
                &id,
                input.content.as_deref(),
                input.title.as_deref(),
            )
            .map_err(store_err)?;

            match updated {
                Some(m) => {
                    let _ = db_queries::log_audit(
                        &conn,
                        &auth.org_id,
                        &auth.user_id,
                        "memory.updated",
                        "memory",
                        Some(&id),
                        serde_json::json!({}),
                    );
                    Ok(Json(m))
                }
                None => Err((
                    StatusCode::NOT_FOUND,
                    Json(ApiError {
                        error: "Memory not found".to_string(),
                        code: "not_found".to_string(),
                    }),
                )),
            }
        }
    }
}

// ── Archive / Restore ─────────────────────────────────────────────────────────

pub async fn archive(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    // Verify ownership / permission (same pattern as delete)
    let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match details {
        None => return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some((_, ref project_name)) => {
            require_permission(&conn, &auth, Some(project_name), "memory:write")?;
        }
    }

    let updated = db_queries::archive_memory(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found or already archived".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

pub async fn restore(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    // Verify ownership / permission
    let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match details {
        None => return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some((_, ref project_name)) => {
            require_permission(&conn, &auth, Some(project_name), "memory:write")?;
        }
    }

    let updated = db_queries::restore_memory(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    if updated {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found or not archived".to_string(),
                code: "not_found".to_string(),
            }),
        ))
    }
}

pub async fn pin(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match details {
        None => return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some((_, ref project_name)) => {
            require_permission(&conn, &auth, Some(project_name), "memory:write")?;
        }
    }

    let updated = db_queries::pin_memory(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

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

pub async fn unpin(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ApiError>)> {
    let db = store.conn();
    let conn = db.lock().map_err(|_| store_err(anyhow::anyhow!("db lock poisoned")))?;

    let details = db_queries::get_memory_owner_and_project(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

    match details {
        None => return Err((
            StatusCode::NOT_FOUND,
            Json(ApiError {
                error: "Memory not found".to_string(),
                code: "not_found".to_string(),
            }),
        )),
        Some((_, ref project_name)) => {
            require_permission(&conn, &auth, Some(project_name), "memory:write")?;
        }
    }

    let updated = db_queries::unpin_memory(&conn, &auth.org_id, &id)
        .map_err(store_err)?;

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

// ── GET /v1/memory/graph ────────────────────────────────────────────────────

const DEFAULT_MEM_GRAPH_LIMIT: i64 = 2_000;
const MAX_MEM_GRAPH_LIMIT: i64 = 10_000;

#[derive(Debug, Deserialize)]
pub struct GetMemoryGraphQuery {
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub since: Option<String>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// `GET /v1/memory/graph`
///
/// Query parameters:
///   - `project` (required): project name or id scoped to the caller's org — `422` if missing
///   - `since` (optional): ISO-8601 timestamp; only memories created at/after this are included
///   - `limit`  (optional, default 2000, max 10000): maximum number of Memory nodes (anchor cap)
///   - `offset` (optional, default 0)
///
/// Returns a read-only, on-the-fly graph of Memory/Project/Session/User/Collection/Tag
/// nodes and their relationships, scoped to `auth.org_id`. Envelope mirrors
/// `GET /v1/code/graph`'s `GraphResponse` shape so the frontend can reuse the same seam.
pub async fn get_graph(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GetMemoryGraphQuery>,
) -> Result<Json<MemoryGraphResponse>, (StatusCode, Json<ApiError>)> {
    let project = match params.project.as_deref().map(str::trim) {
        Some(p) if !p.is_empty() => p.to_string(),
        _ => {
            return Err((
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ApiError {
                    error: "project is required".to_string(),
                    code: "validation_error".to_string(),
                }),
            ))
        }
    };

    let db = store.conn();
    let conn = db.lock().map_err(|_| (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: "Database lock error".to_string(),
            code: "internal_error".to_string(),
        }),
    ))?;

    require_permission(&conn, &auth, Some(&project), "memory:read")?;

    let limit = params
        .limit
        .unwrap_or(DEFAULT_MEM_GRAPH_LIMIT)
        .clamp(1, MAX_MEM_GRAPH_LIMIT);
    let offset = params.offset.unwrap_or(0).max(0);

    let (nodes, edges) = db_queries::get_memory_graph(
        &conn,
        &auth.org_id,
        &project,
        params.since.as_deref(),
        limit,
        offset,
    )
    .map_err(store_err)?;

    let node_count = nodes.len();
    let edge_count = edges.len();

    Ok(Json(MemoryGraphResponse {
        project,
        node_count,
        edge_count,
        nodes,
        edges,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::{delete, get, post},
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
            .route("/v1/memory/store", post(super::store))
            .route("/v1/memory/search", post(search))
            .route("/v1/memory/bulk", delete(super::bulk_delete))
            .route("/v1/memory/:id", get(super::get_by_id).delete(super::delete).patch(super::update))
            .route("/v1/memory/:id/pin", post(super::pin).delete(super::unpin))
            .route("/v1/memory/:id/unpin", post(super::unpin))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    fn setup_with_key() -> (SqliteStore, String) {
        let store = make_store();
        let raw_key = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            key
        };
        (store, raw_key)
    }

    /// Bootstraps the org and returns (store, admin_key, org_id).
    fn setup_org() -> (SqliteStore, String, String) {
        let store = make_store();
        let (admin_key, org_id) = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (org, _, key) = q::bootstrap(&conn, "Acme", "acme", "admin@acme.com", "Admin").unwrap();
            (key, org.id)
        };
        (store, admin_key, org_id)
    }

    /// Creates an extra user with the given role, returns their raw API key.
    fn create_test_user(store: &SqliteStore, org_id: &str, role: &str) -> String {
        use crate::auth::api_keys;
        use uuid::Uuid;
        let db = store.conn();
        let conn = db.lock().unwrap();
        let user_id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO users (id, org_id, email, name, role, status, created_at)
             VALUES (?1, ?2, ?3, 'Test', ?4, 'active', datetime('now'))",
            rusqlite::params![user_id, org_id, format!("{role}@test.com"), role],
        ).unwrap();
        let key_id = Uuid::new_v4().to_string();
        let (raw_key, key_hash) = api_keys::generate();
        conn.execute(
            "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
             VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
            rusqlite::params![key_id, user_id, org_id, key_hash],
        ).unwrap();
        raw_key
    }

    /// Stores a memory via admin key and returns its id.
    async fn seed_memory(store: &SqliteStore, admin_key: &str) -> String {
        let body = serde_json::json!({ "tool": "claude", "content": "seed content" });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        mem["id"].as_str().unwrap().to_string()
    }

    #[tokio::test]
    async fn store_memory_returns_201() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({ "tool": "claude", "content": "use snake_case" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn store_memory_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "tool": "claude", "content": "test" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn delete_memory_not_found_returns_404() {
        let (store, key) = setup_with_key();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/nonexistent-id")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── T4: store role gate ───────────────────────────────────────────────────

    #[tokio::test]
    async fn store_viewer_returns_403() {
        let (store, _, org_id) = setup_org();
        let viewer_key = create_test_user(&store, &org_id, "viewer");
        let body = serde_json::json!({ "tool": "claude", "content": "test" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn store_member_returns_201() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");
        let body = serde_json::json!({ "tool": "claude", "content": "member content" });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    // ── T5: delete role + ownership gate ─────────────────────────────────────

    #[tokio::test]
    async fn delete_viewer_returns_403() {
        let (store, admin_key, org_id) = setup_org();
        let viewer_key = create_test_user(&store, &org_id, "viewer");
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_member_own_memory_returns_204() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Member stores their own memory
        let body = serde_json::json!({ "tool": "claude", "content": "member memory" });
        let store_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(store_resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(store_resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mem_id = mem["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_member_other_memory_returns_403() {
        let (store, admin_key, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Admin stores a memory; member tries to delete it
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn delete_admin_other_memory_returns_204() {
        let (store, admin_key, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        // Member stores a memory; admin deletes it
        let body = serde_json::json!({ "tool": "claude", "content": "member content" });
        let store_resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(store_resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mem_id = mem["id"].as_str().unwrap().to_string();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn delete_nonexistent_memory_returns_404() {
        let (store, _, org_id) = setup_org();
        let member_key = create_test_user(&store, &org_id, "member");

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/does-not-exist")
                    .header("Authorization", format!("Bearer {member_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── T-05 tests ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn get_by_id_returns_200_for_own_memory() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mem["id"].as_str().unwrap(), mem_id);
        // project_id field must be present (may be null but the key must exist)
        assert!(mem.get("project_id").is_some(), "response must include project_id field");
    }

    #[tokio::test]
    async fn get_by_id_returns_404_for_unknown_id() {
        let (store, admin_key, _) = setup_org();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory/ghost-id-does-not-exist")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── T-01 memory export tests (RED phase) ─────────────────────────────────

    fn app_with_export(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/memory/store", post(super::store))
            .route("/v1/memory/export", get(super::export))
            .route("/v1/memory/:id", get(super::get_by_id).delete(super::delete))
            .route("/v1/memory", get(list))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn memory_export_csv_returns_200() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("text/csv"), "expected text/csv, got: {ct}");
    }

    #[tokio::test]
    async fn memory_export_csv_contains_header_row() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body = std::str::from_utf8(&bytes).unwrap();
        let first_line = body.lines().next().unwrap_or("");
        assert_eq!(
            first_line,
            "id,title,type,scope,project,tool,content,tags,topic_key,session_id,revision_count,pinned,created_at",
            "first line must be the CSV header row"
        );
    }

    #[tokio::test]
    async fn memory_export_json_returns_200() {
        let (store, admin_key) = setup_with_key();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=json")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"), "expected application/json, got: {ct}");
    }

    #[tokio::test]
    async fn memory_export_content_truncated() {
        let (store, admin_key) = setup_with_key();

        // Store a memory with content exceeding 500 chars
        let long_content = "a".repeat(600);
        let body = serde_json::json!({ "tool": "claude", "content": long_content });
        app_with_export(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app_with_export(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/export?format=csv")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body_str = std::str::from_utf8(&bytes).unwrap();
        // The data row content cell must be truncated (500 chars + "…" = 501+ char cell, not 600)
        // The CSV has: id,title,type,scope,project,tool,content,created_at
        // content cell must end with "…" (the ellipsis character)
        assert!(body_str.contains('…'), "truncated content must end with … character");
        // Full 600-char content must NOT appear verbatim
        assert!(!body_str.contains(&"a".repeat(501)), "content must be truncated at 500 chars");
    }

    #[tokio::test]
    async fn get_by_id_returns_404_for_other_org_memory() {
        // Org A stores a memory; org B tries to fetch it — must get 404, not 403.
        let store_a = make_store();
        let key_a = {
            let db = store_a.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgA", "orga", "admin@a.com", "AdminA").unwrap();
            key
        };

        let store_b = make_store();
        let key_b = {
            let db = store_b.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();
            key
        };

        // Org A stores a memory in their own store.
        let mem_id = seed_memory(&store_a, &key_a).await;

        // Org B tries to fetch org A's memory id via org B's store — must be 404.
        let resp = app(store_b)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {key_b}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    // ── bulk delete HTTP handler tests ────────────────────────────────────────

    #[tokio::test]
    async fn bulk_delete_admin_deletes_selected_memories() {
        let (store, admin_key, _) = setup_org();

        // Seed 3 memories
        let id1 = seed_memory(&store, &admin_key).await;
        let id2 = seed_memory(&store, &admin_key).await;
        let id3 = seed_memory(&store, &admin_key).await;

        // Bulk delete the first two
        let body = serde_json::json!({ "ids": [id1, id2] });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/bulk")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["deleted"], 2);

        // Verify id3 still exists via GET
        let get_resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{id3}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(get_resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn bulk_delete_empty_ids_returns_zero() {
        let (store, admin_key, _) = setup_org();
        let body = serde_json::json!({ "ids": [] });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/bulk")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let result: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(result["deleted"], 0);
    }

    #[tokio::test]
    async fn bulk_delete_viewer_returns_403() {
        let (store, admin_key, org_id) = setup_org();
        let viewer_key = create_test_user(&store, &org_id, "viewer");
        let id = seed_memory(&store, &admin_key).await;

        let body = serde_json::json!({ "ids": [id] });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/bulk")
                    .header("Authorization", format!("Bearer {viewer_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn bulk_delete_unauthenticated_returns_401() {
        let store = make_store();
        let body = serde_json::json!({ "ids": ["some-id"] });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri("/v1/memory/bulk")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    // ── PATCH /v1/memory/:id tests ────────────────────────────────────────────

    #[tokio::test]
    async fn update_memory_content_returns_200() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let body = serde_json::json!({ "content": "updated content" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mem["content"].as_str().unwrap(), "updated content");
        assert_eq!(mem["id"].as_str().unwrap(), mem_id);
    }

    #[tokio::test]
    async fn patch_increments_revision_count() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        // First PATCH: revision_count must go from 1 → 2
        let body = serde_json::json!({ "content": "v2" });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mem["revision_count"].as_i64().unwrap(), 2, "first PATCH must set revision_count to 2");

        // Second PATCH: revision_count must go from 2 → 3
        let body = serde_json::json!({ "content": "v3" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(mem["revision_count"].as_i64().unwrap(), 3, "second PATCH must set revision_count to 3");
    }

    #[tokio::test]
    async fn update_memory_wrong_org_returns_404() {
        let (store_a, key_a, _) = setup_org();
        let mem_id = seed_memory(&store_a, &key_a).await;

        // Org B gets its own store — memory belongs to org A
        let store_b = make_store();
        let key_b = {
            let db = store_b.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::bootstrap(&conn, "OrgB", "orgb", "admin@b.com", "AdminB").unwrap();
            key
        };

        let body = serde_json::json!({ "content": "should not update" });
        let resp = app(store_b)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {key_b}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn update_memory_title_only_returns_200() {
        let (store, admin_key, _) = setup_org();

        // Seed memory with a known title
        let body = serde_json::json!({ "tool": "claude", "content": "original content", "title": "original title" });
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let mem_id = mem["id"].as_str().unwrap().to_string();

        // PATCH with title only — content must be preserved
        let patch = serde_json::json!({ "title": "new title" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let updated: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(updated["title"].as_str().unwrap(), "new title", "title must be updated");
        assert_eq!(updated["content"].as_str().unwrap(), "original content", "content must be unchanged");
    }

    #[tokio::test]
    async fn update_memory_both_fields_returns_200() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let patch = serde_json::json!({ "content": "new content", "title": "new title" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let updated: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(updated["content"].as_str().unwrap(), "new content");
        assert_eq!(updated["title"].as_str().unwrap(), "new title");
    }

    #[tokio::test]
    async fn update_memory_no_fields_returns_400_json() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let patch = serde_json::json!({});
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"), "error must be JSON, got: {ct}");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(err["error"].as_str().is_some(), "error field must be present");
    }

    #[tokio::test]
    async fn update_memory_title_only_response_is_json() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        // This is Case 2 from the issue — previously returned 422 text/plain
        let patch = serde_json::json!({ "title": "only title no content" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(patch.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Must succeed (200) with JSON body, NOT 422 text/plain
        assert_eq!(resp.status(), StatusCode::OK);
        let ct = resp.headers().get("content-type").unwrap().to_str().unwrap();
        assert!(ct.contains("application/json"), "response must be JSON, got: {ct}");
    }

    // ── POST /v1/memory/:id/pin + /unpin tests ────────────────────────────────

    #[tokio::test]
    async fn pin_sets_pinned_true() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        // Pin it
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/memory/{mem_id}/pin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify pinned = true via GET
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(mem["pinned"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn unpin_sets_pinned_false() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        // Pin then unpin
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/memory/{mem_id}/pin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/memory/{mem_id}/unpin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify pinned = false via GET
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(!mem["pinned"].as_bool().unwrap());
    }

    // ── Malformed body consistency tests ─────────────────────────────────────

    #[tokio::test]
    async fn store_empty_body_returns_422() {
        let (store, key) = setup_with_key();
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn store_null_body_returns_422() {
        let (store, key) = setup_with_key();
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from("null"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn store_array_body_returns_422() {
        let (store, key) = setup_with_key();
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(r#"[{"content":"test"}]"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[tokio::test]
    async fn delete_pin_sets_pinned_false() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        // Pin first
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/memory/{mem_id}/pin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Unpin via DELETE /v1/memory/:id/pin
        let resp = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/v1/memory/{mem_id}/pin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Verify pinned = false
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(!mem["pinned"].as_bool().unwrap());
    }

    // ── search permission tests ───────────────────────────────────────────────

    /// A custom role with only memory:read should be able to call POST /v1/memory/search.
    #[tokio::test]
    async fn search_custom_readonly_role_returns_results() {
        let (store, admin_key, org_id) = setup_org();

        // Create a custom role with only memory:read (no memory:search)
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::create_role(
                &conn,
                &org_id,
                "readonly",
                "Read Only",
                &["memory:read".to_string()],
                None,
            ).unwrap();
        }

        // Create a user with the custom role
        let readonly_key = {
            use crate::auth::api_keys;
            use uuid::Uuid;
            let db = store.conn();
            let conn = db.lock().unwrap();
            let user_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO users (id, org_id, email, name, role, status, created_at)
                 VALUES (?1, ?2, 'readonly@test.com', 'ReadOnly', 'readonly', 'active', datetime('now'))",
                rusqlite::params![user_id, org_id],
            ).unwrap();
            let key_id = Uuid::new_v4().to_string();
            let (raw_key, key_hash) = api_keys::generate();
            conn.execute(
                "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
                rusqlite::params![key_id, user_id, org_id, key_hash],
            ).unwrap();
            raw_key
        };

        // Admin stores a memory with a unique term
        let body = serde_json::json!({ "tool": "claude", "content": "nexusmind_unique_searchable_term" });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Custom readonly role must be able to search (not 403)
        let search_body = serde_json::json!({ "query": "nexusmind_unique_searchable_term" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {readonly_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "custom memory:read role must be able to search");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let results: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let arr = results["memories"].as_array().expect("search returns MemoryPage with memories array");
        assert!(!arr.is_empty(), "search must return the seeded memory");
    }

    /// A user with no permissions at all must get 403 on search.
    #[tokio::test]
    async fn search_no_permissions_returns_403() {
        let (store, _, org_id) = setup_org();

        // Create a custom role with no permissions
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            q::create_role(
                &conn,
                &org_id,
                "noperms",
                "No Perms",
                &[],
                None,
            ).unwrap();
        }

        let noperms_key = {
            use crate::auth::api_keys;
            use uuid::Uuid;
            let db = store.conn();
            let conn = db.lock().unwrap();
            let user_id = Uuid::new_v4().to_string();
            conn.execute(
                "INSERT INTO users (id, org_id, email, name, role, status, created_at)
                 VALUES (?1, ?2, 'noperms@test.com', 'NoPerms', 'noperms', 'active', datetime('now'))",
                rusqlite::params![user_id, org_id],
            ).unwrap();
            let key_id = Uuid::new_v4().to_string();
            let (raw_key, key_hash) = api_keys::generate();
            conn.execute(
                "INSERT INTO api_keys (id, user_id, org_id, key_hash, label, created_at)
                 VALUES (?1, ?2, ?3, ?4, 'default', datetime('now'))",
                rusqlite::params![key_id, user_id, org_id, key_hash],
            ).unwrap();
            raw_key
        };

        let search_body = serde_json::json!({ "query": "test" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {noperms_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // ── degraded search fallback tests ────────────────────────────────────────
    // The test store never attaches an embed service (SqliteStore::new has no
    // `.with_embed(..)`), so `semantic`/`hybrid` modes always fall back to
    // keyword search here — exactly the condition we want to surface.

    #[tokio::test]
    async fn search_hybrid_without_embed_service_reports_degraded() {
        let (store, admin_key, _) = setup_org();
        seed_memory(&store, &admin_key).await;

        let search_body = serde_json::json!({ "query": "seed", "mode": "hybrid" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(
            body["degraded"].as_str(),
            Some("keyword-fallback"),
            "hybrid mode without an embed service must report degraded: keyword-fallback, got: {body}"
        );
    }

    #[tokio::test]
    async fn search_semantic_without_embed_service_reports_degraded() {
        let (store, admin_key, _) = setup_org();
        seed_memory(&store, &admin_key).await;

        let search_body = serde_json::json!({ "query": "seed", "mode": "semantic" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["degraded"].as_str(), Some("keyword-fallback"));
    }

    #[tokio::test]
    async fn search_keyword_mode_never_reports_degraded() {
        let (store, admin_key, _) = setup_org();
        seed_memory(&store, &admin_key).await;

        let search_body = serde_json::json!({ "query": "seed", "mode": "keyword" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert!(
            body.get("degraded").is_none() || body["degraded"].is_null(),
            "keyword mode must never report degraded, got: {body}"
        );
    }

    #[tokio::test]
    async fn list_returns_pinned_first() {
        let (store, admin_key, _) = setup_org();

        // Store two memories — first one will be pinned
        let body1 = serde_json::json!({ "tool": "claude", "content": "unpinned memory" });
        let body2 = serde_json::json!({ "tool": "claude", "content": "pinned memory" });

        let r1 = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body1.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r1.into_body(), usize::MAX).await.unwrap();
        let m1: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id1 = m1["id"].as_str().unwrap().to_string();

        let r2 = app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body2.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(r2.into_body(), usize::MAX).await.unwrap();
        let m2: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id2 = m2["id"].as_str().unwrap().to_string();

        // Pin id1 (created first, so normally would appear after id2)
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/v1/memory/{id1}/pin"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // List — pinned memory must be first
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory?limit=10")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memories = page["memories"].as_array().unwrap();
        assert!(!memories.is_empty(), "should have memories");
        assert_eq!(memories[0]["id"].as_str().unwrap(), id1, "pinned memory must be first");
        assert_eq!(memories[1]["id"].as_str().unwrap(), id2, "unpinned memory must follow");
        assert!(page["total"].as_i64().unwrap() >= 2, "total must be present and >= 2");
    }

    // ── compact response shape tests ──────────────────────────────────────────

    const COMPACT_FIELDS: &[&str] = &[
        "id", "title", "type", "project", "tags", "pinned", "created_at", "preview",
    ];

    #[tokio::test]
    async fn list_compact_true_returns_preview_shape() {
        let (store, admin_key, _) = setup_org();
        let body = serde_json::json!({ "tool": "claude", "content": "a".repeat(300) });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory?compact=true")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memories = page["memories"].as_array().unwrap();
        assert_eq!(memories.len(), 1);
        let item = &memories[0];
        let obj = item.as_object().unwrap();
        for field in COMPACT_FIELDS {
            assert!(obj.contains_key(*field), "compact item must contain '{field}', got: {item}");
        }
        assert!(obj.get("content").is_none(), "compact item must not include full content");
        let preview = item["preview"].as_str().unwrap();
        assert_eq!(preview.chars().count(), 200, "preview must be the first 200 chars of content");
    }

    #[tokio::test]
    async fn list_without_compact_returns_full_shape() {
        let (store, admin_key, _) = setup_org();
        seed_memory(&store, &admin_key).await;

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memories = page["memories"].as_array().unwrap();
        assert!(!memories.is_empty());
        assert!(
            memories[0].as_object().unwrap().contains_key("content"),
            "default (non-compact) shape must still include full content"
        );
        assert!(
            memories[0].as_object().unwrap().get("preview").is_none(),
            "default (non-compact) shape must not include a preview field"
        );
    }

    #[tokio::test]
    async fn search_compact_true_returns_preview_shape() {
        let (store, admin_key, _) = setup_org();
        let content = format!("findme_compact_marker {}", "b".repeat(300));
        let body = serde_json::json!({ "tool": "claude", "content": content });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let search_body = serde_json::json!({ "query": "findme_compact_marker", "mode": "keyword", "compact": true });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memories = page["memories"].as_array().unwrap();
        assert_eq!(memories.len(), 1);
        let obj = memories[0].as_object().unwrap();
        for field in COMPACT_FIELDS {
            assert!(obj.contains_key(*field), "compact search item must contain '{field}'");
        }
        assert!(obj.get("content").is_none());
    }

    #[tokio::test]
    async fn search_without_compact_returns_full_shape() {
        let (store, admin_key, _) = setup_org();
        seed_memory(&store, &admin_key).await;

        let search_body = serde_json::json!({ "query": "seed", "mode": "keyword" });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/search")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(search_body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memories = page["memories"].as_array().unwrap();
        assert!(!memories.is_empty());
        assert!(memories[0].as_object().unwrap().contains_key("content"));
    }

    #[tokio::test]
    async fn compact_preview_does_not_panic_on_multibyte_utf8_boundary() {
        let (store, admin_key, _) = setup_org();
        // 250 multi-byte characters ('é' is 2 bytes in UTF-8) — a naive byte-slice
        // truncation at byte offset 200 would land mid-codepoint and panic.
        let content: String = std::iter::repeat('é').take(250).collect();
        let body = serde_json::json!({ "tool": "claude", "content": content });
        app(store.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/v1/memory?compact=true")
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK, "must not panic on multi-byte UTF-8 truncation");
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let page: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let preview = page["memories"][0]["preview"]
            .as_str()
            .expect("compact preview field must be present and a string");
        assert_eq!(preview.chars().count(), 200);
    }

    // ── Content size limit tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn store_oversized_content_returns_413() {
        let (store, key) = setup_with_key();
        let oversized = "A".repeat(super::MAX_CONTENT_BYTES + 1);
        let body = serde_json::json!({ "tool": "claude", "content": oversized });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["code"].as_str().unwrap(), "content_too_large");
    }

    #[tokio::test]
    async fn store_max_content_exactly_at_limit_returns_201() {
        let (store, key) = setup_with_key();
        let exact = "A".repeat(super::MAX_CONTENT_BYTES);
        let body = serde_json::json!({ "tool": "claude", "content": exact });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn update_oversized_content_returns_413() {
        let (store, admin_key, _) = setup_org();
        let mem_id = seed_memory(&store, &admin_key).await;

        let oversized = "B".repeat(super::MAX_CONTENT_BYTES + 1);
        let body = serde_json::json!({ "content": oversized });
        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/v1/memory/{mem_id}"))
                    .header("Authorization", format!("Bearer {admin_key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["code"].as_str().unwrap(), "content_too_large");
    }

    // ── Tag validation tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn store_filters_empty_and_whitespace_tags() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({
            "tool": "claude",
            "content": "tag test",
            "tags": ["valid-tag", "", "  ", "\t"]
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tags = mem["tags"].as_array().unwrap();
        assert_eq!(tags.len(), 1, "empty/whitespace tags must be filtered out");
        assert_eq!(tags[0].as_str().unwrap(), "valid-tag");
    }

    #[tokio::test]
    async fn store_trims_whitespace_from_tags() {
        let (store, key) = setup_with_key();
        let body = serde_json::json!({
            "tool": "claude",
            "content": "trim test",
            "tags": ["  leading", "trailing  ", "  both  "]
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CREATED);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let mem: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let tags: Vec<&str> = mem["tags"].as_array().unwrap()
            .iter().map(|v| v.as_str().unwrap()).collect();
        assert_eq!(tags, vec!["leading", "trailing", "both"]);
    }

    #[tokio::test]
    async fn store_rejects_tag_exceeding_max_length() {
        let (store, key) = setup_with_key();
        let long_tag = "a".repeat(super::MAX_TAG_LENGTH + 1);
        let body = serde_json::json!({
            "tool": "claude",
            "content": "long tag test",
            "tags": [long_tag]
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["code"].as_str().unwrap(), "validation_error");
    }

    #[tokio::test]
    async fn store_rejects_too_many_tags() {
        let (store, key) = setup_with_key();
        let tags: Vec<String> = (0..=super::MAX_TAGS).map(|i| format!("tag-{i}")).collect();
        let body = serde_json::json!({
            "tool": "claude",
            "content": "too many tags",
            "tags": tags
        });

        let resp = app(store)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/memory/store")
                    .header("Authorization", format!("Bearer {key}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["code"].as_str().unwrap(), "validation_error");
    }

    // ── GET /v1/memory/graph ────────────────────────────────────────────────

    fn graph_app(store: SqliteStore) -> Router {
        Router::new()
            .route("/v1/memory/graph", get(super::get_graph))
            .layer(middleware::from_fn_with_state(store.conn(), auth_mw::auth))
            .layer(tower_cookies::CookieManagerLayer::new())
            .with_state(store)
    }

    #[tokio::test]
    async fn get_memory_graph_unauthenticated_returns_401() {
        let store = make_store();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn get_memory_graph_missing_project_returns_422() {
        let (store, key) = setup_with_key();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let err: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(err["code"].as_str().unwrap(), "validation_error");
    }

    #[tokio::test]
    async fn get_memory_graph_empty_project_returns_200_with_envelope() {
        let (store, key) = setup_with_key();

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=nonexistent")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["project"], "nonexistent");
        assert_eq!(body["node_count"], 0);
        assert_eq!(body["edge_count"], 0);
        assert!(body["nodes"].as_array().unwrap().is_empty());
        assert!(body["edges"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn get_memory_graph_returns_envelope_with_seeded_memory() {
        let (store, key) = setup_with_key();
        seed_memory(&store, &key).await;

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=default")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let node_count = body["node_count"].as_u64().unwrap();
        assert!(node_count >= 1, "must include at least the Memory node, got {node_count}");
        assert_eq!(
            node_count as usize,
            body["nodes"].as_array().unwrap().len(),
            "node_count must equal nodes.length()"
        );
        assert_eq!(
            body["edge_count"].as_u64().unwrap() as usize,
            body["edges"].as_array().unwrap().len(),
            "edge_count must equal edges.length()"
        );

        // No dangling edges — every edge references a node in the returned set.
        let node_ids: std::collections::HashSet<String> = body["nodes"]
            .as_array().unwrap().iter()
            .map(|n| n["id"].as_str().unwrap().to_string())
            .collect();
        for edge in body["edges"].as_array().unwrap() {
            assert!(node_ids.contains(edge["from_id"].as_str().unwrap()));
            assert!(node_ids.contains(edge["to_id"].as_str().unwrap()));
        }
    }

    #[tokio::test]
    async fn get_memory_graph_cross_org_never_leaks() {
        let (store, admin_key, _org_a) = setup_org();
        seed_memory(&store, &admin_key).await;

        // Create a second org on the SAME store/db to prove org isolation.
        // (`bootstrap` only allows the first org per DB; `create_org` has no such guard.)
        let key_b = {
            let db = store.conn();
            let conn = db.lock().unwrap();
            let (_, _, key) = q::create_org(&conn, "OrgB", "orgb", "admin@orgb.com", "Admin").unwrap();
            key
        };

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=default")
                    .header("Authorization", format!("Bearer {key_b}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(body["node_count"], 0, "org B must not see org A's memories");
    }

    #[tokio::test]
    async fn get_memory_graph_since_narrows_results() {
        let (store, key) = setup_with_key();
        let old_id = seed_memory(&store, &key).await;
        seed_memory(&store, &key).await;

        // Push the first memory's created_at into the past so `since` can exclude it.
        {
            let db = store.conn();
            let conn = db.lock().unwrap();
            conn.execute(
                "UPDATE memories SET created_at = '2020-01-01T00:00:00Z' WHERE id = ?1",
                rusqlite::params![old_id],
            ).unwrap();
        }

        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=default&since=2025-01-01T00:00:00Z")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let memory_nodes: Vec<_> = body["nodes"].as_array().unwrap().iter()
            .filter(|n| n["type"] == "Memory")
            .collect();
        assert_eq!(memory_nodes.len(), 1, "only the recent memory must be included");
    }

    #[tokio::test]
    async fn get_memory_graph_limit_capped_at_10000() {
        let (store, key) = setup_with_key();

        // Request limit=999999 — should be silently capped to 10000, never error.
        let resp = graph_app(store)
            .oneshot(
                Request::builder()
                    .uri("/v1/memory/graph?project=default&limit=999999")
                    .header("Authorization", format!("Bearer {key}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
    }
}
