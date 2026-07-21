use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::backup::client::{
    fetch_full_backup, get_backup, list_backups, list_tables_for_backup, BackupRow,
    BackupTableRow,
};
use crate::backup::job::{run_backup, BackupResult};
use crate::backup::restore::{fetch_restore_payload, restore_from_dump, RestoreSummary};
use crate::models::types::{ApiError, AuthContext};
use crate::store::sqlite::SqliteStore;

const DEFAULT_LIST_LIMIT: i64 = 50;
const MAX_LIST_LIMIT: i64 = 200;

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
            error: "Backup not found".to_string(),
            code: "not_found".to_string(),
        }),
    )
}

fn internal(e: impl std::fmt::Display) -> (StatusCode, Json<ApiError>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ApiError {
            error: e.to_string(),
            code: "internal_error".to_string(),
        }),
    )
}

// ── Handlers ────────────────────────────────────────────────────────────────

#[derive(Deserialize, Default)]
pub struct ListBackupsParams {
    pub limit: Option<i64>,
}

/// `GET /v1/backups` — list all backup metadata for the caller's org.
/// Admin-only.
pub async fn list_backups_handler(
    Extension(pool): Extension<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListBackupsParams>,
) -> Result<Json<Vec<BackupRow>>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_super_user() {
        return Err(forbidden());
    }
    let limit = params.limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, MAX_LIST_LIMIT);
    let rows = list_backups(&pool, &auth.org_id, limit).await.map_err(internal)?;
    Ok(Json(rows))
}

#[derive(Serialize)]
pub struct BackupDetail {
    #[serde(flatten)]
    pub backup:     BackupRow,
    pub table_list: Vec<TableInfo>,
}

#[derive(Serialize)]
pub struct TableInfo {
    pub table_name: String,
    pub row_count:  i32,
}

/// `GET /v1/backups/:id` — get backup metadata + table list (no row data).
/// Admin-only.
pub async fn get_backup_handler(
    Extension(pool): Extension<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackupDetail>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_super_user() {
        return Err(forbidden());
    }
    let backup = get_backup(&pool, &auth.org_id, id)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;

    let tables = list_tables_for_backup(&pool, id)
        .await
        .map_err(internal)?
        .into_iter()
        .map(|(table_name, row_count)| TableInfo { table_name, row_count })
        .collect();

    Ok(Json(BackupDetail {
        backup,
        table_list: tables,
    }))
}

/// `POST /v1/backups` — trigger a new manual backup. Runs the full
/// `run_backup` flow synchronously and returns the result. Admin-only.
pub async fn create_backup_handler(
    Extension(pool): Extension<PgPool>,
    Extension(auth): Extension<AuthContext>,
    State(store): State<SqliteStore>,
) -> Result<Json<BackupResult>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_super_user() {
        return Err(forbidden());
    }
    let sqlite = store.conn();
    let result = run_backup(&pool, sqlite, &auth.org_id, "manual")
        .await
        .map_err(internal)?;
    Ok(Json(result))
}

#[derive(Deserialize, Default)]
pub struct RestoreParams {
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Serialize)]
pub struct RestoreResponse {
    pub backup_id: Uuid,
    pub summary:   RestoreSummary,
}

/// `POST /v1/backups/:id/restore?confirm=true` — DESTRUCTIVE: drops all rows
/// from every restorable SQLite table and re-inserts them from the backup.
/// Both the `?confirm=true` query param AND a `X-Confirm-Restore: yes` header
/// are required so that a misclick / scripted accident can't wipe production.
/// Admin-only.
pub async fn restore_backup_handler(
    Extension(pool): Extension<PgPool>,
    Extension(auth): Extension<AuthContext>,
    State(store): State<SqliteStore>,
    Path(id): Path<Uuid>,
    Query(params): Query<RestoreParams>,
    headers: HeaderMap,
) -> Result<Json<RestoreResponse>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_super_user() {
        return Err(forbidden());
    }

    // Two-step confirmation: query param + header. Either missing → 422.
    if !params.confirm {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Restore is destructive: pass ?confirm=true to proceed"
                    .to_string(),
                code: "confirmation_required".to_string(),
            }),
        ));
    }
    let header = headers
        .get("X-Confirm-Restore")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if header != "yes" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: "Restore is destructive: set header `X-Confirm-Restore: yes`"
                    .to_string(),
                code: "confirmation_required".to_string(),
            }),
        ));
    }

    // The backup must exist AND belong to the caller's org.
    let backup = get_backup(&pool, &auth.org_id, id)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;
    if backup.status != "complete" {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ApiError {
                error: format!("Cannot restore from a '{}' backup", backup.status),
                code: "invalid_backup_state".to_string(),
            }),
        ));
    }

    // Fetch the rows from Postgres off the SQLite lock.
    let payload = fetch_restore_payload(&pool, id)
        .await
        .map_err(internal)?;

    // Apply on the SQLite connection. Restore runs synchronously inside a
    // transaction; we hold the lock for the duration. Long restores will
    // block other writers, which is the correct trade-off for a destructive
    // operation.
    let sqlite = store.conn();
    let mut conn = match sqlite.lock() {
        Ok(c) => c,
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiError {
                    error: "Database lock error".to_string(),
                    code: "internal_error".to_string(),
                }),
            ));
        }
    };
    let summary = restore_from_dump(&mut conn, &payload).map_err(internal)?;

    Ok(Json(RestoreResponse {
        backup_id: id,
        summary,
    }))
}

