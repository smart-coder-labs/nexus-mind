//! NexusMind REST API client for multi-PC memory synchronization.

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemoryPayload {
    pub title: String,
    pub memory_type: String,
    pub content: String,
    pub topic_key: String,
    pub scope: String, // "global_user" | "device"
}

impl NexusMindClient {
    pub fn new(config: NexusMindSyncConfig, device_id: String) -> Result<Self> {
        let http = Client::builder().timeout(Duration::from_secs(10)).build()?;
        Ok(Self {
            http,
            config,
            device_id,
        })
    }

    pub fn configured(&self) -> bool {
        self.config.enabled && self.api_key().is_some()
    }

    fn api_key(&self) -> Option<String> {
        std::env::var(&self.config.api_key_env)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    }

    /// Stores or updates a redacted memory in NexusMind using an evolving `topic_key`.
    pub async fn store_memory(&self, mem: &MemoryPayload) -> Result<Value> {
        let key = self
            .api_key()
            .ok_or_else(|| anyhow::anyhow!("{} not set", self.config.api_key_env))?;
        let scoped_topic = if mem.scope == "device" {
            format!("mascot/device/{}/{}", self.device_id, mem.topic_key)
        } else {
            format!("mascot/global/{}", mem.topic_key)
        };
        let url = format!("{}/v1/memories", self.config.base_url.trim_end_matches('/'));
        let body = json!({
            "title": mem.title,
            "type": mem.memory_type,
            "project": self.config.project,
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
        if !resp.status().is_success() {
            bail!("NexusMind returned HTTP {}", resp.status().as_u16());
        }
        Ok(resp.json().await?)
    }

    /// Semantic search over NexusMind memories.
    pub async fn search_memory(&self, query: &str, limit: usize) -> Result<Value> {
        let key = self
            .api_key()
            .ok_or_else(|| anyhow::anyhow!("{} not set", self.config.api_key_env))?;
        let url = format!("{}/v1/search", self.config.base_url.trim_end_matches('/'));
        let resp = self
            .http
            .get(&url)
            .bearer_auth(key)
            .query(&[
                ("q", query),
                ("project", self.config.project.as_str()),
                ("limit", &limit.to_string()),
            ])
            .send()
            .await
            .context("NexusMind search_memory failed")?;
        if !resp.status().is_success() {
            bail!("NexusMind search returned HTTP {}", resp.status().as_u16());
        }
        Ok(resp.json().await?)
    }
}
