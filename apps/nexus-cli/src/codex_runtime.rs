use crate::model::{RuntimeCapabilities, RuntimeDescriptor};
use crate::runtime::{
    emit, ModelRuntime, RuntimeEvent, RuntimeRequest, RuntimeResponse, RuntimeUsage,
};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
};
use uuid::Uuid;

const STDERR_LIMIT: usize = 4 * 1024;
const EVENT_LIMIT: usize = 8 * 1024 * 1024;

/// Runs one Codex turn inside the repository selected by the Nexus harness.
/// The harness owns retries, verification and the decision to finish a task.
pub struct CodexHeadlessRuntime {
    binary: String,
    model: Option<String>,
}

impl CodexHeadlessRuntime {
    pub fn from_environment() -> Self {
        Self {
            binary: std::env::var("NEXUS_CODEX_BIN")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "codex".into()),
            model: std::env::var("NEXUS_CODEX_MODEL")
                .ok()
                .filter(|value| !value.trim().is_empty()),
        }
    }

    fn command(&self, request: &RuntimeRequest) -> Result<Command> {
        if !Path::new(&request.repository).is_dir() {
            bail!("repository path does not exist: {}", request.repository);
        }

        let mut command = if let Some(sandbox) = &request.openshell {
            sandbox.exec_command(
                &[
                    "env",
                    "CODEX_HOME=/sandbox/.nexus-codex-auth",
                    "NEXUSMIND_MCP_TOOL_PROFILE=essential",
                    &self.binary,
                ],
                false,
                None,
            )?
        } else {
            Command::new(&self.binary)
        };
        command.current_dir(&request.repository);
        command.env_clear();

        // Codex can authenticate from its saved login under HOME/CODEX_HOME.
        // An explicitly supplied key is only given to this child process, and
        // Codex is told to filter KEY/SECRET/TOKEN variables from tool shells.
        const ALLOWED_ENV: &[&str] = &[
            "HOME",
            "PATH",
            "USER",
            "LOGNAME",
            "LANG",
            "LC_ALL",
            "TMPDIR",
            "TERM",
            "NO_COLOR",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "NO_PROXY",
            "SSL_CERT_FILE",
            "CODEX_HOME",
            "CODEX_API_KEY",
            "CODEX_ACCESS_TOKEN",
        ];
        for key in ALLOWED_ENV {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }

        command.args(["exec"]);
        // OpenShell is the outer filesystem/network boundary. Codex's nested
        // Linux sandbox needs user namespaces that the OpenShell container
        // does not grant, so disable that inner layer only for this runner.
        let sandbox = if request.openshell.is_some() {
            "danger-full-access"
        } else if request.read_only {
            "read-only"
        } else {
            "workspace-write"
        };

        if let Some(session_id) = request.provider_session_id.as_deref() {
            // Never select `--last`: that could resume somebody else's task.
            Uuid::parse_str(session_id).context("invalid Codex session id")?;
            command.args(["resume", "--json"]);
            command.args(["-c", &format!("sandbox_mode={sandbox}")]);
            command.args(["-c", "approval_policy=never"]);
            command.args([
                "-c",
                "shell_environment_policy.ignore_default_excludes=false",
            ]);
            if let Some(model) = &self.model {
                command.args(["--model", model]);
            }
            command.args(["--", session_id, "-"]);
        } else {
            command.args(["--json", "--sandbox", sandbox]);
            command.args(["-c", "approval_policy=never"]);
            command.args([
                "-c",
                "shell_environment_policy.ignore_default_excludes=false",
            ]);
            let workdir = request
                .openshell
                .as_ref()
                .map(|sandbox| sandbox.workspace())
                .unwrap_or(&request.repository);
            command.args(["--cd", workdir]);
            if let Some(model) = &self.model {
                command.args(["--model", model]);
            }
            command.args(["--", "-"]);
        }

        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        Ok(command)
    }
}

impl ModelRuntime for CodexHeadlessRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        RuntimeDescriptor {
            id: "codex-headless".into(),
            provider: "openai".into(),
            model: self.model.clone(),
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
        let mut child = self
            .command(request)?
            .spawn()
            .with_context(|| format!("could not start Codex using `{}`", self.binary))?;

        let stderr = child
            .stderr
            .take()
            .context("Codex stderr was not captured")?;
        let stderr_reader = thread::spawn(move || capture_stderr(stderr));

        let write_result = child
            .stdin
            .take()
            .context("Codex stdin was not captured")?
            .write_all(request.prompt.as_bytes());
        if let Err(error) = write_result {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stderr_reader.join();
            return Err(error).context("could not send task to Codex");
        }