#[derive(Serialize)]
pub struct BackupDownload {
    pub backup_id: Uuid,
    pub org_id:    String,
    pub tables:    Vec<BackupTableRow>,
}

/// `GET /v1/backups/:id/download` — return the full backup as a single JSON
/// document. Each table is included with its full row payload. Admin-only.
pub async fn download_backup_handler(
    Extension(pool): Extension<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> Result<Json<BackupDownload>, (StatusCode, Json<ApiError>)> {
    if !auth.role.is_super_user() {
        return Err(forbidden());
    }
    let backup = get_backup(&pool, &auth.org_id, id)
        .await
        .map_err(internal)?
        .ok_or_else(not_found)?;

    let tables = fetch_full_backup(&pool, id).await.map_err(internal)?;

    Ok(Json(BackupDownload {
        backup_id: id,
        org_id: backup.org_id,
        tables,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        db::{connection::connect, migrations},
        models::types::{Role, UserRole},
    };
    use axum::extract::{Path, Query, State};
    use sqlx::postgres::PgPoolOptions;

    fn auth(role: &str) -> AuthContext {
        AuthContext {
            org_id: "org".to_string(),
            user_id: "user".to_string(),
            role: if role == "admin" {
                UserRole::Standard(Role::Admin)
            } else {
                UserRole::Custom(role.to_string())
            },
        }
    }

    fn store() -> SqliteStore {
        let conn = connect(":memory:").unwrap();
        migrations::run(&conn).unwrap();
        SqliteStore::new(conn)
    }

    fn pool() -> PgPool {
        PgPoolOptions::new()
            .connect_lazy("postgres://localhost/nexusmind")
            .unwrap()
    }

    fn error<T>(result: Result<T, (StatusCode, Json<ApiError>)>) -> (StatusCode, Json<ApiError>) {
        match result {
            Err(error) => error,
            Ok(_) => panic!("expected handler to return an error"),
        }
    }

    #[tokio::test]
    async fn backup_endpoints_deny_admin_and_allow_super_user_past_role_gate() {
        let backup_id = Uuid::new_v4();
        let headers = HeaderMap::new();

        let list_admin = error(list_backups_handler(
            Extension(pool()), Extension(auth("admin")), Query(ListBackupsParams::default()),
        ).await);
        assert_eq!(list_admin.0, StatusCode::FORBIDDEN);
        let list_super = error(list_backups_handler(
            Extension(pool()), Extension(auth("super_user")), Query(ListBackupsParams::default()),
        ).await);
        assert_ne!(list_super.0, StatusCode::FORBIDDEN);

        let get_admin = error(get_backup_handler(Extension(pool()), Extension(auth("admin")), Path(backup_id)).await);
        assert_eq!(get_admin.0, StatusCode::FORBIDDEN);
        let get_super = error(get_backup_handler(Extension(pool()), Extension(auth("super_user")), Path(backup_id)).await);
        assert_ne!(get_super.0, StatusCode::FORBIDDEN);

        let create_admin = error(create_backup_handler(Extension(pool()), Extension(auth("admin")), State(store())).await);
        assert_eq!(create_admin.0, StatusCode::FORBIDDEN);
        let create_super = error(create_backup_handler(Extension(pool()), Extension(auth("super_user")), State(store())).await);
        assert_ne!(create_super.0, StatusCode::FORBIDDEN);

        let restore_admin = error(restore_backup_handler(
            Extension(pool()), Extension(auth("admin")), State(store()), Path(backup_id), Query(RestoreParams::default()), headers.clone(),
        ).await);
        assert_eq!(restore_admin.0, StatusCode::FORBIDDEN);
        let restore_super = error(restore_backup_handler(
            Extension(pool()), Extension(auth("super_user")), State(store()), Path(backup_id), Query(RestoreParams::default()), headers,
        ).await);
        assert_eq!(restore_super.0, StatusCode::UNPROCESSABLE_ENTITY);

        let download_admin = error(download_backup_handler(Extension(pool()), Extension(auth("admin")), Path(backup_id)).await);
        assert_eq!(download_admin.0, StatusCode::FORBIDDEN);
        let download_super = error(download_backup_handler(Extension(pool()), Extension(auth("super_user")), Path(backup_id)).await);
        assert_ne!(download_super.0, StatusCode::FORBIDDEN);
    }
}
