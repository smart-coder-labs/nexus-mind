use crate::backup::serializer::TableDump;
use anyhow::{Context, Result};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;
use std::str::FromStr;
use std::time::Duration;

pub const SCHEMA_SQL: &str = include_str!("schema.sql");

/// Build a connection pool for the backup Postgres instance.
///
/// `url` is the `BACKUP_DATABASE_URL` env var.
pub async fn connect_pool(url: &str) -> Result<PgPool> {
    let opts = PgConnectOptions::from_str(url)
        .context("parsing BACKUP_DATABASE_URL")?
        .application_name("nexusmind-backup");

    let pool = PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_with(opts)
        .await
        .context("connecting to BACKUP_DATABASE_URL")?;

    Ok(pool)
}

/// Apply the embedded schema. Idempotent — uses `CREATE TABLE IF NOT EXISTS` /
/// `CREATE INDEX IF NOT EXISTS`. Safe to call on every boot.
pub async fn ensure_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(SCHEMA_SQL)
        .execute(pool)
        .await
        .context("applying backup schema")?;
    Ok(())
}

/// A row in the `backups` metadata table — what we return from list/get endpoints.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct BackupRow {
    pub id:         uuid::Uuid,
    pub org_id:     String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub kind:       String,
    pub status:     String,
    pub size_bytes: Option<i64>,
    pub error:      Option<String>,
    pub metadata:   Option<serde_json::Value>,
}

/// A row in the `backup_tables` table — one per table per backup.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct BackupTableRow {
    pub backup_id:  uuid::Uuid,
    pub table_name: String,
    pub row_count:  i32,
    pub data:       serde_json::Value,
}

/// Insert a `pending` backup row, return its id.
pub async fn insert_pending_backup(
    pool: &PgPool,
    org_id: &str,
    kind: &str,
) -> Result<uuid::Uuid> {
    let id: (uuid::Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO backups (org_id, kind, status)
        VALUES ($1, $2, 'pending')
        RETURNING id
        "#,
    )
    .bind(org_id)
    .bind(kind)
    .fetch_one(pool)
    .await
    .context("inserting pending backup")?;
    Ok(id.0)
}

/// Mark a backup as complete with size_bytes and metadata.
pub async fn mark_backup_complete(
    pool: &PgPool,
    backup_id: uuid::Uuid,
    size_bytes: i64,
    metadata: &serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE backups
        SET status = 'complete',
            size_bytes = $2,
            metadata = $3,
            error = NULL
        WHERE id = $1
        "#,
    )
    .bind(backup_id)
    .bind(size_bytes)
    .bind(metadata)
    .execute(pool)
    .await
    .context("marking backup complete")?;
    Ok(())
}

/// Mark a backup as failed with an error message. Metadata is left untouched
/// (typically NULL for failed backups).
pub async fn mark_backup_failed(
    pool: &PgPool,
    backup_id: uuid::Uuid,
    error: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE backups
        SET status = 'failed',
            error = $2
        WHERE id = $1
        "#,
    )
    .bind(backup_id)
    .bind(error)
    .execute(pool)
    .await
    .context("marking backup failed")?;
    Ok(())
}

/// Insert a `backup_tables` row for a single table dump.
pub async fn insert_table_dump(
    pool: &PgPool,
    backup_id: uuid::Uuid,
    dump: &TableDump,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO backup_tables (backup_id, table_name, row_count, data)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (backup_id, table_name)
        DO UPDATE SET row_count = EXCLUDED.row_count,
                      data = EXCLUDED.data
        "#,
    )
    .bind(backup_id)
    .bind(&dump.table_name)
    .bind(dump.row_count as i32)
    .bind(&dump.rows)
    .execute(pool)
    .await
    .context("inserting table dump")?;
    Ok(())
}

/// List all backup metadata for an org, newest first. Excludes `backup_tables`
/// rows (use `list_tables_for_backup` for that).
pub async fn list_backups(
    pool: &PgPool,
    org_id: &str,
    limit: i64,
) -> Result<Vec<BackupRow>> {
    let rows = sqlx::query_as::<_, BackupRow>(
        r#"
        SELECT id, org_id, created_at, kind, status, size_bytes, error, metadata
        FROM backups
        WHERE org_id = $1
        ORDER BY created_at DESC
        LIMIT $2
        "#,
    )
    .bind(org_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("listing backups")?;
    Ok(rows)
}

/// Fetch a single backup's metadata.
pub async fn get_backup(
    pool: &PgPool,
    org_id: &str,
    backup_id: uuid::Uuid,
) -> Result<Option<BackupRow>> {
    let row = sqlx::query_as::<_, BackupRow>(
        r#"
        SELECT id, org_id, created_at, kind, status, size_bytes, error, metadata
        FROM backups
        WHERE id = $1 AND org_id = $2
        "#,
    )
    .bind(backup_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await
    .context("getting backup")?;
    Ok(row)
}

/// Fetch all `backup_tables` rows for a backup — table name + row count, no
/// data. Used by the get-by-id endpoint.
pub async fn list_tables_for_backup(
    pool: &PgPool,
    backup_id: uuid::Uuid,
) -> Result<Vec<(String, i32)>> {
    let rows: Vec<(String, i32)> = sqlx::query_as(
        r#"
        SELECT table_name, row_count
        FROM backup_tables
        WHERE backup_id = $1
        ORDER BY table_name
        "#,
    )
    .bind(backup_id)
    .fetch_all(pool)
    .await
    .context("listing tables for backup")?;
    Ok(rows)
}

/// Fetch all `backup_tables` rows for a backup — full data. Used by the
/// download endpoint.
pub async fn fetch_full_backup(
    pool: &PgPool,
    backup_id: uuid::Uuid,
) -> Result<Vec<BackupTableRow>> {
    let rows = sqlx::query_as::<_, BackupTableRow>(
        r#"
        SELECT backup_id, table_name, row_count, data
        FROM backup_tables
        WHERE backup_id = $1
        ORDER BY table_name
        "#,
    )
    .bind(backup_id)
    .fetch_all(pool)
    .await
    .context("fetching full backup")?;
    Ok(rows)
}

/// Fetch the `data` column for one table from a backup.
pub async fn fetch_table_dump(
    pool: &PgPool,
    backup_id: uuid::Uuid,
    table_name: &str,
) -> Result<Option<serde_json::Value>> {
    let row: Option<(serde_json::Value,)> = sqlx::query_as(
        r#"
        SELECT data
        FROM backup_tables
        WHERE backup_id = $1 AND table_name = $2
        "#,
    )
    .bind(backup_id)
    .bind(table_name)
    .fetch_optional(pool)
    .await
    .context("fetching table dump")?;
    Ok(row.map(|(d,)| d))
}
