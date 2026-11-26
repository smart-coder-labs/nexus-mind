//! Wire protocol types and strict validators for `/v1/systemone`.
//!
//! Both local **Laya** (`convaiinnovations/laya`, via `laya-mlx` / `laya-serve` / ONNX)
//! and hosted **TypeSafe Jev** implement this wire contract:
//!
//! - Request: `{ "model": "...", "state": { ... }, "questions": { ... } }`
//! - Question kinds: `"choice"`, `"score"`, `"noul"`
//! - Response: `{ "model": "...", "answers": { ... }, "usage": { ... } }`
//!
//! All numeric outputs are validated strictly (no NaN, no clamping percentages into `0..=1`).

use anyhow::{anyhow, bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::HashMap;

pub const MAX_REQUEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize)]
pub struct SystemOneRequest {
    pub model: String,
    pub state: Value,
    pub questions: Map<String, Value>,
}

impl SystemOneRequest {
    pub fn new(model: impl Into<String>, state: Value) -> Self {
        Self {
            model: model.into(),
            state,
            questions: Map::new(),
        }
    }

    pub fn with_choice(
        mut self,
        name: impl Into<String>,
        instructions: impl Into<String>,
        criteria: &[(&str, &str)],
    ) -> Self {
        let crit: Map<String, Value> = criteria
            .iter()
            .map(|(k, v)| ((*k).to_string(), json!(v)))
            .collect();
        self.questions.insert(
            name.into(),
            json!({
                "type": "choice",
                "instructions": instructions.into(),
                "criteria": crit,
            }),
        );
        self
    }

    pub fn with_score(
        mut self,
        name: impl Into<String>,
        instructions: impl Into<String>,
        rubric_levels: &[&str],
    ) -> Self {
        self.questions.insert(
            name.into(),
            json!({
                "type": "score",
                "instructions": instructions.into(),
                "criteria": rubric_levels,
            }),
        );
        self
    }

    pub fn with_noul(
        mut self,
        name: impl Into<String>,
        instructions: impl Into<String>,
        true_criteria: Option<&str>,
        false_criteria: Option<&str>,
    ) -> Self {
        let mut q = json!({
            "type": "noul",
            "instructions": instructions.into(),
        });
        if let (Some(t), Some(f)) = (true_criteria, false_criteria) {
            q["criteria"] = json!({ "true": t, "false": f });
        }
        self.questions.insert(name.into(), q);
        self
    }

    pub fn to_bounded_bytes(&self) -> Result<Vec<u8>> {
        let encoded = serde_json::to_vec(self)?;
        if encoded.len() > MAX_REQUEST_BYTES {
            bail!(
                "systemone request ({} bytes) exceeds {} byte cap",
                encoded.len(),
                MAX_REQUEST_BYTES
            );
        }
        Ok(encoded)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct SystemOneResponse {
    pub model: String,
    pub answers: HashMap<String, Value>,
    #[serde(default)]
    pub usage: Option<SystemOneUsage>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SystemOneUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ChoiceResult {
    pub choice: String,
    pub confidence: f64,
    pub probabilities: HashMap<String, f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScoreResult {
    /// Raw rubric index in `0..=(levels - 1)`.
    pub raw_score: f64,
    /// Normalized score in `0.0..=1.0`.
    pub normalized: f64,
    pub confidence: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NoulResult {
    pub probability: f64,
}

pub fn parse_choice(
    response: &SystemOneResponse,
    name: &str,
    allowed: &[&str],
) -> Result<ChoiceResult> {
    let val = response
        .answers
        .get(name)
        .ok_or_else(|| anyhow!("systemone_schema: missing choice answer '{name}'"))?;
    if val.get("type").and_then(Value::as_str) != Some("choice") {
        bail!("systemone_schema: '{name}' is not a choice answer");
    }
    let choice = val
        .get("choice")
        .and_then(Value::as_str)
        .filter(|c| allowed.contains(c))
        .ok_or_else(|| anyhow!("systemone_schema: unknown choice for '{name}'"))?
        .to_string();
    let confidence = require_unit_interval(val, "confidence", name)?;

    let mut probabilities = HashMap::new();
    if let Some(obj) = val.get("probabilities").and_then(Value::as_object) {
        let mut sum = 0.0;
        for (k, v) in obj {
            let p = v
                .as_f64()
                .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                .ok_or_else(|| {
                    anyhow!("systemone_schema: invalid probability for '{name}.{k}'")
                })?;
            sum += p;
            probabilities.insert(k.clone(), p);
        }
        if !probabilities.is_empty() && (sum - 1.0).abs() > 0.05 {
            bail!("systemone_schema: probabilities for '{name}' sum to {sum:.3}, expected ~1.0");
        }
    }

    Ok(ChoiceResult {
        choice,
        confidence,
        probabilities,
    })
}

pub fn parse_score(
    response: &SystemOneResponse,
    name: &str,
    rubric_levels: usize,
) -> Result<ScoreResult> {
    if rubric_levels < 2 {
        bail!("rubric_levels must be >= 2");
    }
    let val = response
        .answers
        .get(name)
        .ok_or_else(|| anyhow!("systemone_schema: missing score answer '{name}'"))?;
    if val.get("type").and_then(Value::as_str) != Some("score") {
        bail!("systemone_schema: '{name}' is not a score answer");
    }
    let raw_score = val
        .get("score")
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| anyhow!("systemone_schema: missing numeric score for '{name}'"))?;
    let max_idx = (rubric_levels - 1) as f64;
    if !(0.0..=max_idx).contains(&raw_score) {
        bail!("systemone_schema: score {raw_score} outside 0..={max_idx} for '{name}'");
    }
    let confidence = require_unit_interval(val, "confidence", name)?;
    Ok(ScoreResult {
        raw_score,
        normalized: raw_score / max_idx,
        confidence,
    })
}

pub fn parse_noul(response: &SystemOneResponse, name: &str) -> Result<NoulResult> {
    let val = response
        .answers
        .get(name)
        .ok_or_else(|| anyhow!("systemone_schema: missing noul answer '{name}'"))?;
    if val.get("type").and_then(Value::as_str) != Some("noul") {
        bail!("systemone_schema: '{name}' is not a noul answer");
    }
    let probability = require_unit_interval(val, "noul", name)?;
    Ok(NoulResult { probability })
}

fn require_unit_interval(val: &Value, field: &str, answer_name: &str) -> Result<f64> {
    let n = val
        .get(field)
        .and_then(Value::as_f64)
        .filter(|n| n.is_finite())
        .ok_or_else(|| anyhow!("systemone_schema: missing '{field}' on '{answer_name}'"))?;
    if !(0.0..=1.0).contains(&n) {
        bail!("systemone_schema: '{field}' ({n}) outside 0..=1 on '{answer_name}'");
    }
    Ok(n)
}
