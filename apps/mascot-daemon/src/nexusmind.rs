//! NexusMind REST API client for mandatory connection verification,
//! multi-PC memory sync, org-wide memory search, live pet heartbeats,
//! full conversation logging, and audit events.

use crate::config::NexusMindSyncConfig;
use anyhow::{bail, Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Clone)]
pub struct NexusMindClient {
    http: Client,
    config: NexusMindSyncConfig,
    device_id: String,
    api_key_override: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryPayload {
    pub title: String,
    pub memory_type: String,
    pub content: String,
    pub topic_key: String,
    pub scope: String, // "global_user" | "device"
}

#[derive(Clone, Debug, Serialize)]
pub struct PetHeartbeatPayload {
    pub device_id: String,
    pub pet_name: String,
    pub hostname: String,
    pub os_name: String,
    pub os_version: String,
    pub arch: String,
    pub autonomy_level: String,
    pub decision_mode: String,
    pub visual_state: String,
    pub skills_count: i64,
    pub mcp_tools_count: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct PetEventPayload {
    pub device_id: String,
    pub run_id: Option<String>,
    pub event_kind: String,
    pub action_kind: Option<String>,
    pub decision_source: Option<String>,
    pub risk: Option<f64>,
    pub outcome: Option<String>,
    pub summary: String,
    pub detail_json: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PetConversationPayload {
    pub device_id: String,
    pub run_id: String,
    pub objective: String,
    pub status: String,
    pub route: Option<String>,
    pub steps_executed: i64,
    pub tool_errors: i64,
    pub transcript_json: String,
    pub final_reply: Option<String>,
}

impl NexusMindClient {
    pub fn new(
        config: NexusMindSyncConfig,
        device_id: String,
        api_key_override: Option<String>,
    ) -> Result<Self> {
        let http = Client::builder().timeout(Duration::from_secs(12)).build()?;
        Ok(Self {
            http,
            config,
            device_id,
            api_key_override,
        })
    }

    pub fn configured(&self) -> bool {
        self.config.enabled && self.api_key().is_some()
    }

    pub fn base_url(&self) -> &str {
        &self.config.base_url
    }

    fn api_key(&self) -> Option<String> {
        if let Some(v) = self
            .api_key_override
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            return Some(v.trim().to_string());
        }
        std::env::var(&self.config.api_key_env)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    /// Sends a live authenticated heartbeat to `/v1/pets/heartbeat` (also validates the connection).
    pub async fn send_heartbeat(&self, payload: &PetHeartbeatPayload) -> Result<Value> {
        let key = self.api_key().ok_or_else(|| {
            anyhow::anyhow!("Falta NEXUSMIND_API_KEY; configúrala en Ajustes → API Keys")
        })?;
        let url = format!(
            "{}/v1/pets/heartbeat",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(payload)
            .send()
            .await
            .with_context(|| format!("No se pudo conectar a NexusMind en {url}"))?;
        let status = resp.status();
        if status.as_u16() == 401 || status.as_u16() == 403 {
            bail!("API key de NexusMind rechazada (HTTP {})", status.as_u16());
        }
        if !status.is_success() {
            bail!("NexusMind respondió HTTP {}", status.as_u16());
        }
        Ok(resp.json().await?)
    }

    /// Pushes one auditable pet event to NexusMind (`/v1/pets/events`).
    pub async fn push_event(&self, payload: &PetEventPayload) -> Result<()> {
        let Some(key) = self.api_key() else {
            return Ok(());
        };
        let url = format!(
            "{}/v1/pets/events",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!("NexusMind /v1/pets/events HTTP {}", resp.status().as_u16());
        }
        Ok(())
    }

    /// Upserts a full redacted conversation transcript to NexusMind (`/v1/pets/conversations`).
    pub async fn push_conversation(&self, payload: &PetConversationPayload) -> Result<()> {
        let Some(key) = self.api_key() else {
            return Ok(());
        };
        let url = format!(
            "{}/v1/pets/conversations",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(payload)
            .send()
            .await?;
        if !resp.status().is_success() {
            bail!(
                "NexusMind /v1/pets/conversations HTTP {}",
                resp.status().as_u16()
            );
        }
        Ok(())
    }

    /// Stores or updates a redacted memory in NexusMind (`/v1/memory/store`).
    pub async fn store_memory(&self, mem: &MemoryPayload) -> Result<Value> {
        let key = self
            .api_key()
            .ok_or_else(|| anyhow::anyhow!("{} not set", self.config.api_key_env))?;
        let scoped_topic = if mem.scope == "device" {
            format!("mascot/device/{}/{}", self.device_id, mem.topic_key)
        } else {
            format!("mascot/global/{}", mem.topic_key)
        };
        let url = format!(
            "{}/v1/memory/store",
            self.config.base_url.trim_end_matches('/')
        );
        let body = json!({
            "title": mem.title,
            "type": mem.memory_type,
            "project": self.config.project,
            "tool": "nexus-mascot",
            "content": mem.content,
            "topic_key": scoped_topic,
            "tags": ["mascot", format!("device:{}", self.device_id), mem.scope],
        });
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(&body)
            .send()
            .await
            .context("NexusMind store_memory failed")?;
        let status = resp.status();
        if !status.is_success() {
            let err_body = resp.text().await.unwrap_or_default();
            bail!(
                "NexusMind /v1/memory/store HTTP {}: {}",
                status.as_u16(),
                err_body.chars().take(240).collect::<String>()
            );
        }
        Ok(resp.json().await?)
    }

    /// Organization-wide semantic/hybrid search over NexusMind memories (`POST /v1/memory/search`).
    /// Does NOT restrict to a single project so the mascot can recall memories across all your projects.
    pub async fn search_memory(&self, query: &str, limit: usize) -> Result<Value> {
        let key = self
            .api_key()
            .ok_or_else(|| anyhow::anyhow!("{} not set", self.config.api_key_env))?;
        let url = format!(
            "{}/v1/memory/search",
            self.config.base_url.trim_end_matches('/')
        );
        let resp = self
            .http
            .post(&url)
            .bearer_auth(key)
            .json(&json!({
                "query": query,
                "limit": limit,
                "mode": "hybrid",
            }))
            .send()
            .await
            .context("NexusMind search_memory failed")?;
        if !resp.status().is_success() {
            bail!("NexusMind search returned HTTP {}", resp.status().as_u16());
        }
        Ok(resp.json().await?)
    }
}

/// Formats NexusMind `/v1/memory/search` response into readable context bullets for the LLM prompt.
pub fn format_memories_for_prompt(val: &Value, max_chars: usize) -> Option<String> {
    let list = val
        .get("memories")
        .or_else(|| val.get("items"))
        .or_else(|| val.get("results"))
        .and_then(Value::as_array)
        .or_else(|| val.as_array())?;

    if list.is_empty() {
        return None;
    }

    let mut out = String::new();
    for item in list.iter().take(6) {
        let project = item.get("project").and_then(Value::as_str).unwrap_or("org");
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("memory");
        let title = item.get("title").and_then(Value::as_str).unwrap_or("Nota");
        let content = item
            .get("content")
            .or_else(|| item.get("preview"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let snippet: String = content.chars().take(280).collect();
        let line = format!("- [{project} · {kind}] {title}: {snippet}\n");
        if out.len() + line.len() > max_chars {
            break;
        }
        out.push_str(&line);
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}
