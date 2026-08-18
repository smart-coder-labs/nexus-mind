use clap::Parser;
use nexusmind::{config::Config, db, store::sqlite::SqliteStore};
use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Config::parse();
    tracing_subscriber::fmt()
        .with_env_filter(&config.log_level)
        .init();

    if !config.autonomous_agents_enabled {
        anyhow::bail!("AUTONOMOUS_AGENTS_ENABLED must be true for the worker")
    }
    if config.db_path == ":memory:" {
        anyhow::bail!("the autonomous worker requires the backend's persistent SQLite database")
    }

    let conn = db::connection::connect(&config.db_path)?;
    let schema_version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if schema_version < 62 {
        anyhow::bail!(
            "database schema v62 or newer is required; run the normal deployment migration first"
        )
    }

    tracing::info!("NexusMind autonomous worker starting on the backend host");
    nexusmind::automation::worker::spawn_local_worker(SqliteStore::new(conn), Arc::new(config))
        .await?;
    Ok(())
}
