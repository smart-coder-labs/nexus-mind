use clap::Parser;
use nexusmind::api;
use nexusmind::backup;
use nexusmind::config;
use nexusmind::db;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = config::Config::parse();

    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    let conn = db::connection::connect(&config.db_path)?;
    db::migrations::run_all(&conn)?;

    // Reset zombie index runs: a code project left in 'indexing' by a crash/OOM/restart
    // would report "indexing" forever and block re-indexing. Flip them to 'error'.
    match db::queries::fail_stale_indexing_projects(&conn) {
        Ok(n) if n > 0 => tracing::warn!("Reset {n} interrupted code index run(s) to 'error'"),
        Ok(_) => {}
        Err(e) => tracing::warn!("Failed to reset stale indexing projects: {e}"),
    }

    // Backfill the default project for orgs created before default-project onboarding
    // existed, so agents using the standard "nexus-mind" project don't get 404s.
    match db::queries::ensure_default_projects(&conn) {
        Ok(n) if n > 0 => tracing::info!(
            "Backfilled default '{}' project for {n} existing org(s)",
            db::queries::DEFAULT_PROJECT_NAME
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!("Default-project backfill failed: {e}"),
    }

    // Backup layer: optional. If BACKUP_DATABASE_URL is unset we log a warning
    // and keep going — the rest of the app works without it.
    let backup_pool = match config.backup_database_url.as_deref() {
        Some(url) => match backup::client::connect_pool(url).await {
            Ok(pool) => {
                tracing::info!("Connected to backup Postgres");
                Some(pool)
            }
            Err(e) => {
                tracing::error!("BACKUP_DATABASE_URL set but connection failed: {e:#}");
                None
            }
        },
        None => None,
    };
    backup::job::boot(backup_pool.clone()).await;

    // Build the router and the SqliteStore together so we can hand a clone
    // of the latter to the background backup job.
    let (mut app, store) = api::router::build_with_store(conn, config.clone());
    if let Some(pool) = backup_pool.clone() {
        app = app.layer(axum::Extension(pool.clone()));
    }

    // Background backup loop — only spawned when the pool is configured.
    if let Some(pool) = backup_pool {
        let job_cfg = backup::job::BackupJobConfig {
            interval: std::time::Duration::from_secs(config.backup_interval_hours * 3600),
            org_id: "default".to_string(),
        };
        // The handle is intentionally not awaited — the loop runs for the
        // lifetime of the process. Dropping it is fine; we just need to
        // own it long enough that the runtime doesn't cancel it.
        let _handle = backup::job::spawn_background_job(pool, store.conn(), job_cfg);
    }

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("NexusMind listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
