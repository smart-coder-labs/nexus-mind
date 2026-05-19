use clap::Parser;
use nexusmind::api;
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
    db::migrations::run(&conn)?;

    let app = api::router::build(conn, config.clone());

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    tracing::info!("NexusMind listening on {addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
