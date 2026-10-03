//! Jev (TypeSafe) as the factory's decision model (plan D6, F3).
//!
//! One call asks three typed questions about a change: its task class
//! (choice), the risk of merging it without a person (score over a 4-level
//! rubric) and whether a person must approve (noul, kept only as an audit
//! signal: the F0 spike found it does not separate low from high risk). The
//! answer is validated against the questions asked; anything else is a schema
//! violation, which callers treat as "hold for a person".
//!
//! F3 runs Jev in shadow (ADR 7d6f870f): its verdict is recorded next to the
//! real outcome and never changes what the factory does.

use serde_json::{json, Value};

pub const ENDPOINT: &str = "https://api.typesafe.ai/v1/systemone";
pub const MODEL: &str = "jev-latest";
/// The spike's p95 was 452 ms; a slow answer is a failed one.
pub const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// The allow rule chosen for F3 (ADR 7d6f870f): any class, normalized risk at
/// most this, with at least [`MIN_RISK_CONFIDENCE`] confidence in that score.
pub const MAX_ALLOWED_RISK: f64 = 0.15;
pub const MIN_RISK_CONFIDENCE: f64 = 0.9;

/// What is sent per change; descriptions and path lists are capped.
const MAX_DESCRIPTION_CHARS: usize = 2000;
const MAX_PATHS: usize = 100;

pub const TASK_CLASSES: [(&str, &str); 10] = [
    ("docs", "Documentation or comments only"),
    ("tests", "Adds or changes tests only"),
    ("ui", "User interface or styling"),
    ("backend", "Server-side business logic or APIs"),
    ("bugfix", "Fixes a defect in existing behavior"),
    ("refactor", "Restructures code without changing behavior"),
    ("migration", "Database schema or data migration"),
    ("infra", "CI, deployment, containers or infrastructure"),
    ("security", "Authentication, authorization, secrets or cryptography"),
    ("unknown", "Not enough information to tell"),
];

pub const RISK_LEVELS: [&str; 4] = [
    "Low: cannot break production behavior (docs, tests, copy)",
    "Medium: changes behavior in one isolated component",
    "High: changes shared logic, data handling or public APIs",
    "Critical: auth, payments, crypto, destructive migrations or cross-service consistency",
];

/// The change Jev is asked about.
#[derive(Clone, Debug)]
pub struct Change<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub paths: &'a [String],
}

/// Jev's validated answer.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Answer {
    pub model: Option<String>,
    pub task_class: String,
    pub class_confidence: f64,
    /// Rubric index normalized to 0..1 (`score / (levels - 1)`).
    pub risk: f64,
    pub risk_confidence: f64,
    /// Probability a person must approve; audit signal only.
    pub needs_human: Option<f64>,
    pub input_tokens: Option<u64>,
}

/// Jev's verdict under the F3 rule.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Allow,
    Hold,
}

pub fn verdict(answer: &Answer) -> Verdict {
    if answer.risk <= MAX_ALLOWED_RISK && answer.risk_confidence >= MIN_RISK_CONFIDENCE {
        Verdict::Allow
    } else {
        Verdict::Hold
    }
}

pub fn build_request(change: &Change<'_>) -> Value {
    let description: String = change.description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    let paths: Vec<&String> = change.paths.iter().take(MAX_PATHS).collect();
    let classes: serde_json::Map<String, Value> = TASK_CLASSES
        .iter()
        .map(|(name, text)| (name.to_string(), json!(text)))
        .collect();
    json!({
        "model": MODEL,
        "state": {"title": change.title, "description": description, "paths": paths},
        "questions": {
            "task_class": {
                "type": "choice",
                "instructions": "What kind of software change is this task?",
                "criteria": classes,
            },
            "risk": {
                "type": "score",
                "instructions": "How risky is it to let an automated agent make and merge this change without a person?",
                "criteria": RISK_LEVELS,
            },
            "needs_human": {
                "type": "noul",
                "instructions": "Must a person approve this change before it is merged?",
                "criteria": {"true": "A person must approve", "false": "Automated verification is enough"},
            },
        },
    })
}

