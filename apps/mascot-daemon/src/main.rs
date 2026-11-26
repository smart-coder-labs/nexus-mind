use anyhow::Result;
use clap::{Parser, Subcommand};
use nexus_mascot_daemon::config::{default_config_dir, MascotConfig};
use nexus_mascot_daemon::ipc::{serve_unix_socket, verify_and_heartbeat, DaemonState};
use nexus_mascot_daemon::triggers::spawn_background_scheduler;
use std::path::PathBuf;
use std::sync::Arc;
use systemone_client::decisions::ActionRiskInput;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "nexus-mascot-daemon",
    about = "Autonomous local-first desktop mascot daemon for NexusMind"
)]
struct Cli {
    /// Path to mascot YAML config (default: ~/.nexusmind/mascot.yaml)
    #[arg(long)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run the local IPC daemon, MCP host, and autonomous scheduler
    Run,
    /// Write a default ~/.nexusmind/mascot.yaml configuration file
    InitConfig {
        #[arg(long)]
        force: bool,
    },
    /// Probe the configured System 1 engine (Laya local / Jev cloud / Hybrid)
    CheckDecisionEngine,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let cfg_path = cli
        .config
        .unwrap_or_else(|| default_config_dir().join("mascot.yaml"));

    match cli.command {
        Commands::InitConfig { force } => {
            if cfg_path.exists() && !force {
                println!(
                    "Config already exists at {} (pass --force to overwrite)",
                    cfg_path.display()
                );
                return Ok(());
            }
            let cfg = MascotConfig::default();
            cfg.write_to(&cfg_path)?;
            println!("Wrote default mascot config to {}", cfg_path.display());
            Ok(())
        }
        Commands::CheckDecisionEngine => {
            let cfg = MascotConfig::load_or_default(&cfg_path)?;
            let router = cfg.build_decision_router()?;
            println!(
                "Probing decision engine (mode={:?}, laya={})...",
                cfg.decision_engine.mode, cfg.decision_engine.laya.endpoint
            );
            let res = router
                .evaluate_action_risk(&ActionRiskInput {
                    action_kind: "filesystem".into(),
                    summary: "List files in ~/Desktop".into(),
                    target_app_or_path: Some("~/Desktop".into()),
                    command_preview: Some("ls -la ~/Desktop".into()),
                    is_private_context: false,
                })
                .await?;
            println!(
                "OK — source={:?} model={} latency={}ms class={} risk={:.2} conf={:.2}",
                res.source,
                res.model,
                res.latency_ms,
                res.answer.action_class,
                res.answer.risk,
                res.answer.risk_confidence
            );
            Ok(())
        }
        Commands::Run => {
            let cfg = MascotConfig::load_or_default(&cfg_path)?;
            let socket = cfg.resolved_socket_path();
            let mcp_servers = cfg.mcp_servers.clone();
            let state = DaemonState::new(cfg)?;
            let _ = verify_and_heartbeat(&state).await;
            state.mcp.connect_configured(&mcp_servers).await;
            spawn_background_scheduler(Arc::clone(&state));
            serve_unix_socket(&socket, state).await
        }
    }
}