        let stdout = child
            .stdout
            .take()
            .context("Codex stdout was not captured")?;
        let mut reader = BufReader::new(stdout);
        let mut event_state = EventState {
            session_id: request.provider_session_id.clone(),
            ..EventState::default()
        };
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => break,
                Ok(_) => {
                    if line.len() > EVENT_LIMIT {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = stderr_reader.join();
                        bail!("Codex event exceeds 8 MiB");
                    }
                    if let Ok(event) = serde_json::from_str::<Value>(&line) {
                        if let Some(progress) = event_state.observe(&event) {
                            emit(&request.events, RuntimeEvent::Activity(progress.into()));
                        }
                        if let Some(activity) = visible_item_event(&event) {
                            emit(
                                &request.events,
                                RuntimeEvent::Activity(crate::redacted(&activity)),
                            );
                        }
                    } else {
                        event_state.invalid_events = event_state.invalid_events.saturating_add(1);
                    }
                }
                Err(error) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stderr_reader.join();
                    return Err(error).context("could not read Codex event stream");
                }
            }
        }

        let status = child.wait().context("could not wait for Codex")?;
        let stderr = stderr_reader.join().unwrap_or_default();
        let secret_values = known_secret_values();
        let mut output = event_state.final_message.unwrap_or_default();
        if !status.success() || event_state.turn_failed || !event_state.turn_completed {
            if let Some(error) = event_state.error_message {
                append_error(&mut output, &error);
            } else if !stderr.trim().is_empty() {
                append_error(&mut output, stderr.trim());
            } else if output.is_empty() {
                append_error(
                    &mut output,
                    &format!("Codex stopped without a completed turn ({status})"),
                );
            }
        }
        if event_state.invalid_events > 0 && !event_state.turn_completed {
            append_error(&mut output, "Codex returned an invalid event stream");
        }

        let output = redact_known_secrets(&output, &secret_values);
        if !output.trim().is_empty() {
            emit(&request.events, RuntimeEvent::Assistant(output.clone()));
        }

        Ok(RuntimeResponse {
            output,
            success: status.success() && event_state.turn_completed && !event_state.turn_failed,
            provider_session_id: event_state.session_id,
            usage: RuntimeUsage {
                input_tokens: event_state.input_tokens,
                output_tokens: event_state.output_tokens,
                cost_usd: None,
            },
        })
    }
}

#[derive(Default)]
struct EventState {
    session_id: Option<String>,
    final_message: Option<String>,
    error_message: Option<String>,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    turn_completed: bool,
    turn_failed: bool,
    invalid_events: u32,
}

impl EventState {
    fn observe(&mut self, event: &Value) -> Option<&'static str> {
        match event.get("type")?.as_str()? {
            "thread.started" => {
                if let Some(id) = event.get("thread_id").and_then(Value::as_str) {
                    if Uuid::parse_str(id).is_ok() {
                        self.session_id = Some(id.to_owned());
                    }
                }
                None
            }
            "turn.started" => Some("Codex está trabajando…"),
            "item.started" => match event
                .get("item")
                .and_then(|item| item.get("type"))
                .and_then(Value::as_str)
            {
                Some("command_execution") => Some("Ejecutando comando…"),
                Some("file_change") => Some("Editando archivos…"),
                Some("mcp_tool_call") => Some("Consultando herramienta…"),
                _ => None,
            },
            "item.completed" => {
                let item = event.get("item")?;
                if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                    if let Some(text) = item.get("text").and_then(Value::as_str) {
                        self.final_message = Some(text.to_owned());
                    }
                }
                None
            }
            "turn.completed" => {
                self.turn_completed = true;
                if let Some(usage) = event.get("usage") {
                    accumulate_tokens(&mut self.input_tokens, usage.get("input_tokens"));
                    accumulate_tokens(&mut self.output_tokens, usage.get("output_tokens"));
                }
                None
            }
            "turn.failed" => {
                self.turn_failed = true;
                self.error_message = event
                    .pointer("/error/message")
                    .or_else(|| event.get("message"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                Some("Codex detuvo el turno.")
            }
            "error" => {
                self.error_message = event
                    .get("message")
                    .or_else(|| event.pointer("/error/message"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned);
                None
            }
            _ => None,
        }
    }
}

fn visible_item_event(event: &Value) -> Option<String> {
    let kind = event.get("type")?.as_str()?;
    let item = event.get("item")?;
    match (kind, item.get("type")?.as_str()?) {
        ("item.started", "command_execution") => {
            Some(format!("$ {}", item.get("command")?.as_str()?))
        }
        ("item.completed", "command_execution") => {
            let output = item
                .get("aggregated_output")
                .or_else(|| item.get("output"))
                .and_then(Value::as_str)
                .unwrap_or("");
            let code = item
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| format!("exit {code}"))
                .unwrap_or_else(|| "command finished".into());
            Some(format!(
                "{code}\n{}",
                output.chars().take(16_000).collect::<String>()
            ))
        }
        ("item.completed", "file_change") => Some(format!(
            "Files changed: {}",
            item.get("changes")
                .map(Value::to_string)
                .unwrap_or_default()
        )),
        ("item.started", "mcp_tool_call") => Some(format!(
            "Tool: {}",
            item.get("tool").and_then(Value::as_str).unwrap_or("MCP")
        )),
        _ => None,
    }
}

