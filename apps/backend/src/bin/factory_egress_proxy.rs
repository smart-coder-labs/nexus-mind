//! Sandbox egress proxy (factory F1). The only process in the sandbox namespace
//! with internet egress and the only holder of the real credentials.
//!
//! Environment:
//! - `FACTORY_PROXY_SIGNING_KEY` (required, ≥ 32 bytes): verifies run tokens.
//! - `FACTORY_ANTHROPIC_OAUTH_TOKEN` or `FACTORY_ANTHROPIC_API_KEY`: Claude credential.
//! - `FACTORY_NEXUSMIND_KEYS` (optional): JSON object `{"<org_id>": "<nexus-bot API key>"}`.
//! - `FACTORY_TUNNEL_ALLOWLIST` (optional): comma-separated CONNECT hosts.
//! - `FACTORY_PROXY_LISTEN` (default `0.0.0.0:8080`).
//! - `FACTORY_PROXY_MAX_IN_FLIGHT` (default 64): concurrent reverse requests.

use std::sync::Arc;

use nexusmind::factory::egress_server::{serve, AnthropicAuth, EgressConfig};

/// Public package registries only: tunnels never carry credentials.
const DEFAULT_ALLOWLIST: &[&str] = &[
    "registry.npmjs.org",
    "pypi.org",
    "files.pythonhosted.org",
    "crates.io",
    "static.crates.io",
    "index.crates.io",
    "codeload.github.com",
];

fn env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "factory_egress=info".into()),
        )
        .init();

    let signing_key = env("FACTORY_PROXY_SIGNING_KEY")
        .ok_or_else(|| anyhow::anyhow!("FACTORY_PROXY_SIGNING_KEY is required"))?;
    if signing_key.len() < 32 {
        anyhow::bail!("FACTORY_PROXY_SIGNING_KEY must be at least 32 bytes");
    }
    let anthropic = env("FACTORY_ANTHROPIC_OAUTH_TOKEN")
        .map(AnthropicAuth::OAuth)
        .or_else(|| env("FACTORY_ANTHROPIC_API_KEY").map(AnthropicAuth::ApiKey));
    let tunnel_allowlist = env("FACTORY_TUNNEL_ALLOWLIST")
        .map(|list| {
            list.split(',')
                .map(|host| host.trim().to_ascii_lowercase())
                .filter(|host| !host.is_empty())
                .collect()
        })
        .unwrap_or_else(|| {
            DEFAULT_ALLOWLIST
                .iter()
                .map(|host| host.to_string())
                .collect()
        });
    let config = EgressConfig {
        signing_key: signing_key.into_bytes(),
        anthropic,
        nexusmind_keys: match env("FACTORY_NEXUSMIND_KEYS") {
            Some(raw) => serde_json::from_str(&raw).map_err(|_| {
                anyhow::anyhow!("FACTORY_NEXUSMIND_KEYS must be a JSON object of org_id -> key")
            })?,
            None => Default::default(),
        },
        tunnel_allowlist,
        upstream_override: None,
        allow_private_upstreams: false,
        max_in_flight: env("FACTORY_PROXY_MAX_IN_FLIGHT")
            .and_then(|value| value.parse().ok())
            .unwrap_or(64),
    };
    let listen = env("FACTORY_PROXY_LISTEN").unwrap_or_else(|| "0.0.0.0:8080".into());
    let listener = tokio::net::TcpListener::bind(&listen).await?;
    tracing::info!(target: "factory_egress", %listen, anthropic = config.anthropic.is_some(), nexusmind_orgs = config.nexusmind_keys.len(), tunnels = config.tunnel_allowlist.len(), "egress proxy listening");
    serve(listener, Arc::new(config)).await?;
    Ok(())
}
