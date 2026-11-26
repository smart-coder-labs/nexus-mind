//! Async HTTP client for `/v1/systemone` endpoints (local Laya or hosted Jev).

use crate::protocol::{SystemOneRequest, SystemOneResponse};
use anyhow::{bail, Context, Result};
use reqwest::{Client, StatusCode};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct EndpointConfig {
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub timeout: Duration,
}

impl EndpointConfig {
    pub fn local_laya_default() -> Self {
        Self {
            endpoint: "http://127.0.0.1:8787/v1/systemone".into(),
            model: "laya-latest".into(),
            api_key: None,
            timeout: Duration::from_millis(750),
        }
    }

    pub fn cloud_jev_default(api_key: Option<String>) -> Self {
        Self {
            endpoint: "https://api.typesafe.ai/v1/systemone".into(),
            model: "jev-latest".into(),
            api_key,
            timeout: Duration::from_secs(5),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RawCallResult {
    pub response: SystemOneResponse,
    pub latency_ms: u64,
}

pub async fn call_systemone(
    client: &Client,
    config: &EndpointConfig,
    request: &SystemOneRequest,
) -> Result<RawCallResult> {
    let encoded = request
        .to_bounded_bytes()
        .context("failed to encode /v1/systemone payload")?;
    let started = Instant::now();

    for attempt in 0..3_u32 {
        let mut req = client
            .post(&config.endpoint)
            .timeout(config.timeout)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(encoded.clone());

        if let Some(key) = config.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            req = req.bearer_auth(key.trim());
        }

        let resp = req
            .send()
            .await
            .with_context(|| format!("systemone HTTP request failed ({})", config.endpoint))?;
        let status = resp.status();

        if status.is_success() {
            let parsed: SystemOneResponse = resp
                .json()
                .await
                .context("failed to decode /v1/systemone JSON response")?;
            if parsed.model.trim().is_empty() {
                bail!("systemone_schema: response omitted model field");
            }
            return Ok(RawCallResult {
                response: parsed,
                latency_ms: started.elapsed().as_millis() as u64,
            });
        }

        if (status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529) && attempt < 2 {
            tokio::time::sleep(Duration::from_millis(200 * (1_u64 << attempt))).await;
            continue;
        }
        if status == StatusCode::UNAUTHORIZED {
            bail!("systemone_http_401: invalid or missing API key");
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            bail!("systemone_http_422: request schema rejected");
        }
        bail!("systemone_http_{}", status.as_u16());
    }

    unreachable!("retry loop always returns or errors")
}
