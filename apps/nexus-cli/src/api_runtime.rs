//! Direct Anthropic Messages and OpenAI Responses adapters.
//! All tool execution is delegated to the required OpenShell sandbox.
use crate::{
    model::{RuntimeCapabilities, RuntimeDescriptor},
    runtime::{ModelRuntime, RuntimeRequest, RuntimeResponse, RuntimeUsage},
};
use anyhow::{bail, Context, Result};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::{env, time::Duration};

const MAX_TOOL_CALLS: usize = 20;
const MAX_TOOL_OUTPUT: usize = 16_000;

#[derive(Clone, Copy)]
pub enum ApiProvider {
    Anthropic,
    OpenAI,
}

pub struct ApiRuntime {
    provider: ApiProvider,
    model: Option<String>,
}

impl ApiRuntime {
    pub fn anthropic() -> Self {
        Self {
            provider: ApiProvider::Anthropic,
            model: env::var("NEXUS_CLAUDE_MODEL").ok(),
        }
    }
    pub fn openai() -> Self {
        Self {
            provider: ApiProvider::OpenAI,
            model: env::var("NEXUS_OPENAI_MODEL").ok(),
        }
    }

    fn model(&self) -> Result<&str> {
        self.model
            .as_deref()
            .filter(|model| !model.trim().is_empty())
            .context(match self.provider {
                ApiProvider::Anthropic => "set NEXUS_CLAUDE_MODEL to use the Claude API runtime",
                ApiProvider::OpenAI => "set NEXUS_OPENAI_MODEL to use the OpenAI API runtime",
            })
    }
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?)
}

fn api_response(request: reqwest::blocking::RequestBuilder) -> Result<Value> {
    let response = request.send().context("model API request failed")?;
    let status = response.status();
    let body = response
        .bytes()
        .context("could not read model API response")?;
    if !status.is_success() {
        let excerpt = String::from_utf8_lossy(&body[..body.len().min(2_000)]);
        bail!("model API returned HTTP {}: {}", status.as_u16(), excerpt);
    }
    serde_json::from_slice(&body).context("model API returned invalid JSON")
}

fn shell_tool(request: &RuntimeRequest, arguments: &Value) -> Result<String> {
    let sandbox = request
        .openshell
        .as_ref()
        .context("OpenShell is required for API tools")?;
    let command = arguments
        .get("command")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .context("shell tool requires a nonempty command")?;
    if command.len() > 10_000 {
        bail!("shell command exceeds 10 KiB");
    }
    crate::runtime::emit(
        &request.events,
        crate::runtime::RuntimeEvent::Activity(crate::redacted(&format!("$ {command}"))),
    );
    let timeout = arguments
        .get("timeout_seconds")
        .and_then(Value::as_u64)
        .unwrap_or(60)
        .clamp(1, 120);
    let output = sandbox.command_output(&["sh", "-lc", command], Some(timeout))?;
    let mut combined = format!("exit_status: {}\n", output.status);
    combined.push_str(&String::from_utf8_lossy(&output.stdout));
    combined.push_str(&String::from_utf8_lossy(&output.stderr));
    if combined.len() > MAX_TOOL_OUTPUT {
        combined = combined.chars().take(MAX_TOOL_OUTPUT).collect();
    }
    crate::runtime::emit(
        &request.events,
        crate::runtime::RuntimeEvent::Activity(crate::redacted(&combined)),
    );
    Ok(combined)
}

fn tool_schema_anthropic() -> Value {
    json!({"name":"shell","description":"Run a command in the OpenShell repository sandbox. Use for reading files, editing code, and tests.",
        "input_schema":{"type":"object","properties":{"command":{"type":"string"},"timeout_seconds":{"type":"integer"}},"required":["command"]}})
}

fn tool_schema_openai() -> Value {
    json!({"type":"function","name":"shell","description":"Run a command in the OpenShell repository sandbox. Use for reading files, editing code, and tests.",
        "parameters":{"type":"object","properties":{"command":{"type":"string"},"timeout_seconds":{"type":"integer"}},"required":["command","timeout_seconds"],"additionalProperties":false},"strict":true})
}

impl ModelRuntime for ApiRuntime {
    fn descriptor(&self) -> RuntimeDescriptor {
        let (id, provider, resume) = match self.provider {
            ApiProvider::Anthropic => ("claude-api", "anthropic", false),
            ApiProvider::OpenAI => ("openai-api", "openai", true),
        };
        RuntimeDescriptor {
            id: id.into(),
            provider: provider.into(),
            model: self.model.clone(),
            execution_mode: "api".into(),
            capabilities: RuntimeCapabilities {
                streaming: false,
                tool_calling: true,
                native_file_editing: false,
                shell_execution: true,
                session_resume: resume,
                subagents: false,
                structured_output: false,
                long_running_execution: true,
            },
        }
    }

    fn run(&self, request: &RuntimeRequest) -> Result<RuntimeResponse> {
        if request.openshell.is_none() {
            bail!("OpenShell is required for direct API runtimes");
        }
        let model = self.model()?;
        match self.provider {
            ApiProvider::Anthropic => run_anthropic(request, model),
            ApiProvider::OpenAI => run_openai(request, model),
        }
    }
}