/// Validates a response against the questions [`build_request`] asks.
pub fn parse_response(body: &Value) -> anyhow::Result<Answer> {
    let answer = |name: &str, kind: &str| -> anyhow::Result<&Value> {
        let value = body
            .pointer(&format!("/answers/{name}"))
            .ok_or_else(|| anyhow::anyhow!("jev_schema: {name} missing"))?;
        if value.get("type").and_then(Value::as_str) != Some(kind) {
            anyhow::bail!("jev_schema: {name} is not a {kind} answer");
        }
        Ok(value)
    };
    let number = |value: &Value, field: &str| -> anyhow::Result<f64> {
        value
            .get(field)
            .and_then(Value::as_f64)
            .filter(|n| n.is_finite())
            .ok_or_else(|| anyhow::anyhow!("jev_schema: {field} missing"))
    };
    let class = answer("task_class", "choice")?;
    let task_class = class
        .get("choice")
        .and_then(Value::as_str)
        .filter(|choice| TASK_CLASSES.iter().any(|(name, _)| name == choice))
        .ok_or_else(|| anyhow::anyhow!("jev_schema: unknown task_class"))?
        .to_string();
    let risk = answer("risk", "score")?;
    let score = number(risk, "score")?;
    let levels = (RISK_LEVELS.len() - 1) as f64;
    if !(0.0..=levels).contains(&score) {
        anyhow::bail!("jev_schema: risk score {score} outside 0..={levels}");
    }
    let needs_human = answer("needs_human", "noul")?.get("noul").and_then(Value::as_f64);
    Ok(Answer {
        model: body.get("model").and_then(Value::as_str).map(str::to_string),
        task_class,
        class_confidence: number(class, "confidence")?.clamp(0.0, 1.0),
        risk: score / levels,
        risk_confidence: number(risk, "confidence")?.clamp(0.0, 1.0),
        needs_human,
        input_tokens: body.pointer("/usage/input_tokens").and_then(Value::as_u64),
    })
}

/// Asks Jev about one change. Returns the answer and the latency.
pub async fn decide(
    client: &reqwest::Client,
    key: &str,
    change: &Change<'_>,
) -> anyhow::Result<(Answer, u64)> {
    let started = std::time::Instant::now();
    let response = client
        .post(ENDPOINT)
        .bearer_auth(key)
        .timeout(TIMEOUT)
        .json(&build_request(change))
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        anyhow::bail!("jev_http_{}", status.as_u16());
    }
    let body: Value = response.json().await?;
    let answer = parse_response(&body)?;
    Ok((answer, started.elapsed().as_millis() as u64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(choice: &str, class_conf: f64, score: f64, risk_conf: f64) -> Value {
        json!({
            "model": "jev-1.13.0",
            "answers": {
                "task_class": {"type": "choice", "choice": choice, "confidence": class_conf},
                "risk": {"type": "score", "score": score, "confidence": risk_conf},
                "needs_human": {"type": "noul", "noul": 0.42},
            },
            "usage": {"input_tokens": 640},
        })
    }

    #[test]
    fn a_valid_answer_is_parsed_and_its_risk_normalized() {
        let answer = parse_response(&response("docs", 1.0, 0.0, 0.95)).unwrap();
        assert_eq!(answer.task_class, "docs");
        assert_eq!(answer.risk, 0.0);
        assert_eq!(answer.needs_human, Some(0.42));
        assert_eq!(answer.input_tokens, Some(640));
        assert_eq!(parse_response(&response("security", 1.0, 3.0, 0.9)).unwrap().risk, 1.0);
        assert!((parse_response(&response("ui", 0.8, 0.45, 0.9)).unwrap().risk - 0.15).abs() < 1e-9);
    }

    #[test]
    fn answers_that_do_not_match_the_questions_are_rejected() {
        assert!(parse_response(&response("poetry", 1.0, 0.0, 1.0)).is_err());
        assert!(parse_response(&response("docs", 1.0, 7.0, 1.0)).is_err());
        assert!(parse_response(&json!({"answers": {}})).is_err());
        let mut wrong_type = response("docs", 1.0, 0.0, 1.0);
        wrong_type["answers"]["risk"]["type"] = json!("choice");
        assert!(parse_response(&wrong_type).is_err());
        let mut missing_confidence = response("docs", 1.0, 0.0, 1.0);
        missing_confidence["answers"]["risk"].as_object_mut().unwrap().remove("confidence");
        assert!(parse_response(&missing_confidence).is_err());
    }

    #[test]
    fn allow_needs_low_risk_with_high_confidence_in_any_class() {
        let at = |choice, score, conf| verdict(&parse_response(&response(choice, 1.0, score, conf)).unwrap());
        assert_eq!(at("refactor", 0.0, 0.95), Verdict::Allow);
        assert_eq!(at("ui", 0.45, 0.9), Verdict::Allow);
        assert_eq!(at("ui", 0.46, 0.99), Verdict::Hold);
        assert_eq!(at("docs", 0.0, 0.89), Verdict::Hold);
        assert_eq!(at("security", 3.0, 1.0), Verdict::Hold);
    }

    #[test]
    fn the_request_caps_what_leaves_the_machine() {
        let paths: Vec<String> = (0..500).map(|i| format!("src/f{i}.rs")).collect();
        let long = "x".repeat(10_000);
        let request = build_request(&Change { title: "t", description: &long, paths: &paths });
        assert_eq!(request["state"]["description"].as_str().unwrap().len(), MAX_DESCRIPTION_CHARS);
        assert_eq!(request["state"]["paths"].as_array().unwrap().len(), MAX_PATHS);
        assert_eq!(request["questions"]["risk"]["criteria"].as_array().unwrap().len(), 4);
        assert_eq!(request["questions"]["task_class"]["criteria"].as_object().unwrap().len(), 10);
    }
}
