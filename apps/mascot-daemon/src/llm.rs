//! Direct API System 2 client supporting multi-turn **tool calling** across
//! Claude (Anthropic), Gemini (Google), and OpenAI (plus configurable base_url for testing/proxies).

use crate::config::{LlmConfig, LlmProviderKind, ModelTier};
use anyhow::{anyhow, bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
pub enum AgentMessage {
    User {
        text: String,
    },
    Assistant {
        text: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        tool_name: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LlmTurnResponse {
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub provider: String,
    pub model: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlannedReply {
    pub text: String,
    pub provider: String,
    pub model: String,
}

#[derive(Clone)]
pub struct LlmClient {
    http: Client,
    config: LlmConfig,
}

impl LlmClient {
    pub fn new(config: LlmConfig) -> Result<Self> {
        let http = Client::builder().timeout(Duration::from_secs(60)).build()?;
        Ok(Self { http, config })
    }

    pub fn configured(&self) -> bool {
        self.api_key().is_some()
    }

    pub fn provider_name(&self) -> &'static str {
        match self.config.provider {
            LlmProviderKind::Claude => "claude",
            LlmProviderKind::Gemini => "gemini",
            LlmProviderKind::OpenAi => "openai",
        }
    }

    pub fn model_name(&self) -> &str {
        &self.config.model
    }

    pub fn model_for_tier(&self, tier: ModelTier) -> &str {
        self.config.model_for_tier(tier)
    }

    pub fn auto_escalate_enabled(&self) -> bool {
        self.config.auto_escalate_model
    }

    fn api_key(&self) -> Option<String> {
        if let Some(inline) = self
            .config
            .api_key_override
            .as_deref()
            .filter(|v| !v.trim().is_empty())
        {
            return Some(inline.trim().to_string());
        }
        std::env::var(&self.config.api_key_env)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    pub async fn complete(&self, system_prompt: &str, user_prompt: &str) -> Result<PlannedReply> {
        let turn = self
            .chat_turn(
                system_prompt,
                &[AgentMessage::User {
                    text: user_prompt.to_string(),
                }],
                &[],
            )
            .await?;
        Ok(PlannedReply {
            text: turn.text,
            provider: turn.provider,
            model: turn.model,
        })
    }

    pub async fn chat_turn(
        &self,
        system_prompt: &str,
        messages: &[AgentMessage],
        tools: &[ToolSpec],
    ) -> Result<LlmTurnResponse> {
        self.chat_turn_with_model(None, system_prompt, messages, tools)
            .await
    }

    pub async fn chat_turn_with_model(
        &self,
        model_override: Option<&str>,
        system_prompt: &str,
        messages: &[AgentMessage],
        tools: &[ToolSpec],
    ) -> Result<LlmTurnResponse> {
        let key = self.api_key().ok_or_else(|| {
            anyhow!(
                "System 2 API key missing; set environment variable {}",
                self.config.api_key_env
            )
        })?;
        let model = model_override
            .filter(|m| !m.trim().is_empty())
            .unwrap_or(&self.config.model);

        match self.config.provider {
            LlmProviderKind::Claude => {
                self.call_claude(&key, model, system_prompt, messages, tools)
                    .await
            }
            LlmProviderKind::Gemini => {
                self.call_gemini(&key, model, system_prompt, messages, tools)
                    .await
            }
            LlmProviderKind::OpenAi => {
                self.call_openai(&key, model, system_prompt, messages, tools)
                    .await
            }
        }
    }

    async fn call_claude(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[AgentMessage],
        tools: &[ToolSpec],
    ) -> Result<LlmTurnResponse> {
        let url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.anthropic.com/v1/messages".into());

        let mut wire_messages = Vec::new();
        for msg in messages {
            match msg {
                AgentMessage::User { text } => {
                    wire_messages.push(json!({ "role": "user", "content": text }));
                }
                AgentMessage::Assistant { text, tool_calls } => {
                    let mut blocks = Vec::new();
                    if !text.is_empty() {
                        blocks.push(json!({ "type": "text", "text": text }));
                    }
                    for tc in tool_calls {
                        blocks.push(json!({
                            "type": "tool_use",
                            "id": tc.id,
                            "name": tc.name,
                            "input": tc.arguments
                        }));
                    }
                    wire_messages.push(json!({ "role": "assistant", "content": blocks }));
                }
                AgentMessage::ToolResult {
                    tool_call_id,
                    content,
                    is_error,
                    ..
                } => {
                    wire_messages.push(json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": content,
                            "is_error": is_error
                        }]
                    }));
                }
            }
        }

        let mut body = json!({
            "model": model,
            "max_tokens": self.config.max_tokens,
            "system": system_prompt,
            "messages": wire_messages
        });
        if !tools.is_empty() {
            let wire_tools: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema
                    })
                })
                .collect();
            body["tools"] = Value::Array(wire_tools);
        }

        let resp = self
            .http
            .post(&url)
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&body)
            .send()
            .await
            .context("Anthropic request failed")?;
        if !resp.status().is_success() {
            bail!("Anthropic returned HTTP {}", resp.status().as_u16());
        }
        let val: Value = resp.json().await?;

        let mut text_parts = Vec::new();
        let mut calls = Vec::new();
        if let Some(arr) = val.get("content").and_then(Value::as_array) {
            for block in arr {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(t) = block.get("text").and_then(Value::as_str) {
                            text_parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        let id = block
                            .get("id")
                            .and_then(Value::as_str)
                            .unwrap_or("tool_call")
                            .to_string();
                        let name = block
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let input = block.get("input").cloned().unwrap_or_else(|| json!({}));
                        if !name.is_empty() {
                            calls.push(ToolCall {
                                id,
                                name,
                                arguments: input,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }

        Ok(LlmTurnResponse {
            text: text_parts.join("\n"),
            tool_calls: calls,
            provider: "claude".into(),
            model: model.to_string(),
        })
    }

    async fn call_gemini(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[AgentMessage],
        tools: &[ToolSpec],
    ) -> Result<LlmTurnResponse> {
        let url = self.config.base_url.clone().unwrap_or_else(|| {
            format!(
                "https://generativelanguage.googleapis.com/v1beta/models/{model}:generateContent?key={api_key}"
            )
        });

        let mut contents = Vec::new();
        for msg in messages {
            match msg {
                AgentMessage::User { text } => {
                    contents.push(json!({ "role": "user", "parts": [{ "text": text }] }));
                }
                AgentMessage::Assistant { text, tool_calls } => {
                    let mut parts = Vec::new();
                    if !text.is_empty() {
                        parts.push(json!({ "text": text }));
                    }
                    for tc in tool_calls {
                        parts.push(json!({
                            "functionCall": { "name": tc.name, "args": tc.arguments }
                        }));
                    }
                    contents.push(json!({ "role": "model", "parts": parts }));
                }
                AgentMessage::ToolResult {
                    tool_name,
                    content,
                    is_error,
                    ..
                } => {
                    contents.push(json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": tool_name,
                                "response": { "output": content, "is_error": is_error }
                            }
                        }]
                    }));
                }
            }
        }

        let mut body = json!({
            "systemInstruction": { "parts": [{ "text": system_prompt }] },
            "contents": contents,
            "generationConfig": { "maxOutputTokens": self.config.max_tokens }
        });
        if !tools.is_empty() {
            let decls: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    })
                })
                .collect();
            body["tools"] = json!([{ "functionDeclarations": decls }]);
        }

        let resp = self
            .http
            .post(&url)
            .json(&body)
            .send()
            .await
            .context("Gemini request failed")?;
        if !resp.status().is_success() {
            bail!("Gemini returned HTTP {}", resp.status().as_u16());
        }
        let val: Value = resp.json().await?;

        let mut text_parts = Vec::new();
        let mut calls = Vec::new();
        if let Some(parts) = val
            .pointer("/candidates/0/content/parts")
            .and_then(Value::as_array)
        {
            for (idx, part) in parts.iter().enumerate() {
                if let Some(t) = part.get("text").and_then(Value::as_str) {
                    text_parts.push(t.to_string());
                }
                if let Some(fc) = part.get("functionCall") {
                    if let Some(name) = fc.get("name").and_then(Value::as_str) {
                        calls.push(ToolCall {
                            id: format!("gemini_call_{idx}"),
                            name: name.to_string(),
                            arguments: fc.get("args").cloned().unwrap_or_else(|| json!({})),
                        });
                    }
                }
            }
        }

        Ok(LlmTurnResponse {
            text: text_parts.join("\n"),
            tool_calls: calls,
            provider: "gemini".into(),
            model: model.to_string(),
        })
    }

    async fn call_openai(
        &self,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        messages: &[AgentMessage],
        tools: &[ToolSpec],
    ) -> Result<LlmTurnResponse> {
        let url = self
            .config
            .base_url
            .clone()
            .unwrap_or_else(|| "https://api.openai.com/v1/chat/completions".into());

        let mut wire_messages = vec![json!({ "role": "system", "content": system_prompt })];
        for msg in messages {
            match msg {
                AgentMessage::User { text } => {
                    wire_messages.push(json!({ "role": "user", "content": text }));
                }
                AgentMessage::Assistant { text, tool_calls } => {
                    let mut m = json!({ "role": "assistant", "content": text });
                    if !tool_calls.is_empty() {
                        let tcs: Vec<Value> = tool_calls
                            .iter()
                            .map(|tc| {
                                json!({
                                    "id": tc.id,
                                    "type": "function",
                                    "function": {
                                        "name": tc.name,
                                        "arguments": tc.arguments.to_string()
                                    }
                                })
                            })
                            .collect();
                        m["tool_calls"] = Value::Array(tcs);
                    }
                    wire_messages.push(m);
                }
                AgentMessage::ToolResult {
                    tool_call_id,
                    content,
                    ..
                } => {
                    wire_messages.push(json!({
                        "role": "tool",
                        "tool_call_id": tool_call_id,
                        "content": content
                    }));
                }
            }
        }

        let mut body = json!({
            "model": model,
            "max_tokens": self.config.max_tokens,
            "messages": wire_messages
        });
        if !tools.is_empty() {
            let wire_tools: Vec<Value> = tools
                .iter()
                .map(|t| {
                    json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema
                        }
                    })
                })
                .collect();
            body["tools"] = Value::Array(wire_tools);
        }

        let resp = self
            .http
            .post(&url)
            .bearer_auth(api_key)
            .json(&body)
            .send()
            .await
            .context("OpenAI request failed")?;
        if !resp.status().is_success() {
            bail!("OpenAI returned HTTP {}", resp.status().as_u16());
        }
        let val: Value = resp.json().await?;
        let message = val.pointer("/choices/0/message").cloned().unwrap_or(Value::Null);
        let text = message
            .get("content")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();

        let mut calls = Vec::new();
        if let Some(arr) = message.get("tool_calls").and_then(Value::as_array) {
            for tc in arr {
                let id = tc
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("openai_call")
                    .to_string();
                let name = tc
                    .pointer("/function/name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let args_str = tc
                    .pointer("/function/arguments")
                    .and_then(Value::as_str)
                    .unwrap_or("{}");
                let arguments = serde_json::from_str(args_str).unwrap_or_else(|_| json!({}));
                if !name.is_empty() {
                    calls.push(ToolCall {
                        id,
                        name,
                        arguments,
                    });
                }
            }
        }

        Ok(LlmTurnResponse {
            text,
            tool_calls: calls,
            provider: "openai".into(),
            model: model.to_string(),
        })
    }
}
