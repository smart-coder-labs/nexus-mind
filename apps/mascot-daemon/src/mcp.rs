//! Native Model Context Protocol (MCP) stdio client host.
//!
//! Spawns configured MCP servers (e.g. `@smart-coder-labs/nexusmind-mcp`, GitHub,
//! Playwright, Calendar, etc.), negotiates `initialize`, discovers `tools/list`,
//! and invokes `tools/call` — without ever placing literal secrets in config files.

use crate::config::McpServerConfig;
use crate::redact::LocalRedactor;
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::{info, warn};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpToolInfo {
    pub server_id: String,
    pub name: String,
    pub qualified_name: String,
    pub description: String,
    pub input_schema: Value,
}

struct McpConnection {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: AtomicU64,
}

impl McpConnection {
    async fn spawn(cfg: &McpServerConfig) -> Result<Self> {
        let mut cmd = Command::new(&cfg.command);
        cmd.args(&cfg.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        for (k, v) in &cfg.env {
            let resolved = resolve_env_placeholder(v);
            cmd.env(k, resolved);
        }

        let mut child = cmd
            .spawn()
            .with_context(|| format!("failed to spawn MCP server '{}' ({})", cfg.id, cfg.command))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow!("MCP server '{}' missing stdin", cfg.id))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("MCP server '{}' missing stdout", cfg.id))?;

        let mut conn = Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: AtomicU64::new(1),
        };

        // MCP handshake: initialize + notifications/initialized
        let _init = conn
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {},
                    "clientInfo": { "name": "nexus-mascot-daemon", "version": "0.1.0" }
                }),
            )
            .await?;
        conn.notify("notifications/initialized", json!({})).await?;
        Ok(conn)
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let msg = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        let mut line = serde_json::to_string(&msg)?;
        line.push('\n');
        self.stdin.write_all(line.as_bytes()).await?;
        self.stdin.flush().await?;

        let deadline = Duration::from_secs(15);
        timeout(deadline, async {
            let mut buf = String::new();
            loop {
                buf.clear();
                let n = self.stdout.read_line(&mut buf).await?;
                if n == 0 {
                    bail!("MCP server closed stdout unexpectedly");
                }
                let trimmed = buf.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let val: Value = match serde_json::from_str(trimmed) {
                    Ok(v) => v,
                    Err(_) => continue, // ignore stray non-JSON stdout noise
                };
                if val.get("id").and_then(Value::as_u64) == Some(id) {
                    if let Some(err) = val.get("error") {
                        bail!("MCP error: {err}");
                    }
                    return Ok(val.get("result").cloned().unwrap_or(Value::Null));
                }
            }
        })
        .await
        .context("MCP request timed out")?
    }
}

/// Manages configured MCP servers and exposes their discovered tools to System 2.
pub struct McpHost {
    servers: Mutex<HashMap<String, McpConnection>>,
    tools: Mutex<Vec<McpToolInfo>>,
}

impl Default for McpHost {
    fn default() -> Self {
        Self::new()
    }
}

impl McpHost {
    pub fn new() -> Self {
        Self {
            servers: Mutex::new(HashMap::new()),
            tools: Mutex::new(Vec::new()),
        }
    }

    pub async fn connect_configured(&self, configs: &[McpServerConfig]) -> usize {
        let mut connected = 0;
        for cfg in configs {
            if !cfg.enabled {
                continue;
            }
            match self.connect_server(cfg).await {
                Ok(tool_count) => {
                    info!(server = %cfg.id, tools = tool_count, "connected MCP server");
                    connected += 1;
                }
                Err(err) => {
                    warn!(server = %cfg.id, error = %err, "could not connect MCP server");
                }
            }
        }
        connected
    }

    pub async fn connect_server(&self, cfg: &McpServerConfig) -> Result<usize> {
        let mut conn = McpConnection::spawn(cfg).await?;
        let list = conn.request("tools/list", json!({})).await?;
        let mut discovered = Vec::new();
        if let Some(arr) = list.get("tools").and_then(Value::as_array) {
            for item in arr {
                if let Some(name) = item.get("name").and_then(Value::as_str) {
                    let desc = item
                        .get("description")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    let schema = item
                        .get("inputSchema")
                        .cloned()
                        .unwrap_or_else(|| json!({"type": "object", "properties": {}}));
                    discovered.push(McpToolInfo {
                        server_id: cfg.id.clone(),
                        name: name.to_string(),
                        qualified_name: format!("mcp__{}__{}", sanitize_ident(&cfg.id), sanitize_ident(name)),
                        description: desc,
                        input_schema: schema,
                    });
                }
            }
        }
        let count = discovered.len();
        self.servers.lock().await.insert(cfg.id.clone(), conn);
        let mut all_tools = self.tools.lock().await;
        all_tools.retain(|t| t.server_id != cfg.id);
        all_tools.extend(discovered);
        Ok(count)
    }

    pub async fn list_tools(&self) -> Vec<McpToolInfo> {
        self.tools.lock().await.clone()
    }

    pub async fn call_tool(&self, server_id: &str, tool_name: &str, arguments: Value) -> Result<String> {
        let mut servers = self.servers.lock().await;
        let conn = servers
            .get_mut(server_id)
            .ok_or_else(|| anyhow!("MCP server '{server_id}' is not connected"))?;
        let result = conn
            .request(
                "tools/call",
                json!({
                    "name": tool_name,
                    "arguments": arguments
                }),
            )
            .await?;

        let raw = serde_json::to_string_pretty(&result)?;
        let mut redactor = LocalRedactor::new();
        Ok(redactor.redact(&raw))
    }
}

fn sanitize_ident(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' })
        .collect()
}

fn resolve_env_placeholder(raw: &str) -> String {
    if let Some(inner) = raw.strip_prefix("${").and_then(|s| s.strip_suffix('}')) {
        std::env::var(inner).unwrap_or_default()
    } else {
        raw.to_string()
    }
}
