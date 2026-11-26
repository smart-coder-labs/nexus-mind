//! Unified System 1 decision router supporting **Laya** (local), **Jev** (cloud),
//! **Hybrid** (Laya-first with confidence-based Jev escalation), and **DeterministicOnly**.

pub mod client;
pub mod decisions;
pub mod protocol;

use anyhow::{anyhow, Result};
use client::{call_systemone, EndpointConfig};
use decisions::{
    build_action_risk_request, build_completion_judge_request, build_memory_worthiness_request,
    build_proactive_interrupt_request, build_route_task_request, parse_action_risk_response,
    parse_completion_judge_response, parse_memory_worthiness_response,
    parse_proactive_interrupt_response, parse_route_task_response, ActionRiskAnswer,
    ActionRiskInput, CompletionJudgeAnswer, CompletionJudgeInput, MemoryEventInput,
    MemoryWorthinessAnswer, ProactiveInterruptAnswer, ProactiveInterruptInput, RouteTaskAnswer,
    RouteTaskInput,
};
use protocol::{SystemOneRequest, SystemOneResponse};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionMode {
    LayaLocal,
    JevCloud,
    #[default]
    Hybrid,
    DeterministicOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSource {
    LayaLocal,
    JevCloud,
    DeterministicFallback,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DecisionEnvelope<T> {
    pub answer: T,
    pub source: DecisionSource,
    pub model: String,
    pub latency_ms: u64,
    pub escalated_from_laya: bool,
}

#[derive(Clone, Debug)]
pub struct HybridPolicy {
    /// If Laya's minimum confidence is below this threshold in `Hybrid` mode, escalate to Jev.
    pub escalate_below_confidence: f64,
    /// Never send private-context payloads to Jev Cloud even when escalating.
    pub allow_cloud_for_private_context: bool,
}

impl Default for HybridPolicy {
    fn default() -> Self {
        Self {
            escalate_below_confidence: 0.80,
            allow_cloud_for_private_context: false,
        }
    }
}

#[derive(Clone, Debug)]
pub struct SystemOneRouter {
    client: Client,
    pub mode: DecisionMode,
    pub laya: EndpointConfig,
    pub jev: EndpointConfig,
    pub hybrid: HybridPolicy,
}

impl SystemOneRouter {
    pub fn new(
        mode: DecisionMode,
        laya: EndpointConfig,
        jev: EndpointConfig,
        hybrid: HybridPolicy,
    ) -> Result<Self> {
        let client = Client::builder().build()?;
        Ok(Self {
            client,
            mode,
            laya,
            jev,
            hybrid,
        })
    }

    pub async fn evaluate_action_risk(
        &self,
        input: &ActionRiskInput,
    ) -> Result<DecisionEnvelope<ActionRiskAnswer>> {
        self.dispatch(
            input.is_private_context,
            |model| build_action_risk_request(model, input),
            parse_action_risk_response,
            |ans| ans.risk_confidence.min(ans.class_confidence),
        )
        .await
    }

    pub async fn evaluate_memory_worthiness(
        &self,
        input: &MemoryEventInput,
        is_private_context: bool,
    ) -> Result<DecisionEnvelope<MemoryWorthinessAnswer>> {
        self.dispatch(
            is_private_context,
            |model| build_memory_worthiness_request(model, input),
            parse_memory_worthiness_response,
            |ans| ans.category_confidence.min(ans.importance_confidence),
        )
        .await
    }

    pub async fn route_task(
        &self,
        input: &RouteTaskInput,
        is_private_context: bool,
    ) -> Result<DecisionEnvelope<RouteTaskAnswer>> {
        self.dispatch(
            is_private_context,
            |model| build_route_task_request(model, input),
            parse_route_task_response,
            |ans| ans.confidence,
        )
        .await
    }

    pub async fn evaluate_proactive_interrupt(
        &self,
        input: &ProactiveInterruptInput,
    ) -> Result<DecisionEnvelope<ProactiveInterruptAnswer>> {
        self.dispatch(
            true, // always prefer local for passive desktop state
            |model| build_proactive_interrupt_request(model, input),
            parse_proactive_interrupt_response,
            |ans| ans.confidence,
        )
        .await
    }

    pub async fn judge_completion(
        &self,
        input: &CompletionJudgeInput,
        is_private_context: bool,
    ) -> Result<DecisionEnvelope<CompletionJudgeAnswer>> {
        self.dispatch(
            is_private_context,
            |model| build_completion_judge_request(model, input),
            parse_completion_judge_response,
            |ans| ans.confidence,
        )
        .await
    }

    async fn dispatch<T, B, P, C>(
        &self,
        is_private_context: bool,
        build: B,
        parse: P,
        min_confidence: C,
    ) -> Result<DecisionEnvelope<T>>
    where
        B: Fn(&str) -> SystemOneRequest,
        P: Fn(&SystemOneResponse) -> Result<T>,
        C: Fn(&T) -> f64,
    {
        let can_use_cloud = !is_private_context || self.hybrid.allow_cloud_for_private_context;
        let has_jev_key = self
            .jev
            .api_key
            .as_deref()
            .is_some_and(|k| !k.trim().is_empty());

        match self.mode {
            DecisionMode::DeterministicOnly => {
                Err(anyhow!("decision engine is set to deterministic_only"))
            }
            DecisionMode::LayaLocal => {
                let req = build(&self.laya.model);
                let raw = call_systemone(&self.client, &self.laya, &req).await?;
                let answer = parse(&raw.response)?;
                Ok(DecisionEnvelope {
                    answer,
                    source: DecisionSource::LayaLocal,
                    model: raw.response.model,
                    latency_ms: raw.latency_ms,
                    escalated_from_laya: false,
                })
            }
            DecisionMode::JevCloud => {
                if !can_use_cloud {
                    return Err(anyhow!(
                        "cloud Jev blocked for private context; configure laya_local or hybrid"
                    ));
                }
                if !has_jev_key {
                    return Err(anyhow!("TYPESAFE_API_KEY / JEV_API_KEY not configured"));
                }
                let req = build(&self.jev.model);
                let raw = call_systemone(&self.client, &self.jev, &req).await?;
                let answer = parse(&raw.response)?;
                Ok(DecisionEnvelope {
                    answer,
                    source: DecisionSource::JevCloud,
                    model: raw.response.model,
                    latency_ms: raw.latency_ms,
                    escalated_from_laya: false,
                })
            }
            DecisionMode::Hybrid => {
                let laya_req = build(&self.laya.model);
                match call_systemone(&self.client, &self.laya, &laya_req).await {
                    Ok(raw) => {
                        let answer = parse(&raw.response)?;
                        let conf = min_confidence(&answer);
                        if conf < self.hybrid.escalate_below_confidence
                            && can_use_cloud
                            && has_jev_key
                        {
                            let jev_req = build(&self.jev.model);
                            match call_systemone(&self.client, &self.jev, &jev_req).await {
                                Ok(jev_raw) => {
                                    let jev_answer = parse(&jev_raw.response)?;
                                    return Ok(DecisionEnvelope {
                                        answer: jev_answer,
                                        source: DecisionSource::JevCloud,
                                        model: jev_raw.response.model,
                                        latency_ms: raw.latency_ms + jev_raw.latency_ms,
                                        escalated_from_laya: true,
                                    });
                                }
                                Err(err) => {
                                    warn!(
                                        error = %err,
                                        "hybrid escalation to Jev failed; keeping Laya answer"
                                    );
                                }
                            }
                        }
                        Ok(DecisionEnvelope {
                            answer,
                            source: DecisionSource::LayaLocal,
                            model: raw.response.model,
                            latency_ms: raw.latency_ms,
                            escalated_from_laya: false,
                        })
                    }
                    Err(laya_err) => {
                        if can_use_cloud && has_jev_key {
                            warn!(
                                error = %laya_err,
                                "Laya local unavailable; falling back to Jev cloud"
                            );
                            let jev_req = build(&self.jev.model);
                            let jev_raw = call_systemone(&self.client, &self.jev, &jev_req).await?;
                            let jev_answer = parse(&jev_raw.response)?;
                            Ok(DecisionEnvelope {
                                answer: jev_answer,
                                source: DecisionSource::JevCloud,
                                model: jev_raw.response.model,
                                latency_ms: jev_raw.latency_ms,
                                escalated_from_laya: true,
                            })
                        } else {
                            Err(laya_err)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    async fn spawn_mock_systemone(body: serde_json::Value) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let payload = serde_json::to_vec(&body).unwrap();
        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0_u8; 4096];
                let _ = socket.read(&mut buf).await;
                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    payload.len()
                );
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.write_all(&payload).await;
            }
        });
        format!("http://{addr}/v1/systemone")
    }

    fn action_risk_payload(model: &str, risk_score: f64, confidence: f64) -> serde_json::Value {
        json!({
            "model": model,
            "answers": {
                "action_class": { "type": "choice", "choice": "reversible_local", "confidence": confidence },
                "reversibility": { "type": "choice", "choice": "reversible", "confidence": confidence },
                "risk": { "type": "score", "score": risk_score, "confidence": confidence }
            },
            "usage": { "input_tokens": 120, "output_tokens": 12 }
        })
    }

    #[tokio::test]
    async fn hybrid_stays_on_laya_when_confident() {
        let laya_url = spawn_mock_systemone(action_risk_payload("laya-mlx-1.0", 0.3, 0.94)).await;
        let router = SystemOneRouter::new(
            DecisionMode::Hybrid,
            EndpointConfig {
                endpoint: laya_url,
                model: "laya-latest".into(),
                api_key: None,
                timeout: Duration::from_secs(2),
            },
            EndpointConfig {
                endpoint: "http://127.0.0.1:1/unreachable".into(),
                model: "jev-latest".into(),
                api_key: Some("test-key".into()),
                timeout: Duration::from_secs(1),
            },
            HybridPolicy::default(),
        )
        .unwrap();

        let env = router
            .evaluate_action_risk(&ActionRiskInput {
                action_kind: "filesystem".into(),
                summary: "Organize ~/Downloads into folders by file type".into(),
                target_app_or_path: Some("~/Downloads".into()),
                command_preview: None,
                is_private_context: false,
            })
            .await
            .unwrap();

        assert_eq!(env.source, DecisionSource::LayaLocal);
        assert!(!env.escalated_from_laya);
        assert!((env.answer.risk - 0.1).abs() < 1e-6);
    }

    #[tokio::test]
    async fn hybrid_escalates_to_jev_when_laya_confidence_is_low() {
        let laya_url = spawn_mock_systemone(action_risk_payload("laya-mlx-1.0", 1.5, 0.52)).await;
        let jev_url = spawn_mock_systemone(action_risk_payload("jev-1.13.0", 2.4, 0.96)).await;

        let router = SystemOneRouter::new(
            DecisionMode::Hybrid,
            EndpointConfig {
                endpoint: laya_url,
                model: "laya-latest".into(),
                api_key: None,
                timeout: Duration::from_secs(2),
            },
            EndpointConfig {
                endpoint: jev_url,
                model: "jev-latest".into(),
                api_key: Some("test-key".into()),
                timeout: Duration::from_secs(2),
            },
            HybridPolicy::default(),
        )
        .unwrap();

        let env = router
            .evaluate_action_risk(&ActionRiskInput {
                action_kind: "shell".into(),
                summary: "Run deploy script to staging".into(),
                target_app_or_path: Some("scripts/deploy.sh".into()),
                command_preview: Some("./scripts/deploy.sh".into()),
                is_private_context: false,
            })
            .await
            .unwrap();

        assert_eq!(env.source, DecisionSource::JevCloud);
        assert!(env.escalated_from_laya);
        assert!((env.answer.risk - 0.8).abs() < 1e-6);
    }

    #[tokio::test]
    async fn hybrid_never_escalates_private_context_to_cloud() {
        let laya_url = spawn_mock_systemone(action_risk_payload("laya-mlx-1.0", 1.5, 0.45)).await;
        let router = SystemOneRouter::new(
            DecisionMode::Hybrid,
            EndpointConfig {
                endpoint: laya_url,
                model: "laya-latest".into(),
                api_key: None,
                timeout: Duration::from_secs(2),
            },
            EndpointConfig {
                endpoint: "http://127.0.0.1:1/should-not-be-called".into(),
                model: "jev-latest".into(),
                api_key: Some("test-key".into()),
                timeout: Duration::from_secs(1),
            },
            HybridPolicy {
                escalate_below_confidence: 0.80,
                allow_cloud_for_private_context: false,
            },
        )
        .unwrap();

        let env = router
            .evaluate_action_risk(&ActionRiskInput {
                action_kind: "ui".into(),
                summary: "Read document in private folder".into(),
                target_app_or_path: Some("~/Private".into()),
                command_preview: None,
                is_private_context: true,
            })
            .await
            .unwrap();

        assert_eq!(env.source, DecisionSource::LayaLocal);
        assert!(!env.escalated_from_laya);
    }

    #[test]
    fn rejects_percentage_confidence_without_clamping() {
        let resp: SystemOneResponse = serde_json::from_value(json!({
            "model": "laya-mlx-1.0",
            "answers": {
                "action_class": { "type": "choice", "choice": "read_only", "confidence": 95.0 },
                "reversibility": { "type": "choice", "choice": "reversible", "confidence": 0.9 },
                "risk": { "type": "score", "score": 0.0, "confidence": 0.9 }
            }
        }))
        .unwrap();
        assert!(parse_action_risk_response(&resp).is_err());
    }
}
