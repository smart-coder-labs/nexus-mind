//! Per-agent, revisioned resources. No package installation or global configuration mutation.
use anyhow::{bail, ensure, Context, Result};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const PROFILES: &[&str] = &[
    "essential",
    "only_context",
    "reduced_readonly",
    "factory_operator",
    "legacy",
];
fn profile() -> String {
    "essential".into()
}
fn enabled() -> bool {
    true
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HarnessRef {
    pub id: String,
    pub version: String,
    pub manifest_hash: String,
    /// Explicit manifest component paths, never an implicit "all".
    pub components: Vec<String>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Component {
    pub id: String,
    pub kind: String,
    #[serde(default = "enabled")]
    pub enabled: bool,
    /// Explicit extension tools, never a wildcard that could enable write tools in a review.
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<McpServer>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpServer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub env: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub headers: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default = "profile")]
    pub nexusmind_profile: String,
    #[serde(default)]
    pub harnesses: Vec<HarnessRef>,
    #[serde(default)]
    pub components: Vec<Component>,
    #[serde(default)]
    pub acknowledge_executable: bool,
}
impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            nexusmind_profile: profile(),
            harnesses: vec![],
            components: vec![],
            acknowledge_executable: false,
        }
    }
}
pub fn kinds(executor: &str) -> &'static [&'static str] {
    match executor {
        "claude" => &[
            "instructions",
            "skill",
            "script",
            "plugin",
            "prompt_template",
            "mcp",
        ],
        "pi" => &[
            "instructions",
            "skill",
            "script",
            "extension",
            "prompt_template",
            "mcp",
        ],
        "codex" => &["instructions", "skill", "script", "mcp"],
        // OpenShell receives Markdown inline and MCP configuration, not arbitrary resource folders.
        "nexus" => &["instructions", "mcp"],
        _ => &[],
    }
}
fn safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 100
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
}
fn safe_path(path: &str) -> bool {
    crate::factory::workflows::relative_path(path) && !path.split('/').any(|p| p.starts_with('.'))
}
fn reference(value: &str) -> bool {
    value
        .strip_prefix("${")
        .and_then(|s| s.strip_suffix('}'))
        .is_some_and(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
        })
}
impl RuntimeConfig {
    pub fn validate(&self, executor: &str) -> Result<()> {
        ensure!(
            PROFILES.contains(&self.nexusmind_profile.as_str()),
            "invalid_nexusmind_profile"
        );
        ensure!(
            self.harnesses.len() <= 16 && self.components.len() <= 64,
            "too_many_runtime_components"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= 1_048_576,
            "runtime_config_too_large"
        );
        let mut ids = BTreeSet::new();
        for component in &self.components {
            ensure!(
                safe_id(&component.id)
                    && ids.insert(component.id.replace('-', "_"))
                    && !component.id.to_ascii_lowercase().contains("nexusmind"),
                "invalid_or_duplicate_runtime_component"
            );
            ensure!(
                !component.enabled || kinds(executor).contains(&component.kind.as_str()),
                "runtime_component_unsupported_for_executor"
            );
            ensure!(
                component.tools.len() <= 32
                    && (component.tools.is_empty() || component.kind == "extension"),
                "invalid_extension_tools"
            );
            ensure!(
                component.tools.iter().all(|t| safe_id(t)
                    && ![
                        "read",
                        "bash",
                        "edit",
                        "write",
                        "grep",
                        "find",
                        "ls",
                        "powershell"
                    ]
                    .contains(&t.as_str())),
                "invalid_extension_tools"
            );
            if component.kind == "mcp" {
                let server = component.server.as_ref().context("missing_mcp_server")?;
                ensure!(
                    executor != "nexus" || (server.env.is_empty() && server.headers.is_empty()),
                    "nexus_runtime_credentials_require_provider"
                );
                ensure!(
                    component.files.is_empty() && server.command.is_some() != server.url.is_some(),
                    "invalid_mcp_transport"
                );
                ensure!(
                    server.args.len() <= 64
                        && server
                            .args
                            .iter()
                            .all(|s| s.len() <= 4096 && !s.contains('\0')),
                    "invalid_mcp_args"
                );
                if let Some(command) = &server.command {
                    ensure!(
                        !command.is_empty()
                            && command.len() <= 512
                            && !command.chars().any(char::is_whitespace)
                            && !command.starts_with('-'),
                        "invalid_mcp_command"
                    );
                    ensure!(server.headers.is_empty(), "invalid_mcp_headers");
                }
                if let Some(url) = &server.url {
                    let url = reqwest::Url::parse(url).context("invalid_mcp_url")?;
                    ensure!(
                        url.scheme() == "https"
                            && url.username().is_empty()
                            && url.password().is_none()
                            && url.query().is_none()
                            && url.fragment().is_none(),
                        "unsafe_mcp_url"
                    );
                    ensure!(
                        server.args.is_empty() && server.env.is_empty(),
                        "invalid_http_mcp_config"
                    );
                }
                for arg in server.args.iter().chain(server.command.iter()) {
                    crate::models::types::validate_safe_manifest_content(arg)
                        .map_err(anyhow::Error::msg)?;
                    ensure!(
                        ![
                            "--token",
                            "--api-key",
                            "--password",
                            "--secret",
                            "--authorization"
                        ]
                        .iter()
                        .any(|flag| arg.to_lowercase().starts_with(flag)),
                        "mcp_secrets_require_environment_references"
                    );
                }
                for (key, value) in server.env.iter().chain(server.headers.iter()) {
                    ensure!(
                        safe_id(key) && reference(value),
                        "mcp_secrets_require_environment_references"
                    );
                }
                ensure!(
                    !component.enabled || self.acknowledge_executable,
                    "runtime_executable_acknowledgement_required"
                );
            } else {
                ensure!(
                    component.server.is_none()
                        && !component.files.is_empty()
                        && component.files.len() <= 100,
                    "invalid_runtime_files"
                );
                for (path, content) in &component.files {
                    ensure!(
                        safe_path(path) && content.len() <= 262_144 && !content.contains('\0'),
                        "unsafe_runtime_file"
                    );
                    crate::models::types::validate_safe_manifest_content(content)
                        .map_err(anyhow::Error::msg)?;
                }
                if matches!(component.kind.as_str(), "plugin" | "extension" | "script")
                    || component
                        .files
                        .keys()
                        .any(|p| !p.ends_with(".md") && !p.ends_with(".txt"))
                {
                    ensure!(
                        !component.enabled || self.acknowledge_executable,
                        "runtime_executable_acknowledgement_required"
                    );
                }
                if component.kind == "instructions" {
                    ensure!(
                        component
                            .files
                            .keys()
                            .all(|p| p.to_ascii_lowercase().ends_with(".md")),
                        "instruction_files_must_be_markdown"
                    );
                }
                if component.kind == "skill" {
                    let content = component
                        .files
                        .get("SKILL.md")
                        .context("skill_requires_SKILL_md")?
                        .replace("\r\n", "\n");
                    let frontmatter = content
                        .strip_prefix("---\n")
                        .and_then(|s| s.split_once("\n---").map(|(head, _)| head))
                        .context("invalid_skill_frontmatter")?;
                    let metadata: serde_yaml::Value =
                        serde_yaml::from_str(frontmatter).context("invalid_skill_frontmatter")?;
                    let name = metadata["name"]
                        .as_str()
                        .context("invalid_skill_frontmatter")?;
                    ensure!(
                        !name.is_empty()
                            && name.len() <= 64
                            && !name.starts_with('-')
                            && !name.ends_with('-')
                            && !name.contains("--")
                            && name
                                .bytes()
                                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
                        "invalid_skill_frontmatter"
                    );
                    ensure!(
                        metadata["description"]
                            .as_str()
                            .is_some_and(|d| !d.trim().is_empty() && d.len() <= 1024),
                        "invalid_skill_frontmatter"
                    );
                }
                if component.kind == "extension" {
                    ensure!(
                        component.files.contains_key("index.ts")
                            || component.files.contains_key("index.js"),
                        "extension_requires_index"
                    );
                }
            }
        }
        Ok(())
    }
}