fn accumulate_tokens(total: &mut Option<u64>, value: Option<&Value>) {
    if let Some(count) = value.and_then(Value::as_u64) {
        *total = Some(total.unwrap_or(0).saturating_add(count));
    }
}

fn capture_stderr(mut stderr: impl Read) -> String {
    let mut captured = Vec::new();
    let mut chunk = [0u8; 1024];
    loop {
        match stderr.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(count) if captured.len() < STDERR_LIMIT => {
                let remaining = STDERR_LIMIT - captured.len();
                captured.extend_from_slice(&chunk[..count.min(remaining)]);
            }
            Ok(_) => {}
        }
    }
    String::from_utf8_lossy(&captured).into_owned()
}

fn append_error(output: &mut String, error: &str) {
    if !output.is_empty() {
        output.push_str("\n\n");
    }
    output.push_str("[Codex] ");
    output.push_str(error);
}

fn known_secret_values() -> Vec<String> {
    [
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "OPENAI_API_KEY",
        "NEXUSMIND_API_KEY",
    ]
    .iter()
    .filter_map(|key| std::env::var(key).ok())
    .filter(|value| !value.is_empty())
    .collect()
}

fn redact_known_secrets(text: &str, secrets: &[String]) -> String {
    secrets.iter().fold(text.to_owned(), |output, secret| {
        output.replace(secret, "[redacted]")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_session_message_and_usage_from_jsonl_events() {
        let id = "0199a213-81c0-7800-8aa1-bbab2a035a53";
        let mut state = EventState::default();
        state.observe(&json!({"type":"thread.started","thread_id":id}));
        state.observe(&json!({"type":"turn.started"}));
        state.observe(
            &json!({"type":"item.completed","item":{"type":"agent_message","text":"Listo"}}),
        );
        state.observe(
            &json!({"type":"turn.completed","usage":{"input_tokens":32,"output_tokens":7}}),
        );
        assert_eq!(state.session_id.as_deref(), Some(id));
        assert_eq!(state.final_message.as_deref(), Some("Listo"));
        assert_eq!(state.input_tokens, Some(32));
        assert_eq!(state.output_tokens, Some(7));
        assert!(state.turn_completed);
    }

    #[test]
    fn shows_command_and_output_from_codex_events() {
        let started =
            json!({"type":"item.started","item":{"type":"command_execution","command":"pwd"}});
        let completed = json!({"type":"item.completed","item":{"type":"command_execution","aggregated_output":"/workspace\n","exit_code":0}});
        assert_eq!(visible_item_event(&started).as_deref(), Some("$ pwd"));
        assert_eq!(
            visible_item_event(&completed).as_deref(),
            Some("exit 0\n/workspace\n")
        );
    }

    #[test]
    fn forces_sandbox_again_when_resuming() {
        let runtime = CodexHeadlessRuntime {
            binary: "codex".into(),
            model: None,
        };
        let request = RuntimeRequest {
            prompt: "continue".into(),
            repository: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            read_only: true,
            provider_session_id: Some("0199a213-81c0-7800-8aa1-bbab2a035a53".into()),
            openshell: None,
            events: None,
        };
        let command = runtime.command(&request).unwrap();
        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c", "sandbox_mode=read-only"]));
        assert!(args
            .windows(2)
            .any(|pair| pair == ["-c", "approval_policy=never"]));
        assert!(!args.iter().any(|arg| arg == "--last"));
    }

    #[test]
    fn redacts_token_values_from_error_text() {
        assert_eq!(
            redact_known_secrets("failed: secret-token", &["secret-token".into()]),
            "failed: [redacted]"
        );
    }

    #[test]
    fn refuses_non_uuid_session_selector() {
        let runtime = CodexHeadlessRuntime {
            binary: "codex".into(),
            model: None,
        };
        let request = RuntimeRequest {
            prompt: "continue".into(),
            repository: std::env::current_dir()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            read_only: true,
            provider_session_id: Some("last".into()),
            openshell: None,
            events: None,
        };
        assert!(runtime.command(&request).is_err());
    }
}
