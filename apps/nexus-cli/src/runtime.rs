use crate::model::{RuntimeCapabilities, RuntimeDescriptor};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::mpsc::Sender,
};

#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    Activity(String),
    Assistant(String),
}

pub fn emit(events: &Option<Sender<RuntimeEvent>>, event: RuntimeEvent) {
    if let Some(sender) = events {
        let _ = sender.send(event);
    } else {
        match event {
            RuntimeEvent::Activity(text) => println!("  {text}"),
            RuntimeEvent::Assistant(text) => println!("{text}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeRequest {
    pub prompt: String,
    pub repository: String,
    pub read_only: bool,
    pub provider_session_id: Option<String>,
    pub openshell: Option<crate::openshell::OpenShell>,
    pub events: Option<Sender<RuntimeEvent>>,
}

#[derive(Clone, Debug, Default)]
pub struct RuntimeUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_usd: Option<f64>,
}

#[derive(Clone, Debug)]
pub struct RuntimeResponse {
    pub output: String,
    pub success: bool,
    pub provider_session_id: Option<String>,
    pub usage: RuntimeUsage,
}

pub trait ModelRuntime: Send + Sync {
    fn descriptor(&self) -> RuntimeDescriptor;
    fn run(&self, request: &RuntimeRequest) -> Result<RuntimeResponse>;
}

pub struct RuntimeRegistry {
    runtimes: BTreeMap<String, Box<dyn ModelRuntime>>,
}

impl RuntimeRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self {
            runtimes: BTreeMap::new(),
        };
        registry.register(Box::new(ClaudeCodeHeadlessRuntime::from_environment()));
        registry.register(Box::new(
            crate::codex_runtime::CodexHeadlessRuntime::from_environment(),
        ));
        registry.register(Box::new(crate::api_runtime::ApiRuntime::anthropic()));
        registry.register(Box::new(crate::api_runtime::ApiRuntime::openai()));
        registry
    }
    pub fn register(&mut self, runtime: Box<dyn ModelRuntime>) {
        self.runtimes
            .insert(runtime.descriptor().id.clone(), runtime);
    }
    pub fn resolve(&self, requested: &str) -> Result<&dyn ModelRuntime> {
        self.runtimes
            .get(requested)
            .map(|runtime| runtime.as_ref())
            .with_context(|| format!("runtime `{requested}` is not registered"))
    }
    pub fn descriptors(&self) -> Vec<RuntimeDescriptor> {
        self.runtimes
            .values()
            .map(|runtime| runtime.descriptor())
            .collect()
    }
}

pub struct ClaudeCodeHeadlessRuntime {
    binary: String,
}

impl ClaudeCodeHeadlessRuntime {
    pub fn from_environment() -> Self {
        Self {
            binary: std::env::var("NEXUS_CLAUDE_CODE_BIN").unwrap_or_else(|_| "claude".into()),
        }
    }
    fn command(&self, request: &RuntimeRequest) -> Result<Command> {
        let mut command = if let Some(sandbox) = &request.openshell {
            sandbox.exec_command(
                &["env", "NEXUSMIND_MCP_TOOL_PROFILE=essential", &self.binary],
                false,
                None,
            )?
        } else {
            Command::new(&self.binary)
        };
        let allowed = [
            "HOME",
            "PATH",
            "USER",
            "LOGNAME",
            "LANG",
            "LC_ALL",
            "TMPDIR",
            "TERM",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "SSL_CERT_FILE",
            "NODE_EXTRA_CA_CERTS",
            "NEXUSMIND_API_KEY",
            "NEXUSMIND_BASE_URL",
            "ANTHROPIC_API_KEY",
        ];
        let values = allowed
            .iter()
            .filter_map(|key| std::env::var_os(key).map(|value| (*key, value)))
            .collect::<Vec<_>>();
        command.env_clear();
        for (key, value) in values {
            command.env(key, value);
        }
        command.env("DISABLE_AUTOUPDATER", "1");
        command.env("NEXUSMIND_MCP_TOOL_PROFILE", "essential");
        Ok(command)
    }
}