fn run_anthropic(request: &RuntimeRequest, model: &str) -> Result<RuntimeResponse> {
    let key = env::var("ANTHROPIC_API_KEY").context("set ANTHROPIC_API_KEY for Claude API")?;
    let http = client()?;
    let mut messages = vec![json!({"role":"user","content":request.prompt})];
    let mut usage = RuntimeUsage::default();
    let mut final_text = String::new();
    for iteration in 0..=MAX_TOOL_CALLS {
        let mut body = json!({"model":model,"max_tokens":4096,"messages":messages});
        if !request.read_only {
            body["tools"] = json!([tool_schema_anthropic()]);
            body["tool_choice"] = json!({"type":"auto","disable_parallel_tool_use":true});
        }
        let response = api_response(
            http.post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .json(&body),
        )?;
        usage.input_tokens = Some(
            usage.input_tokens.unwrap_or(0)
                + response
                    .pointer("/usage/input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
        );
        usage.output_tokens = Some(
            usage.output_tokens.unwrap_or(0)
                + response
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
        );
        let content = response
            .get("content")
            .and_then(Value::as_array)
            .context("Claude response has no content blocks")?;
        let mut results = vec![];
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        final_text.push_str(text);
                        final_text.push('\n');
                    }
                }
                Some("tool_use") if !request.read_only => {
                    let id = block
                        .get("id")
                        .and_then(Value::as_str)
                        .context("Claude tool has no id")?;
                    let name = block
                        .get("name")
                        .and_then(Value::as_str)
                        .context("Claude tool has no name")?;
                    if name != "shell" {
                        bail!("Claude requested unknown tool: {name}");
                    }
                    let args = block.get("input").context("Claude tool has no input")?;
                    let (output, is_error) = match shell_tool(request, args) {
                        Ok(output) => (output, false),
                        Err(error) => (format!("{error:#}"), true),
                    };
                    results.push(json!({"type":"tool_result","tool_use_id":id,"content":output,"is_error":is_error}));
                }
                _ => {}
            }
        }
        if results.is_empty() {
            let success = response.get("stop_reason").and_then(Value::as_str) != Some("max_tokens");
            return Ok(RuntimeResponse {
                output: final_text.trim().into(),
                success,
                provider_session_id: None,
                usage,
            });
        }
        if iteration == MAX_TOOL_CALLS {
            bail!("Claude API exceeded the tool-call limit");
        }
        messages.push(json!({"role":"assistant","content":content}));
        messages.push(json!({"role":"user","content":results}));
    }
    unreachable!()
}

fn run_openai(request: &RuntimeRequest, model: &str) -> Result<RuntimeResponse> {
    let key = env::var("OPENAI_API_KEY").context("set OPENAI_API_KEY for OpenAI API")?;
    let http = client()?;
    let mut previous = request.provider_session_id.clone();
    let mut input = json!(request.prompt);
    let mut usage = RuntimeUsage::default();
    let mut final_text = String::new();
    for iteration in 0..=MAX_TOOL_CALLS {
        let mut body =
            json!({"model":model,"input":input,"store":true,"parallel_tool_calls":false});
        if let Some(id) = &previous {
            body["previous_response_id"] = json!(id);
        }
        if !request.read_only {
            body["tools"] = json!([tool_schema_openai()]);
        }
        let response = api_response(
            http.post("https://api.openai.com/v1/responses")
                .bearer_auth(&key)
                .json(&body),
        )?;
        previous = Some(
            response
                .get("id")
                .and_then(Value::as_str)
                .context("OpenAI response has no id")?
                .to_string(),
        );
        usage.input_tokens = Some(
            usage.input_tokens.unwrap_or(0)
                + response
                    .pointer("/usage/input_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
        );
        usage.output_tokens = Some(
            usage.output_tokens.unwrap_or(0)
                + response
                    .pointer("/usage/output_tokens")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
        );
        let output = response
            .get("output")
            .and_then(Value::as_array)
            .context("OpenAI response has no output items")?;
        let mut results = vec![];
        for item in output {
            match item.get("type").and_then(Value::as_str) {
                Some("message") => {
                    if let Some(parts) = item.get("content").and_then(Value::as_array) {
                        for part in parts {
                            if part.get("type").and_then(Value::as_str) == Some("output_text") {
                                if let Some(text) = part.get("text").and_then(Value::as_str) {
                                    final_text.push_str(text);
                                    final_text.push('\n');
                                }
                            }
                        }
                    }
                }
                Some("function_call") if !request.read_only => {
                    let name = item
                        .get("name")
                        .and_then(Value::as_str)
                        .context("OpenAI function has no name")?;
                    if name != "shell" {
                        bail!("OpenAI requested unknown function: {name}");
                    }
                    let call_id = item
                        .get("call_id")
                        .and_then(Value::as_str)
                        .context("OpenAI function has no call_id")?;
                    let arguments: Value = serde_json::from_str(
                        item.get("arguments")
                            .and_then(Value::as_str)
                            .context("OpenAI function has no arguments")?,
                    )?;
                    let result = shell_tool(request, &arguments)
                        .unwrap_or_else(|error| format!("{error:#}"));
                    results.push(
                        json!({"type":"function_call_output","call_id":call_id,"output":result}),
                    );
                }
                _ => {}
            }
        }
        if results.is_empty() {
            let success = response.get("status").and_then(Value::as_str) == Some("completed");
            return Ok(RuntimeResponse {
                output: final_text.trim().into(),
                success,
                provider_session_id: previous,
                usage,
            });
        }
        if iteration == MAX_TOOL_CALLS {
            bail!("OpenAI API exceeded the tool-call limit");
        }
        input = json!(results);
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn api_runtime_descriptors_are_distinct() {
        assert_eq!(ApiRuntime::anthropic().descriptor().id, "claude-api");
        assert_eq!(ApiRuntime::openai().descriptor().id, "openai-api");
    }
    #[test]
    fn api_tools_require_openshell() {
        let request = RuntimeRequest {
            prompt: "task".into(),
            repository: "/tmp".into(),
            read_only: false,
            provider_session_id: None,
            openshell: None,
            events: None,
        };
        assert!(shell_tool(&request, &json!({"command":"pwd"})).is_err());
    }
}
