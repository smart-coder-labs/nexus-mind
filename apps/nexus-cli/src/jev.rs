//! TypeSafe Jev completion judge.
//!
//! Wire format: https://docs.typesafe.ai/api
//! This module only asks bounded questions. It never treats model output as
//! permission to bypass the harness's deterministic completion checks.

use crate::model::{DecisionRecord, NextAction, TaskExecutionState};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use reqwest::{blocking::Client, StatusCode};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, env, thread, time::Duration};
use uuid::Uuid;

const JEV_ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
const JEV_MODEL: &str = "jev-latest";
const MIN_FINISH_CONFIDENCE: f32 = 0.75;
const MAX_INCOMPLETE_PROBABILITY: f32 = 0.20;
const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Clone)]
struct JevConfig {
    api_key: String,
    endpoint: String,
    timeout: Duration,
}

impl JevConfig {
    fn from_env() -> Result<Self> {
        let api_key = api_key().ok_or_else(|| {
            anyhow!(
                "Jev API key is missing; set TYPESAFE_API_KEY (or JEV_API_KEY) in your environment"
            )
        })?;
        Ok(Self {
            api_key,
            endpoint: JEV_ENDPOINT.into(),
            timeout: Duration::from_secs(10),
        })
    }
}

fn api_key() -> Option<String> {
    ["TYPESAFE_API_KEY", "JEV_API_KEY"]
        .into_iter()
        .filter_map(|name| env::var(name).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

/// Whether an API key is configured. This does not validate the key or make a
/// network request.
pub fn configured() -> bool {
    api_key().is_some()
}

/// Ask Jev whether a task is complete. API errors and malformed answers remain
/// errors; callers must not present them as Jev decisions.
pub fn completion_judge(
    state: &TaskExecutionState,
    runtime: Option<String>,
) -> Result<DecisionRecord> {
    completion_judge_with_config(state, runtime, &JevConfig::from_env()?)
}

fn completion_judge_with_config(
    state: &TaskExecutionState,
    runtime: Option<String>,
    config: &JevConfig,
) -> Result<DecisionRecord> {
    if state.objective.trim().is_empty() {
        bail!("Jev completion state has no objective");
    }
    if state.requirements_covered > state.requirements_total {
        bail!("Jev completion state has inconsistent requirement counts");
    }

    let payload = request_body(state);
    let encoded = serde_json::to_vec(&payload).context("could not encode Jev request")?;
    if encoded.len() > MAX_REQUEST_BYTES {
        bail!("Jev completion state exceeds the 64 KiB request limit");
    }

    let client = Client::builder()
        .timeout(config.timeout)
        .build()
        .context("could not initialize Jev HTTP client")?;
    let response_body = post_with_backoff(&client, config, &encoded)?;
    let response: JevResponse =
        serde_json::from_slice(&response_body).context("Jev returned an invalid response body")?;
    let answer = validate_response(&response)?;
    let (selected, gate_reason) = apply_evidence_gate(
        state,
        answer.selected.clone(),
        answer.confidence,
        answer.selected_probability,
        answer.incomplete_probability,
    );
    let decision_confidence = if gate_reason.is_some() {
        1.0
    } else {
        answer.confidence
    };

    let mut reason = format!(
        "Jev ({}) chose {} (confidence {:.2}, selected probability {:.2}, incomplete probability {:.2}; tokens {}/{})",
        response.model,
        answer.choice,
        answer.confidence,
        answer.selected_probability,
        answer.incomplete_probability,
        response.usage.input_tokens,
        response.usage.output_tokens,
    );
    if let Some(gate_reason) = gate_reason {
        reason.push_str(gate_reason);
    }

    let state_hash = format!(
        "sha256:{:x}",
        Sha256::digest(serde_json::to_vec(state).context("could not encode completion state")?)
    );
    Ok(DecisionRecord {
        id: Uuid::new_v4().to_string(),
        task_id: state.task_id.clone(),
        state_hash,
        decision_type: "task_completion_jev".into(),
        candidates: vec![
            NextAction::Finish,
            NextAction::RetrieveMore,
            NextAction::RunTests,
            NextAction::Retry,
            NextAction::HumanReview,
        ],
        selected,
        confidence: decision_confidence,
        reason,
        runtime,
        timestamp: Utc::now(),
    })
}

fn request_body(state: &TaskExecutionState) -> Value {
    // Repository paths, transcripts, API keys, and environment variables are
    // deliberately excluded. The decision only needs compact task evidence.
    json!({
        "model": JEV_MODEL,
        "state": {
            "objective": state.objective,
            "requirements_total": state.requirements_total,
            "requirements_covered": state.requirements_covered,
            "unresolved_requirements": state.unresolved_requirements,
            "changed_files": state.changed_files,
            "changed_symbols": state.changed_symbols,
            "change_summary": state.change_summary,
            "impact_radius": state.impact_radius,
            "affected_processes": state.affected_processes,
            "affected_contracts": state.affected_contracts,
            "context_sources": state.context_sources,
            "verification": {
                "commands_executed": state.verification.commands.len(),
                "tests_passed": state.verification.tests_passed,
                "tests_failed": state.verification.tests_failed,
                "lint_passed": state.verification.lint_passed,
                "typecheck_passed": state.verification.typecheck_passed,
            },
            "unresolved_evidence": state.unresolved_evidence,
            "attempts": state.attempts,
            "tool_errors": state.tool_errors,
        },
        "questions": {
            "next_action": {
                "type": "choice",
                "instructions": "Based only on the supplied task evidence, what is the next action? Choose human_review when the evidence cannot support a confident decision.",
                "criteria": {
                    "finish": "All requirements are covered, no known gaps or failures remain, and relevant verification passed.",
                    "retrieve_more": "Requirements, scope, or implementation evidence is insufficient and more context is needed.",
                    "run_tests": "Implementation may be complete, but relevant verification is missing or incomplete.",
                    "retry": "Known implementation or verification failure requires another attempt.",
                    "human_review": "The supplied evidence is ambiguous, conflicting, or needs a person's judgment."
                }
            },
            "task_is_incomplete": {
                "type": "noul",
                "instructions": "Does the supplied evidence show that the objective is incomplete?"
            }
        }
    })
}

fn post_with_backoff(client: &Client, config: &JevConfig, encoded: &[u8]) -> Result<Vec<u8>> {
    for attempt in 0..3 {
        let response = client
            .post(&config.endpoint)
            .bearer_auth(&config.api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(encoded.to_vec())
            .send()
            .context("Jev request failed")?;
        let status = response.status();
        if status.is_success() {
            return response
                .bytes()
                .map(|bytes| bytes.to_vec())
                .context("could not read Jev response");
        }
        if (status == StatusCode::TOO_MANY_REQUESTS || status.as_u16() == 529) && attempt < 2 {
            thread::sleep(Duration::from_millis(250 * (1 << attempt)));
            continue;
        }
        if status == StatusCode::UNAUTHORIZED {
            bail!("Jev rejected the API key (HTTP 401); check TYPESAFE_API_KEY or JEV_API_KEY");
        }
        if status == StatusCode::UNPROCESSABLE_ENTITY {
            bail!("Jev rejected the request schema (HTTP 422)");
        }
        bail!("Jev returned HTTP {}", status.as_u16());
    }
    unreachable!("retry loop must return or error")
}

#[derive(Deserialize)]
struct JevResponse {
    model: String,
    answers: HashMap<String, Value>,
    usage: JevUsage,
}

#[derive(Deserialize)]
struct JevUsage {
    input_tokens: u64,
    output_tokens: u64,
}

#[derive(Deserialize)]
struct ChoiceAnswer {
    #[serde(rename = "type")]
    kind: String,
    choice: String,
    confidence: f32,
    probabilities: HashMap<String, f32>,
}

#[derive(Deserialize)]
struct NoulAnswer {
    #[serde(rename = "type")]
    kind: String,
    noul: f32,
}

struct JevEvaluation {
    selected: NextAction,
    choice: String,
    confidence: f32,
    selected_probability: f32,
    incomplete_probability: f32,
}

fn validate_response(response: &JevResponse) -> Result<JevEvaluation> {
    if response.model.trim().is_empty() {
        bail!("Jev response omitted the model");
    }
    let choice: ChoiceAnswer = serde_json::from_value(
        response
            .answers
            .get("next_action")
            .cloned()
            .ok_or_else(|| anyhow!("Jev response omitted next_action"))?,
    )
    .context("Jev returned an invalid next_action answer")?;
    if choice.kind != "choice" {
        bail!("Jev returned the wrong next_action answer type");
    }
    let selected = match choice.choice.as_str() {
        "finish" => NextAction::Finish,
        "retrieve_more" => NextAction::RetrieveMore,
        "run_tests" => NextAction::RunTests,
        "retry" => NextAction::Retry,
        "human_review" => NextAction::HumanReview,
        _ => bail!("Jev selected an unknown next_action"),
    };
    if !unit_interval(choice.confidence) {
        bail!("Jev returned an invalid confidence");
    }
    if choice.probabilities.len() != 5
        || choice
            .probabilities
            .values()
            .any(|value| !unit_interval(*value))
        || ![
            "finish",
            "retrieve_more",
            "run_tests",
            "retry",
            "human_review",
        ]
        .iter()
        .all(|candidate| choice.probabilities.contains_key(*candidate))
    {
        bail!("Jev returned invalid choice probabilities");
    }
    let selected_probability = choice.probabilities[&choice.choice];
    if (choice.probabilities.values().sum::<f32>() - 1.0).abs() > 0.02 {
        bail!("Jev choice probabilities do not sum to one");
    }

    let incomplete: NoulAnswer = serde_json::from_value(
        response
            .answers
            .get("task_is_incomplete")
            .cloned()
            .ok_or_else(|| anyhow!("Jev response omitted task_is_incomplete"))?,
    )
    .context("Jev returned an invalid task_is_incomplete answer")?;
    if incomplete.kind != "noul" || !unit_interval(incomplete.noul) {
        bail!("Jev returned an invalid task_is_incomplete probability");
    }

    Ok(JevEvaluation {
        selected,
        choice: choice.choice,
        confidence: choice.confidence,
        selected_probability,
        incomplete_probability: incomplete.noul,
    })
}

fn unit_interval(value: f32) -> bool {
    value.is_finite() && (0.0..=1.0).contains(&value)
}

fn apply_evidence_gate(
    state: &TaskExecutionState,
    selected: NextAction,
    confidence: f32,
    selected_probability: f32,
    incomplete_probability: f32,
) -> (NextAction, Option<&'static str>) {
    if selected != NextAction::Finish {
        return (selected, None);
    }
    if let Some(blocker) = completion_blocker(state) {
        return (blocker, Some("; harness evidence gate overrode finish"));
    }
    if confidence < MIN_FINISH_CONFIDENCE
        || selected_probability < MIN_FINISH_CONFIDENCE
        || incomplete_probability > MAX_INCOMPLETE_PROBABILITY
    {
        return (
            NextAction::HumanReview,
            Some("; Jev signals are insufficient for automatic finish"),
        );
    }
    (NextAction::Finish, None)
}

fn completion_blocker(state: &TaskExecutionState) -> Option<NextAction> {
    if state.requirements_total == 0
        || state.requirements_covered < state.requirements_total
        || !state.unresolved_requirements.is_empty()
    {
        Some(NextAction::RetrieveMore)
    } else if state.verification.tests_failed > 0
        || state.verification.lint_passed == Some(false)
        || state.verification.typecheck_passed == Some(false)
    {
        Some(NextAction::Retry)
    } else if state.verification.commands.is_empty()
        || state.verification.tests_passed == 0
        || !state.unresolved_evidence.is_empty()
    {
        Some(NextAction::RunTests)
    } else if state.tool_errors >= 3 {
        Some(NextAction::HumanReview)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Verification;
    use std::{
        io::{Read, Write},
        net::TcpListener,
    };

    fn complete_state() -> TaskExecutionState {
        TaskExecutionState {
            task_id: "task-1".into(),
            objective: "Fix the failing parser".into(),
            repository: "/secret/path".into(),
            requirements_total: 1,
            requirements_covered: 1,
            changed_files: vec!["src/parser.rs".into()],
            verification: Verification {
                tests_passed: 1,
                commands: vec!["cargo test --token=do-not-send".into()],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    fn response(choice: &str, confidence: f32, incomplete: f32) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "model": "jev-1.13.0",
            "answers": {
                "next_action": {
                    "type": "choice", "choice": choice, "confidence": confidence,
                    "probabilities": {
                        "finish": if choice == "finish" { 0.9 } else { 0.025 },
                        "retrieve_more": if choice == "retrieve_more" { 0.9 } else { 0.025 },
                        "run_tests": if choice == "run_tests" { 0.9 } else { 0.025 },
                        "retry": if choice == "retry" { 0.9 } else { 0.025 },
                        "human_review": if choice == "human_review" { 0.9 } else { 0.025 }
                    }
                },
                "task_is_incomplete": {"type": "noul", "noul": incomplete}
            },
            "usage": {"input_tokens": 100, "output_tokens": 20}
        }))
        .unwrap()
    }

    #[test]
    #[ignore = "requires a live TypeSafe credential and network access"]
    fn live_jev_smoke() {
        let record = completion_judge(&complete_state(), Some("smoke-test".into())).unwrap();
        assert_eq!(record.decision_type, "task_completion_jev");
        println!("{}", record.reason);
    }

    #[test]
    fn request_uses_official_shape_without_repository_path() {
        let value = request_body(&complete_state());
        assert_eq!(value["model"], "jev-latest");
        assert_eq!(value["questions"]["next_action"]["type"], "choice");
        assert_eq!(value["questions"]["task_is_incomplete"]["type"], "noul");
        assert!(!value.to_string().contains("/secret/path"));
        assert!(!value.to_string().contains("do-not-send"));
    }

    #[test]
    fn rejects_malformed_answer_and_unknown_choice() {
        let mut value: Value = serde_json::from_slice(&response("finish", 0.9, 0.1)).unwrap();
        value["answers"]["next_action"]["choice"] = json!("deploy");
        let parsed: JevResponse = serde_json::from_value(value).unwrap();
        assert!(validate_response(&parsed).is_err());
    }

    #[test]
    fn calls_real_wire_format_and_gates_finish() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0_u8; 4096];
            loop {
                let count = stream.read(&mut buffer).unwrap();
                bytes.extend_from_slice(&buffer[..count]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            let header_end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
            let headers = String::from_utf8_lossy(&bytes[..header_end]);
            assert!(headers.starts_with("POST /v1/systemone HTTP/1.1"));
            assert!(headers
                .to_ascii_lowercase()
                .contains("authorization: bearer test-key"));
            let content_length: usize = headers
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length: ")
                        .and_then(|length| length.trim().parse().ok())
                })
                .unwrap();
            while bytes.len() < header_end + content_length {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let request: Value =
                serde_json::from_slice(&bytes[header_end..header_end + content_length]).unwrap();
            assert_eq!(request["model"], "jev-latest");
            assert_eq!(request["state"]["objective"], "Fix the failing parser");
            let body = response("finish", 0.9, 0.1);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
            stream.write_all(&body).unwrap();
        });
        let config = JevConfig {
            api_key: "test-key".into(),
            endpoint,
            timeout: Duration::from_secs(3),
        };
        let record = completion_judge_with_config(&complete_state(), None, &config).unwrap();
        server.join().unwrap();
        assert_eq!(record.selected, NextAction::Finish);
        assert_eq!(record.decision_type, "task_completion_jev");
    }

    #[test]
    fn auth_error_is_not_a_decision() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/systemone", listener.local_addr().unwrap());
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let config = JevConfig {
            api_key: "invalid-key".into(),
            endpoint,
            timeout: Duration::from_secs(3),
        };
        let error = completion_judge_with_config(&complete_state(), None, &config).unwrap_err();
        server.join().unwrap();
        assert!(error.to_string().contains("HTTP 401"));
    }

    #[test]
    fn low_confidence_does_not_finish() {
        let parsed: JevResponse = serde_json::from_slice(&response("finish", 0.4, 0.1)).unwrap();
        let answer = validate_response(&parsed).unwrap();
        assert_eq!(
            apply_evidence_gate(
                &complete_state(),
                answer.selected,
                answer.confidence,
                answer.selected_probability,
                answer.incomplete_probability,
            )
            .0,
            NextAction::HumanReview
        );
    }

    #[test]
    fn missing_verification_blocks_finish() {
        let mut state = complete_state();
        state.verification.commands.clear();
        assert_eq!(completion_blocker(&state), Some(NextAction::RunTests));
    }

    #[test]
    fn failed_typecheck_blocks_finish() {
        let mut state = complete_state();
        state.verification.typecheck_passed = Some(false);
        assert_eq!(completion_blocker(&state), Some(NextAction::Retry));
    }

    #[test]
    fn conflicting_incomplete_signal_requires_review() {
        assert_eq!(
            apply_evidence_gate(&complete_state(), NextAction::Finish, 0.9, 0.9, 0.85).0,
            NextAction::HumanReview
        );
    }
}