impl ModelRuntime for ClaudeCodeHeadlessRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            id: "claude-code-headless".into(),
            provider: "anthropic".into(),
            model: None,
            execution_mode: "headless_cli".into(),
            capabilities: RuntimeCapabilities {
                streaming: true,
                tool_calling: true,
                native_file_editing: true,
                shell_execution: true,
                session_resume: true,
                subagents: true,
                structured_output: true,
                long_running_execution: true,
            },
        }
    }

    fn run(&self, request: &RuntimeRequest) -> Result<RuntimeResponse> {
        if !Path::new(&request.repository).is_dir() {
            bail!("repository path does not exist: {}", request.repository);
        }
        let mut command = self.command(request)?;
        command.current_dir(&request.repository).args([
            "--print",
            "--verbose",
            "--include-partial-messages",
            "--output-format",
            "stream-json",
        ]);
        if request.read_only {
            command.args(["--permission-mode", "plan"]);
        } else {
            command.args(["--permission-mode", "auto"]);
        }
        if let Some(session_id) = &request.provider_session_id {
            command.args(["--resume", session_id]);
        }
        if let Ok(budget) = std::env::var("NEXUS_MAX_TASK_COST_USD") {
            let amount: f64 = budget
                .parse()
                .context("NEXUS_MAX_TASK_COST_USD must be a number")?;
            if !amount.is_finite() || amount <= 0.0 {
                bail!("NEXUS_MAX_TASK_COST_USD must be positive");
            }
            command.args(["--max-budget-usd", &budget]);
        }
        command
            .arg(&request.prompt)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("could not start Claude Code using `{}`", self.binary))?;
        let stdout = child
            .stdout
            .take()
            .context("Claude Code stdout was not piped")?;
        let stderr = child
            .stderr
            .take()
            .context("Claude Code stderr was not piped")?;
        let stderr_task = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(256 * 1024).read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).to_string()
        });
        let mut result_text = String::new();
        let mut assistant_text = String::new();
        let mut saw_delta = false;
        let mut session_id = request.provider_session_id.clone();
        let mut usage = RuntimeUsage::default();
        let mut event_error = false;
        for line in BufReader::new(stdout).lines() {
            let line = line.context("reading Claude Code event stream")?;
            if line.len() > 2 * 1024 * 1024 {
                child.kill().ok();
                bail!("Claude Code event exceeds 2 MiB");
            }
            let Ok(event) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if let Some(id) = event.get("session_id").and_then(Value::as_str) {
                session_id = Some(id.to_string());
            }
            match event.get("type").and_then(Value::as_str) {
                Some("stream_event") => {
                    let delta = event.pointer("/event/delta");
                    if let Some(text) = delta.and_then(|d| d.get("text")).and_then(Value::as_str) {
                        if delta.and_then(|d| d.get("type")).and_then(Value::as_str)
                            == Some("text_delta")
                        {
                            if request.events.is_none() {
                                print!("{text}");
                                std::io::stdout().flush().ok();
                            }
                            saw_delta = true;
                        }
                    }
                }
                Some("assistant") => {
                    if let Some(content) =
                        event.pointer("/message/content").and_then(Value::as_array)
                    {
                        for block in content {
                            if block.get("type").and_then(Value::as_str) == Some("text") {
                                if let Some(text) = block.get("text").and_then(Value::as_str) {
                                    assistant_text.push_str(text);
                                    assistant_text.push('\n');
                                    emit(
                                        &request.events,
                                        RuntimeEvent::Assistant(crate::redacted(text)),
                                    );
                                }
                            } else if block.get("type").and_then(Value::as_str) == Some("tool_use")
                            {
                                let name =
                                    block.get("name").and_then(Value::as_str).unwrap_or("tool");
                                let detail = block
                                    .pointer("/input/command")
                                    .and_then(Value::as_str)
                                    .or_else(|| {
                                        block.pointer("/input/file_path").and_then(Value::as_str)
                                    })
                                    .unwrap_or("");
                                emit(
                                    &request.events,
                                    RuntimeEvent::Activity(crate::redacted(&format!(
                                        "{name}: {detail}"
                                    ))),
                                );
                            }
                        }
                    }
                }
                Some("user") => {
                    if let Some(content) =
                        event.pointer("/message/content").and_then(Value::as_array)
                    {
                        for block in content {
                            if block.get("type").and_then(Value::as_str) == Some("tool_result") {
                                let output = block
                                    .get("content")
                                    .and_then(Value::as_str)
                                    .or_else(|| {
                                        block.pointer("/content/0/text").and_then(Value::as_str)
                                    })
                                    .unwrap_or("");
                                if !output.is_empty() {
                                    emit(
                                        &request.events,
                                        RuntimeEvent::Activity(crate::redacted(output)),
                                    );
                                }
                            }
                        }
                    }
                }
                Some("result") => {
                    result_text = event
                        .get("result")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    event_error = event
                        .get("is_error")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    usage.cost_usd = event.get("total_cost_usd").and_then(Value::as_f64);
                    usage.input_tokens =
                        event.pointer("/usage/input_tokens").and_then(Value::as_u64);
                    usage.output_tokens = event
                        .pointer("/usage/output_tokens")
                        .and_then(Value::as_u64);
                }
                _ => {}
            }
        }
        let status = child.wait().context("waiting for Claude Code")?;
        let stderr = stderr_task.join().unwrap_or_default();
        if saw_delta && request.events.is_none() {
            println!();
        }
        if !status.success() && !stderr.trim().is_empty() {
            emit(
                &request.events,
                RuntimeEvent::Activity(crate::redacted(&format!("Claude Code: {}", stderr.trim()))),
            );
        }
        let output = if !result_text.is_empty() {
            result_text
        } else {
            assistant_text.trim().to_string()
        };
        Ok(RuntimeResponse {
            output,
            success: status.success() && !event_error,
            provider_session_id: session_id,
            usage,
        })
    }
}
