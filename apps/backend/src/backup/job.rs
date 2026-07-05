use crate::backup::client::{
    ensure_schema, insert_pending_backup, insert_table_dump, mark_backup_complete,
    mark_backup_failed,
};
use crate::backup::serializer::{approx_dump_size, dump_table, BACKUP_TABLES};
use anyhow::{Context, Result};
use rusqlite::Connection;
use sqlx::PgPool;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Result of a successful backup run. Returned to API handlers and to the
/// background job.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BackupResult {
    pub backup_id:    uuid::Uuid,
    pub size_bytes:   i64,
    pub table_count:  i64,
    pub total_rows:   i64,
    pub duration_ms:  i64,
}

/// Run a full backup of `conn` (the live SQLite store) into the Postgres
/// `pool`. The full lifecycle is:
///   1. insert pending row → get id
///   2. dump every whitelisted table
///   3. insert each table's data in `backup_tables`
///   4. mark complete with total size + metadata
///   5. on any error, mark failed with the message
pub async fn run_backup(
    pool: &PgPool,
    sqlite: Arc<Mutex<Connection>>,
    org_id: &str,
    kind: &str,
) -> Result<BackupResult> {
    let started = std::time::Instant::now();

    // 1. Insert pending
    let backup_id = insert_pending_backup(pool, org_id, kind)
        .await
        .context("insert pending backup")?;

    let result = do_backup(pool, sqlite, org_id, backup_id).await;

    let duration_ms = started.elapsed().as_millis() as i64;

    match result {
        Ok((size, tables, rows)) => {
            let metadata = serde_json::json!({
                "table_count": tables,
                "total_rows": rows,
                "duration_ms": duration_ms,
            });
            mark_backup_complete(pool, backup_id, size, &metadata)
                .await
                .context("mark complete")?;
            Ok(BackupResult {
                backup_id,
                size_bytes: size,
                table_count: tables,
                total_rows: rows,
                duration_ms,
            })
        }
        Err(e) => {
            let msg = format!("{e:#}");
            // Don't propagate the error — we already recorded the failure
            // in the `backups` table. The caller still needs to know
            // something went wrong, so we return a structured error.
            let _ = mark_backup_failed(pool, backup_id, &msg).await;
            Err(e.context(format!("backup {backup_id} failed; recorded in Postgres")))
        }
    }
}

async fn do_backup(
    pool: &PgPool,
    sqlite: Arc<Mutex<Connection>>,
    org_id: &str,
    backup_id: uuid::Uuid,
) -> Result<(i64, i64, i64)> {
    // Dump tables on a blocking thread — rusqlite is sync and the dump
    // can take a while for large tables.
    let org_id_owned = org_id.to_string();
    let dumps = tokio::task::spawn_blocking(move || -> Result<Vec<_>> {
        let conn = sqlite.lock().map_err(|e| anyhow::anyhow!("sqlite lock poisoned: {e}"))?;
        let mut out = Vec::with_capacity(BACKUP_TABLES.len());
        for table in BACKUP_TABLES {
            match dump_table(&conn, table) {
                Ok(d) => out.push(d),
                Err(e) => {
                    // A single broken table should fail the whole backup —
                    // partial backups are worse than no backup.
                    return Err(anyhow::anyhow!("dump {table}: {e}"));
                }
            }
        }
        // org_id is unused here but kept for symmetry with the metadata payload
        // built by the caller. Touch it to silence dead-code lint.
        let _ = org_id_owned;
        Ok(out)
    })
    .await
    .context("dump task join")??;

    let size = approx_dump_size(&dumps);
    let total_rows: i64 = dumps.iter().map(|d| d.row_count).sum();
    let table_count = dumps.len() as i64;

    // 3. Insert each table's data
    for dump in &dumps {
        insert_table_dump(pool, backup_id, dump)
            .await
            .with_context(|| format!("insert dump for {}", dump.table_name))?;
    }

    Ok((size, table_count, total_rows))
}

/// Boot-time wiring: ensure the schema exists, then spawn the background
/// loop. If `BACKUP_DATABASE_URL` is unset or the schema cannot be applied,
/// the function logs a warning and returns without crashing the app.
pub async fn boot(pool: Option<PgPool>) {
    let Some(pool) = pool else {
        tracing::warn!(
            "BACKUP_DATABASE_URL is not set — Postgres backup layer disabled. \
             Set it in the environment to enable periodic snapshots."
        );
        return;
    };

    if let Err(e) = ensure_schema(&pool).await {
        tracing::error!("Failed to apply backup schema, backup layer disabled: {e:#}");
        return;
    }
    tracing::info!("Backup schema ready on Postgres");
}

/// Configuration for the background job. `interval` is the period between
/// backup runs; `org_id` is the org we snapshot (NexusMind is single-tenant
/// per process, so this is a constant).
#[derive(Clone, Debug)]
pub struct BackupJobConfig {
    pub interval: Duration,
    pub org_id:   String,
}

impl BackupJobConfig {
    /// Build from env vars. `BACKUP_INTERVAL_HOURS` overrides the 6h default.
    /// `BACKUP_ORG_ID` defaults to `"default"`.
    pub fn from_env() -> Self {
        let hours: u64 = std::env::var("BACKUP_INTERVAL_HOURS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(6);
        let org_id = std::env::var("BACKUP_ORG_ID").unwrap_or_else(|_| "default".to_string());
        Self {
            interval: Duration::from_secs(hours.saturating_mul(3600).max(60)),
            org_id,
        }
    }
}

/// Spawn the background backup loop. Returns immediately; the loop runs
/// forever. The first run happens after `interval`, not immediately, so we
/// don't dump on every boot — admins can trigger a manual backup via the API
/// if they want one now.
pub fn spawn_background_job(
    pool: PgPool,
    sqlite: Arc<Mutex<Connection>>,
    cfg: BackupJobConfig,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(cfg.interval);
        // Skip the first immediate tick — we want a wait before the first run.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            tracing::info!("Background backup starting (org={})", cfg.org_id);
            match run_backup(&pool, sqlite.clone(), &cfg.org_id, "full").await {
                Ok(r) => tracing::info!(
                    "Background backup {} complete: {} tables, {} rows, {} bytes in {}ms",
                    r.backup_id,
                    r.table_count,
                    r.total_rows,
                    r.size_bytes,
                    r.duration_ms
                ),
                Err(e) => tracing::error!("Background backup failed: {e:#}"),
            }
        }
    })
}