/// Markdown is portable data, not a native tool/plugin installation. Mixed bundles
/// and executable declarations retain the executor target constraint.
pub fn portable_markdown(manifest: &Value) -> bool {
    if !matches!(
        manifest.get("format").and_then(Value::as_str),
        None | Some("agent" | "skill" | "command" | "output_style")
    ) || manifest["security"]
        .get("executable")
        .is_some_and(|value| value != &json!(false))
    {
        return false;
    }
    let markdown = |file: &Value| {
        file["kind"] == "file"
            && file["path"]
                .as_str()
                .is_some_and(|p| p.to_ascii_lowercase().ends_with(".md"))
    };
    manifest["components"].as_array().is_some_and(|components| {
        !components.is_empty()
            && components.iter().all(|component| {
                if component["kind"] == "folder" {
                    component["entries"]
                        .as_array()
                        .is_some_and(|entries| !entries.is_empty() && entries.iter().all(&markdown))
                } else {
                    markdown(component)
                }
            })
    })
}

/// Run evidence for the harnesses a run carried: the pinned references from the
/// saved configuration and the components resolved from them at save time, with
/// a hash per injected file. Never the file contents.
pub fn harness_evidence(config: &Value) -> Value {
    use sha2::{Digest, Sha256};
    let assigned = config["runtime"]["harnesses"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|h| {
            json!({"id":h["id"],"version":h["version"],"manifest_hash":h["manifest_hash"],"components":h["components"]})
        })
        .collect::<Vec<_>>();
    let injected = config["runtime_resolved"]["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["enabled"] != false && c["id"].as_str().is_some_and(is_harness_component_id))
        .map(|c| {
            let files = c["files"]
                .as_object()
                .into_iter()
                .flatten()
                .map(|(path, content)| {
                    let digest = Sha256::digest(content.as_str().unwrap_or_default().as_bytes());
                    json!({"path":path,"sha256":hex::encode(digest)})
                })
                .collect::<Vec<_>>();
            json!({"id":c["id"],"kind":c["kind"],"files":files})
        })
        .collect::<Vec<_>>();
    json!({"assigned":assigned,"injected":injected})
}

/// `prepare_config` names harness components `h<harness>-<component>`.
fn is_harness_component_id(id: &str) -> bool {
    id.strip_prefix('h')
        .and_then(|rest| rest.split_once('-'))
        .is_some_and(|(a, b)| {
            !a.is_empty() && !b.is_empty() && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
        })
}

pub fn markdown_guidance(config: &Value) -> Value {
    json!(config["runtime_resolved"]["components"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["kind"] == "instructions" && c["enabled"] != false)
        .map(|c| json!({"id":c["id"],"files":c["files"]}))
        .collect::<Vec<_>>())
}

/// Resolve visible, approved, hash-pinned catalog versions at save time. The
/// derived field is never accepted from a client as authority.
pub fn prepare_config(conn: &Connection, org: &str, user: &str, config: &Value) -> Result<Value> {
    let mut config = config.clone();
    config
        .as_object_mut()
        .context("invalid_configuration")?
        .remove("runtime_resolved");
    let Some(raw) = config.get("runtime") else {
        return Ok(config);
    };
    let requested: RuntimeConfig =
        serde_json::from_value(raw.clone()).context("invalid_agent_runtime")?;
    let executor = super::super::db::queries::autonomous_executor(&config)?;
    requested.validate(executor)?;
    let mut resolved = requested.clone();
    let mut harness_ids = BTreeSet::new();
    for harness in &requested.harnesses {
        ensure!(
            harness_ids.insert(&harness.id) && !harness.components.is_empty(),
            "invalid_harness_selection"
        );
        let version = crate::db::queries::get_visible_harness_version(
            conn,
            org,
            &harness.id,
            &harness.version,
            Some(user),
        )?
        .context("harness_not_found")?;
        ensure!(
            version.status == "published" && version.manifest_hash == harness.manifest_hash,
            "harness_hash_or_status_mismatch"
        );
        let portable = portable_markdown(&version.manifest);
        ensure!(
            portable || version.targets.iter().any(|t| t == executor),
            "harness_executor_incompatible"
        );
        let approved: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM harness_install_approvals WHERE org_id=?1 AND user_id=?2 AND harness_version_id=?3 AND manifest_hash=?4 AND target_tool=?5 AND target_scope='agent-runtime' AND status='approved')", rusqlite::params![org, user, version.id, harness.manifest_hash, executor], |row| row.get(0))?;
        ensure!(approved, "approval_required");
        crate::db::queries::download_harness_version(
            conn,
            org,
            user,
            Some(user),
            &harness.id,
            &harness.version,
        )?
        .context("harness_not_found")?;
        let format = version.manifest["format"].as_str().unwrap_or("");
        let kind = if portable {
            "instructions"
        } else {
            match format {
                "skill" => "skill",
                "command" => "prompt_template",
                "hook" => "script",
                _ => bail!("unsupported_harness_format_for_runtime"),
            }
        };
        let components = version.manifest["components"]
            .as_array()
            .context("invalid_harness_manifest")?;
        let mut selected = BTreeSet::new();
        for path in &harness.components {
            ensure!(selected.insert(path), "duplicate_harness_component");
            let entry = components
                .iter()
                .find(|c| c["path"].as_str() == Some(path))
                .context("harness_component_not_found")?;
            let mut files = BTreeMap::new();
            if let Some(entries) = entry["entries"].as_array() {
                for file in entries {
                    let path = file["path"].as_str().context("invalid_harness_file")?;
                    let relative = path
                        .strip_prefix(&format!("{}/", entry["path"].as_str().unwrap_or_default()))
                        .unwrap_or(path);
                    files.insert(
                        relative.into(),
                        file["content"]
                            .as_str()
                            .context("harness_content_missing")?
                            .into(),
                    );
                }
            } else {
                let filename = if kind == "instructions" {
                    entry["path"].as_str().context("invalid_harness_file")?
                } else if kind == "skill" {
                    "SKILL.md"
                } else if kind == "script" {
                    "run.sh"
                } else {
                    "prompt.md"
                };
                files.insert(
                    filename.into(),
                    entry["content"]
                        .as_str()
                        .context("harness_content_missing")?
                        .into(),
                );
            }
            resolved.components.push(Component {
                id: format!("h{}-{}", harness_ids.len(), selected.len()),
                kind: kind.into(),
                enabled: true,
                tools: vec![],
                files,
                server: None,
            });
        }
    }
    resolved.validate(executor)?;
    config["runtime_resolved"] = serde_json::to_value(resolved)?;
    Ok(config)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Plan {
    pub files: BTreeMap<String, String>,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub mcp_names: Vec<String>,
    pub tools: Vec<String>,
}
/// Root is supplied by the trusted runtime, never by the saved configuration.
pub fn plan(runtime: &RuntimeConfig, executor: &str, root: &str, nexusmind: Value) -> Result<Plan> {
    runtime.validate(executor)?;
    let mut plan = Plan::default();
    let mut servers = serde_json::Map::new();
    let mut nexusmind = nexusmind;
    ensure!(
        nexusmind["command"].is_string(),
        "nexusmind_runtime_config_missing"
    );
    nexusmind["env"]["NEXUSMIND_MCP_TOOL_PROFILE"] = json!(runtime.nexusmind_profile);
    if executor == "pi" {
        nexusmind["exposure"] = json!("direct");
    }
    let nexus_name = if matches!(executor, "claude" | "nexus") {
        "plugin_nexusmind_nexusmind"
    } else {
        "nexusmind"
    };
    servers.insert(nexus_name.into(), nexusmind);
    plan.mcp_names.push(nexus_name.into());
    for component in runtime.components.iter().filter(|c| c.enabled) {
        plan.tools.extend(component.tools.iter().cloned());
        if component.kind == "mcp" {
            let mut server = serde_json::to_value(component.server.as_ref().unwrap())?;
            if component.server.as_ref().unwrap().url.is_some() {
                server["type"] = json!("http");
            }
            if executor == "pi" {
                server["exposure"] = json!("direct");
            }
            servers.insert(component.id.clone(), server);
            plan.mcp_names.push(component.id.clone());
            continue;
        }
        let directory = if executor == "codex" && component.kind == "skill" {
            format!("{root}/skills/{}", component.id)
        } else {
            format!("{root}/resources/{}", component.id)
        };
        if executor != "claude" || component.kind == "script" {
            for (path, content) in &component.files {
                plan.files
                    .insert(format!("{directory}/{path}"), content.clone());
            }
        }
        match (executor, component.kind.as_str()) {
            (_, "instructions") => {} // Included as bounded, non-authoritative task guidance.
            ("pi", "skill") => plan.args.extend(["--skill".into(), directory]),
            ("pi", "extension") => {
                let index = if component.files.contains_key("index.ts") {
                    "index.ts"
                } else {
                    "index.js"
                };
                plan.args
                    .extend(["--extension".into(), format!("{directory}/{index}")]);
            }
            ("pi", "prompt_template") => {
                for path in component.files.keys() {
                    plan.args
                        .extend(["--prompt-template".into(), format!("{directory}/{path}")]);
                }
            }
            ("claude", "skill" | "plugin" | "prompt_template") => {
                let plugin = format!("{root}/plugins/{}", component.id);
                plan.files.insert(
                    format!("{plugin}/.claude-plugin/plugin.json"),
                    json!({"name":component.id.to_ascii_lowercase().replace('_', "-"),"version":"1.0.0"}).to_string(),
                );
                for (path, content) in &component.files {
                    let path = if component.kind == "skill" {
                        format!("skills/{}/{path}", component.id)
                    } else if component.kind == "prompt_template" {
                        format!("commands/{path}")
                    } else if path == "plugin.json" {
                        ".claude-plugin/plugin.json".into()
                    } else {
                        path.clone()
                    };
                    plan.files
                        .insert(format!("{plugin}/{path}"), content.clone());
                }
                plan.args.extend(["--plugin-dir".into(), plugin]);
            }
            ("codex", "skill") => {} // Native discovery under CODEX_HOME/skills.
            (_, "script") => {
                let notice = format!(
                    "Assigned scripts (run explicitly, never automatic startup): {}",
                    component
                        .files
                        .keys()
                        .map(|p| format!("{directory}/{p}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                if matches!(executor, "pi" | "claude") {
                    plan.args.extend(["--append-system-prompt".into(), notice]);
                } else {
                    plan.args.extend([
                        "-c".into(),
                        format!("developer_instructions={}", serde_json::to_string(&notice)?),
                    ]);
                }
            }
            _ => bail!("runtime_component_unsupported_for_executor"),
        }
    }
    if executor == "pi" {
        plan.env.insert("PI_CODING_AGENT_DIR".into(), root.into());
        plan.files.insert(
            format!("{root}/mcp.json"),
            json!({"mcpServers":servers}).to_string(),
        );
        plan.args
            .extend(["--extension".into(), "builtin:mcp".into()]);
    } else if executor == "codex" {
        plan.env.insert("CODEX_HOME".into(), root.into());
        // JSON inline tables are not TOML; emit each scalar with JSON string escaping.
        let mut config = String::new();
        for (name, server) in servers {
            config.push_str(&format!("\n[mcp_servers.{name}]\n"));
            for key in ["command", "url"] {
                if let Some(value) = server.get(key).filter(|v| !v.is_null()) {
                    config.push_str(&format!("{key} = {value}\n"));
                }
            }
            if let Some(args) = server.get("args") {
                config.push_str(&format!("args = {args}\n"));
            }
            for (field, target) in [("env", "env"), ("headers", "http_headers")] {
                if let Some(values) = server[field].as_object().filter(|m| !m.is_empty()) {
                    config.push_str(&format!("[mcp_servers.{name}.{target}]\n"));
                    for (key, value) in values {
                        config.push_str(&format!("{} = {value}\n", serde_json::to_string(key)?));
                    }
                }
            }
        }
        plan.files.insert(format!("{root}/runtime.toml"), config);
    } else {
        plan.files.insert(
            format!("{root}/mcp.json"),
            json!({"mcpServers":servers}).to_string(),
        );
        plan.args
            .extend(["--mcp-config".into(), format!("{root}/mcp.json")]);
    }
    Ok(plan)
}

/// Load only the control-plane snapshot; never resolve "latest" at execution.
pub fn runtime_from_config(config: &Value) -> Result<RuntimeConfig> {
    if config.get("runtime").is_some() {
        serde_json::from_value(
            config
                .get("runtime_resolved")
                .context("runtime_snapshot_missing")?
                .clone(),
        )
        .context("invalid_runtime_snapshot")
    } else {
        Ok(RuntimeConfig::default())
    }
}

pub fn public_error(error: &anyhow::Error) -> &'static str {
    match error.to_string().as_str() {
        "factory_bot_not_configured" => "factory_bot_not_configured",
        "factory_bot_disabled" => "factory_bot_disabled",
        "runtime_snapshot_missing" => "runtime_snapshot_missing",
        "nexusmind_runtime_config_missing" => "nexusmind_runtime_config_missing",
        _ => "runtime_setup_failed",
    }
}

/// Own both the private runtime files and its short-lived bot credential.
/// Drop also runs on cancellation/setup failure; expiry covers a worker crash.
pub struct LocalRuntime {
    _directory: tempfile::TempDir,
    store: crate::store::sqlite::SqliteStore,
    key_id: Option<String>,
}
impl Drop for LocalRuntime {
    fn drop(&mut self) {
        if let Some(id) = &self.key_id {
            if let Ok(c) = self.store.conn().lock() {
                if let Err(error) = c.execute("UPDATE api_keys SET revoked=1 WHERE id=?1", [id]) {
                    tracing::warn!(%error, "could not revoke temporary runtime credential; expiry remains enforced");
                }
            }
        }
    }
}
fn issue_bot_key(
    store: &crate::store::sqlite::SqliteStore,
    org: &str,
    run: &str,
    wall_time: std::time::Duration,
) -> Result<(String, String)> {
    let h = store.conn();
    let c = h.lock().map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
    let tx = c.unchecked_transaction()?;
    let bot = crate::db::factory_queries::get_factory_bot(&tx, org)?
        .context("factory_bot_not_configured")?;
    ensure!(bot.status == "active", "factory_bot_disabled");
    let owned: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM autonomous_agent_runs WHERE id=?1 AND org_id=?2)", rusqlite::params![run, org], |r| r.get(0))?;
    ensure!(owned, "run_not_found");
    let (key, raw) = crate::db::queries::create_key_admin(&tx, org, &bot.user_id, &format!("agent-runtime:{run}"), None)?;
    // ponytail: one credential per invocation; existing API-key expiry handles worker crashes.
    let ttl = format!("+{} seconds", wall_time.as_secs().clamp(1, 86400) + 300);
    tx.execute("UPDATE api_keys SET expires_at=datetime('now',?2) WHERE id=?1", rusqlite::params![key.id, ttl])?;
    crate::db::queries::append_autonomous_agent_event(&tx, org, run, "runtime.identity_selected",
        &json!({"user_id":bot.user_id,"role":bot.role,"source":"organization_factory_bot","expires_in_seconds":wall_time.as_secs().clamp(1,86400)+300}))?;
    tx.commit()?;
    Ok((key.id, raw))
}
pub fn apply_local(
    command: &mut tokio::process::Command,
    config: &Value,
    template: &str,
    store: &crate::store::sqlite::SqliteStore,
    org: &str,
    run: &str,
    wall_time: std::time::Duration,
) -> Result<(LocalRuntime, Vec<String>)> {
    use std::{fs, path::Path};
    let executor = crate::db::queries::autonomous_executor(config)?;
    ensure!(executor != "codex", "codex_requires_sandbox");
    let runtime = runtime_from_config(config)?;
    let directory = tempfile::Builder::new()
        .prefix("nm-agent-runtime-")
        .tempdir()?;
    let mcp_path = std::env::var("AUTONOMOUS_NEXUSMIND_MCP_CONFIG")
        .unwrap_or_else(|_| "/app/nexusmind-mcp.json".into());
    let mcp: Value =
        serde_json::from_slice(&fs::read(mcp_path).context("nexusmind_runtime_config_missing")?)?;
    let mut secrets = Vec::new();
    let mut nexusmind = mcp["mcpServers"]
        .as_object()
        .and_then(|servers| {
            servers
                .iter()
                .find(|(name, _)| name.contains("nexusmind"))
                .map(|(_, server)| server.clone())
        })
        .context("nexusmind_runtime_config_missing")?;
    if executor == "nexus" {
        if let Some(env) = nexusmind["env"].as_object_mut() {
            // OpenShell providers own credentials; never upload host API keys.
            env.retain(|key, _| {
                !["KEY", "TOKEN", "SECRET", "PASSWORD"]
                    .iter()
                    .any(|part| key.to_ascii_uppercase().contains(part))
            });
        }
    }
    let (key_id, key) = if executor != "nexus" {
        let (id, key) = issue_bot_key(store, org, run, wall_time)?;
        (Some(id), Some(key))
    } else { (None, None) };
    let local = LocalRuntime { _directory: directory, store: store.clone(), key_id };
    let root = local._directory.path().to_str().context("invalid_runtime_directory")?;
    if let Some(key) = key {
        // Override stale/global/personal keys, including literals in the host MCP config.
        nexusmind["env"]["NEXUSMIND_API_KEY"] = json!(key);
        secrets.push(key);
        command.env_remove("NEXUSMIND_API_KEY");
    }
    let mut plan = plan(&runtime, executor, root, nexusmind)?;
    if executor == "nexus" && matches!(template, "qa" | "judge") {
        let qa_path =
            std::env::var("AUTONOMOUS_QA_MCP_CONFIG").unwrap_or_else(|_| "/app/qa-mcp.json".into());
        let qa: Value =
            serde_json::from_slice(&fs::read(qa_path).context("qa_mcp_config_missing")?)?;
        let path = format!("{root}/mcp.json");
        let mut mcp: Value = serde_json::from_str(&plan.files[&path])?;
        for (name, server) in qa["mcpServers"]
            .as_object()
            .context("qa_mcp_config_missing")?
        {
            ensure!(
                !name.contains("nexusmind") && mcp["mcpServers"].get(name).is_none(),
                "duplicate_mcp_server"
            );
            mcp["mcpServers"][name] = server.clone();
            plan.mcp_names.push(name.clone());
        }
        plan.files.insert(path, serde_json::to_string(&mcp)?);
    }
    for (path, content) in &plan.files {
        let path = Path::new(path);
        fs::create_dir_all(path.parent().context("invalid_runtime_directory")?)?;
        fs::write(path, content)?;
    }
    if executor == "pi" {
        let original = std::env::var_os("PI_CODING_AGENT_DIR")
            .map(std::path::PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join(".pi/agent")))
            .context("pi_auth_directory_missing")?;
        // Share only the executor's authentication, never global settings/resources.
        #[cfg(unix)]
        std::os::unix::fs::symlink(
            original.join("auth.json"),
            local._directory.path().join("auth.json"),
        )?;
    }
    let allowed = std::env::var("AUTONOMOUS_RUNTIME_ALLOWED_ENV").unwrap_or_default();
    for server in runtime
        .components
        .iter()
        .filter(|c| c.enabled)
        .filter_map(|c| c.server.as_ref())
    {
        for value in server.env.values().chain(server.headers.values()) {
            let key = &value[2..value.len() - 1];
            ensure!(
                allowed.split(',').any(|v| v.trim() == key),
                "runtime_environment_reference_not_allowed"
            );
            let secret = std::env::var(key).context("runtime_environment_reference_missing")?;
            command.env(key, &secret);
            secrets.push(secret);
        }
    }
    if executor == "nexus" {
        let original = command.as_std();
        let program = original.get_program().to_owned();
        let mut args = original
            .get_args()
            .map(|a| a.to_owned())
            .collect::<Vec<_>>();
        let envs = original
            .get_envs()
            .map(|(k, v)| (k.to_owned(), v.map(|v| v.to_owned())))
            .collect::<Vec<_>>();
        let index = args
            .iter()
            .position(|a| a == "--allowed-tools")
            .context("nexus_tool_allowlist_missing")?
            + 1;
        let tools = plan
            .mcp_names
            .iter()
            .map(|n| format!("mcp__{n}__*"))
            .collect::<Vec<_>>()
            .join(",");
        args[index] = format!("{},{tools}", args[index].to_string_lossy()).into();
        let mut replacement = tokio::process::Command::new(program);
        replacement
            .args(args)
            .env_clear()
            .stdin(std::process::Stdio::piped());
        for (key, value) in envs {
            if let Some(value) = value {
                replacement.env(key, value);
            }
        }
        *command = replacement;
    }
    if executor == "pi" {
        let base = command
            .as_std()
            .get_args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|args| args[0] == "--tools")
            .map(|args| args[1].to_string_lossy().into_owned())
            .unwrap_or_default();
        let extra = plan
            .tools
            .iter()
            .cloned()
            .chain(["mcp__*".into()])
            .collect::<Vec<_>>()
            .join(",");
        command.args(["--tools", &format!("{base},{extra}")]);
    }
    if executor == "claude" {
        command.args(["--setting-sources", ""]);
        let base = command
            .as_std()
            .get_args()
            .collect::<Vec<_>>()
            .windows(2)
            .find(|args| args[0] == "--allowedTools")
            .map(|args| args[1].to_string_lossy().into_owned())
            .unwrap_or_default();
        let tools = plan
            .mcp_names
            .iter()
            .map(|n| format!("mcp__{n}__*"))
            .collect::<Vec<_>>()
            .join(",");
        command.args(["--allowedTools", &format!("{base},{tools}")]);
    }
    command.args(&plan.args).envs(&plan.env);
    command.env("NEXUSMIND_MCP_TOOL_PROFILE", &runtime.nexusmind_profile);
    Ok((local, secrets))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn runtime_credentials_use_the_tenant_bot_expire_and_revoke_without_rotating_its_key() {
        use crate::{db::{connection, migrations, queries, factory_queries}, store::sqlite::SqliteStore, auth::api_keys::hash_key};
        let c = connection::connect(":memory:").unwrap();
        migrations::run(&c).unwrap();
        let (org, user, _) = queries::bootstrap(&c, "Org", "org", "owner@test.com", "Owner").unwrap();
        c.execute("INSERT INTO autonomous_agent_definitions(id,org_id,template_key,template_version,name,status,current_revision,created_by) VALUES ('reviewer',?1,'github_pr_reviewer',1,'Reviewer','enabled',1,?2)",rusqlite::params![org.id,user.id]).unwrap();
        c.execute("INSERT INTO autonomous_agent_revisions(id,definition_id,revision,config_json,config_hash,capabilities_json,budgets_json,validation_status,created_by) VALUES ('rev','reviewer',1,'{\"repository\":\"acme/api\"}','hash','[]','{}','pending',?1)",[&user.id]).unwrap();
        c.execute("INSERT INTO autonomous_agent_validations(id,org_id,definition_id,revision_id,config_hash,status,result_json,validated_by) VALUES ('valid',?1,'reviewer','rev','hash','valid','{}',?2)",rusqlite::params![org.id,user.id]).unwrap();
        let run = queries::enqueue_autonomous_agent_run(&c,&org.id,"reviewer","manual","identity-test",None,None).unwrap().unwrap();
        let store = SqliteStore::new(c);
        let ttl = std::time::Duration::from_secs(60);
        assert_eq!(issue_bot_key(&store,&org.id,&run.id,ttl).unwrap_err().to_string(), "factory_bot_not_configured");
        let (bot, original) = {
            let h=store.conn(); let c=h.lock().unwrap();
            factory_queries::rotate_factory_bot_key(&c,&org.id,&user.id).unwrap()
        };
        let (id, raw) = issue_bot_key(&store,&org.id,&run.id,ttl).unwrap();
        {
            let h=store.conn(); let c=h.lock().unwrap();
            let auth=queries::validate_api_key(&c,&hash_key(&raw)).unwrap().unwrap();
            assert_eq!(auth.user_id,bot.user_id);
            assert_eq!(auth.org_id,org.id);
            assert!(!auth.role.is_privileged());
            assert!(queries::validate_api_key(&c,&hash_key(&original)).unwrap().is_some());
            let expiring:bool=c.query_row("SELECT expires_at>datetime('now') AND expires_at<=datetime('now','+6 minutes') FROM api_keys WHERE id=?1",[&id],|r|r.get(0)).unwrap();
            assert!(expiring);
        }
        let guard=LocalRuntime{_directory:tempfile::tempdir().unwrap(),store:store.clone(),key_id:Some(id.clone())};
        drop(guard);
        {
            let h=store.conn(); let c=h.lock().unwrap();
            assert!(queries::validate_api_key(&c,&hash_key(&raw)).unwrap().is_none());
            assert!(queries::validate_api_key(&c,&hash_key(&original)).unwrap().is_some());
        }
        // Exercise the actual MCP materialization, not just key issuance.
        let mcp=tempfile::NamedTempFile::new().unwrap();
        std::fs::write(mcp.path(),json!({"mcpServers":{"nexusmind":{"command":"nexusmind-mcp","env":{"NEXUSMIND_API_KEY":"revoked-personal-fixture"}}}}).to_string()).unwrap();
        let previous=std::env::var_os("AUTONOMOUS_NEXUSMIND_MCP_CONFIG");
        std::env::set_var("AUTONOMOUS_NEXUSMIND_MCP_CONFIG",mcp.path());
        let mut command=tokio::process::Command::new("claude");
        command.args(["--allowedTools","Read,Grep"]).env("NEXUSMIND_API_KEY","revoked-personal-fixture");
        let (local,secrets)=apply_local(&mut command,&json!({"executor":"claude"}),"github_pr_reviewer",&store,&org.id,&run.id,ttl).unwrap();
        let materialized:Value=serde_json::from_slice(&std::fs::read(local._directory.path().join("mcp.json")).unwrap()).unwrap();
        let actual=materialized["mcpServers"]["plugin_nexusmind_nexusmind"]["env"]["NEXUSMIND_API_KEY"].as_str().unwrap();
        assert_ne!(actual,"revoked-personal-fixture");
        assert!(secrets.iter().any(|s|s==actual),"The temporary credential must be redacted from output");
        assert!(command.as_std().get_envs().any(|(k,v)|k=="NEXUSMIND_API_KEY" && v.is_none()));
        {
            let h=store.conn(); let c=h.lock().unwrap();
            assert_eq!(queries::validate_api_key(&c,&hash_key(actual)).unwrap().unwrap().user_id,bot.user_id);
        }
        drop(local);
        {
            let h=store.conn(); let c=h.lock().unwrap();
            assert!(queries::validate_api_key(&c,&hash_key(actual)).unwrap().is_none());
        }
        let invalid=json!({"executor":"claude","runtime":{},"runtime_resolved":{"acknowledge_executable":true,"components":[{"id":"extra","kind":"mcp","server":{"command":"example-mcp","env":{"AUTH":"${UNAPPROVED_RUNTIME_ENV}"}}}]}});
        assert!(apply_local(&mut command,&invalid,"github_pr_reviewer",&store,&org.id,&run.id,ttl).is_err());
        {
            let h=store.conn(); let c=h.lock().unwrap();
            assert_eq!(c.query_row("SELECT count(*) FROM api_keys WHERE label LIKE 'agent-runtime:%' AND revoked=0",[],|r|r.get::<_,i64>(0)).unwrap(),0,"Setup failure must revoke its just-issued key");
        }
        if let Some(value)=previous {std::env::set_var("AUTONOMOUS_NEXUSMIND_MCP_CONFIG",value);} else {std::env::remove_var("AUTONOMOUS_NEXUSMIND_MCP_CONFIG");}
        let (expired_id,expired)=issue_bot_key(&store,&org.id,&run.id,ttl).unwrap();
        {
            let h=store.conn(); let c=h.lock().unwrap();
            c.execute("UPDATE api_keys SET expires_at=datetime('now','-1 second') WHERE id=?1",[expired_id]).unwrap();
            assert!(queries::validate_api_key(&c,&hash_key(&expired)).unwrap().is_none());
        }
        assert!(issue_bot_key(&store,"other-org",&run.id,ttl).is_err());
        {
            let h=store.conn(); let c=h.lock().unwrap();
            c.execute("UPDATE users SET disabled_at=datetime('now') WHERE id=?1",[&bot.user_id]).unwrap();
        }
        assert_eq!(issue_bot_key(&store,&org.id,&run.id,ttl).unwrap_err().to_string(),"factory_bot_disabled");
    }
    #[test]
    fn isolates_resources_and_rejects_incompatible_or_secret_configs() {
        let mut runtime = RuntimeConfig::default();
        runtime.components.push(Component {
            id: "rust".into(),
            kind: "skill".into(),
            enabled: true,
            tools: vec![],
            files: BTreeMap::from([(
                "SKILL.md".into(),
                "---\nname: rust\ndescription: Rust review\n---\nReview Rust".into(),
            )]),
            server: None,
        });
        let pi = plan(
            &runtime,
            "pi",
            "/runtime",
            json!({"command":"nexusmind-mcp","env":{}}),
        )
        .unwrap();
        assert!(pi.args.contains(&"--skill".into()));
        assert_eq!(pi.env["PI_CODING_AGENT_DIR"], "/runtime");
        assert!(pi.files["/runtime/mcp.json"].contains("essential"));
        assert!(runtime.validate("nexus").is_err());
        runtime.components[0]
            .files
            .insert("../escape".into(), "bad".into());
        assert!(runtime.validate("pi").is_err());
        let mut mcp: RuntimeConfig = serde_json::from_value(json!({"acknowledge_executable":true,"components":[{"id":"docs","kind":"mcp","server":{"url":"https://example.com/mcp","headers":{"Authorization":"secret"}}}]})).unwrap();
        assert!(mcp.validate("pi").is_err());
        mcp.components[0]
            .server
            .as_mut()
            .unwrap()
            .headers
            .insert("Authorization".into(), "${DOCS_TOKEN}".into());
        assert!(mcp.validate("pi").is_ok());
        let native = plan(
            &mcp,
            "claude",
            "/runtime",
            json!({"command":"nexusmind-mcp","env":{}}),
        )
        .unwrap();
        let servers: Value = serde_json::from_str(&native.files["/runtime/mcp.json"]).unwrap();
        assert_eq!(servers["mcpServers"]["docs"]["type"], "http");
        assert!(servers["mcpServers"]["docs"].get("args").is_none());
        mcp.components[0].id = "nexusmind".into();
        assert!(mcp.validate("pi").is_err());
    }
    #[test]
    fn disabled_extensions_do_not_load_or_enable_tools() {
        let runtime: RuntimeConfig = serde_json::from_value(json!({"components":[{"id":"browser","kind":"extension","enabled":false,"tools":["browser"],"files":{"index.ts":"export default () => {}"}}]})).unwrap();
        let plan = plan(
            &runtime,
            "codex",
            "/runtime",
            json!({"command":"nexusmind-mcp","env":{}}),
        )
        .unwrap();
        assert!(plan.tools.is_empty());
        assert!(!plan.files.keys().any(|p| p.contains("browser")));
        assert!(plan.files["/runtime/runtime.toml"].contains("[mcp_servers.nexusmind]"));
        let mut runtime = runtime;
        runtime.components[0].enabled = true;
        assert!(runtime.validate("codex").is_err());
    }
    #[test]
    fn native_plugins_and_skills_are_materialized_under_the_run_root() {
        let runtime: RuntimeConfig = serde_json::from_value(json!({"acknowledge_executable":true,"components":[{"id":"review","kind":"plugin","files":{"plugin.json":"{\"name\":\"review\"}","commands/review.md":"Review contracts"}}]})).unwrap();
        let plan = plan(
            &runtime,
            "claude",
            "/runtime",
            json!({"command":"nexusmind-mcp","env":{}}),
        )
        .unwrap();
        assert!(plan
            .files
            .contains_key("/runtime/plugins/review/.claude-plugin/plugin.json"));
        assert!(plan.files.keys().all(|p| p.starts_with("/runtime/")));
        assert!(plan.files["/runtime/mcp.json"].contains("plugin_nexusmind_nexusmind"));
        assert!(plan.args.contains(&"--plugin-dir".into()));
    }
}
