//! Run telemetry (plan §7 F0, §8): raw cost, cache and latency for every finished
//! autonomous run, so "cost per accepted change" can be computed once the router
//! (F3) and trajectory store (F5) exist. An unknown value stays `None` — never 0 —
//! so a run whose cost was not reported is never counted as free.

use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RunMetrics {
    pub model: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub cache_write_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub num_turns: Option<i64>,
}

/// Locates Claude Code's `result` event in a finished run's result: the worker
/// stores it either as-is or wrapped as `{"code": …, "result": <event>}`.
pub fn run_result_event(run_result: &Value) -> Option<&Value> {
    let is_event = |value: &Value| {
        value.get("type").and_then(Value::as_str) == Some("result")
            || value.get("total_cost_usd").is_some()
    };
    if is_event(run_result) {
        return Some(run_result);
    }
    run_result.get("result").filter(|inner| is_event(inner))
}

/// Reads Claude Code's final `result` event. Tolerates any missing field.
pub fn extract_run_metrics(result: &Value) -> RunMetrics {
    let count = |pointer: &str| {
        result
            .pointer(pointer)
            .and_then(Value::as_i64)
            .filter(|n| *n >= 0)
    };
    let model = result
        .get("modelUsage")
        .and_then(Value::as_object)
        .and_then(|models| {
            models
                .iter()
                .map(|(name, usage)| {
                    let cost = usage.get("costUSD").and_then(Value::as_f64).unwrap_or(0.0);
                    (name, cost)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(name, _)| name.clone())
        });
    RunMetrics {
        model,
        input_tokens: count("/usage/input_tokens"),
        cached_input_tokens: count("/usage/cache_read_input_tokens"),
        cache_write_tokens: count("/usage/cache_creation_input_tokens"),
        output_tokens: count("/usage/output_tokens"),
        cost_usd: result
            .get("total_cost_usd")
            .and_then(Value::as_f64)
            .filter(|cost| cost.is_finite() && *cost >= 0.0),
        duration_ms: count("/duration_ms"),
        num_turns: count("/num_turns"),
    }
}

/// Metrics of a finished run: its own result event, or — for a fan-out run that
/// ran one session per issue — the sum over every per-issue `usage` event. A total
/// is only known when every part is known, so a partly unknown cost is `None`.
pub fn run_metrics(run_result: &Value) -> RunMetrics {
    if let Some(event) = run_result_event(run_result) {
        return extract_run_metrics(event);
    }
    let Some(issues) = run_result.get("issues").and_then(Value::as_array) else {
        return RunMetrics::default();
    };
    let parts: Vec<RunMetrics> = issues
        .iter()
        .map(|issue| {
            issue
                .get("usage")
                .map(extract_run_metrics)
                .unwrap_or_default()
        })
        .collect();
    if parts.is_empty() {
        return RunMetrics::default();
    }
    // Known only when known in every part: a partial sum would under-report.
    fn total<T: Copy + std::iter::Sum<T>>(
        parts: &[RunMetrics],
        field: impl Fn(&RunMetrics) -> Option<T>,
    ) -> Option<T> {
        parts
            .iter()
            .map(&field)
            .collect::<Option<Vec<T>>>()
            .map(|values| values.into_iter().sum())
    }
    // The model of the most expensive part.
    let model = parts
        .iter()
        .filter(|part| part.model.is_some())
        .max_by(|a, b| {
            a.cost_usd
                .unwrap_or(0.0)
                .total_cmp(&b.cost_usd.unwrap_or(0.0))
        })
        .and_then(|part| part.model.clone());
    RunMetrics {
        model,
        input_tokens: total(&parts, |p| p.input_tokens),
        cached_input_tokens: total(&parts, |p| p.cached_input_tokens),
        cache_write_tokens: total(&parts, |p| p.cache_write_tokens),
        output_tokens: total(&parts, |p| p.output_tokens),
        cost_usd: total(&parts, |p| p.cost_usd),
        duration_ms: total(&parts, |p| p.duration_ms),
        num_turns: total(&parts, |p| p.num_turns),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_cost_cache_and_latency_from_the_result_event() {
        let result = json!({
            "type": "result",
            "subtype": "success",
            "duration_ms": 48210,
            "num_turns": 17,
            "total_cost_usd": 0.4213,
            "usage": {
                "input_tokens": 1200,
                "cache_read_input_tokens": 90000,
                "cache_creation_input_tokens": 15000,
                "output_tokens": 3400
            },
            "modelUsage": {
                "claude-haiku-4-5": {"costUSD": 0.0123},
                "claude-sonnet-5": {"costUSD": 0.409}
            }
        });
        assert_eq!(
            extract_run_metrics(&result),
            RunMetrics {
                model: Some("claude-sonnet-5".into()),
                input_tokens: Some(1200),
                cached_input_tokens: Some(90000),
                cache_write_tokens: Some(15000),
                output_tokens: Some(3400),
                cost_usd: Some(0.4213),
                duration_ms: Some(48210),
                num_turns: Some(17),
            }
        );
    }

    #[test]
    fn finds_the_result_event_whether_wrapped_or_not() {
        let event = json!({"type": "result", "total_cost_usd": 0.2});
        assert_eq!(run_result_event(&event), Some(&event));
        let wrapped = json!({"code": "completed", "result": event.clone(), "stream": []});
        assert_eq!(run_result_event(&wrapped), Some(&event));
        assert_eq!(run_result_event(&json!({"code": "claude_timeout"})), None);
    }

    #[test]
    fn a_fanout_run_sums_its_per_issue_sessions() {
        let fanout = json!({
            "code": "fanout_completed",
            "issues": [
                {"issue": 1, "status": "succeeded", "usage": {
                    "total_cost_usd": 0.25, "duration_ms": 1000, "num_turns": 3,
                    "usage": {"input_tokens": 10, "output_tokens": 5},
                    "modelUsage": {"claude-sonnet-5": {"costUSD": 0.25}}}},
                {"issue": 2, "status": "failed", "usage": {
                    "total_cost_usd": 0.75, "duration_ms": 3000, "num_turns": 9,
                    "usage": {"input_tokens": 30, "output_tokens": 15},
                    "modelUsage": {"claude-opus-5-5": {"costUSD": 0.75}}}}
            ]
        });
        let metrics = run_metrics(&fanout);
        assert_eq!(metrics.cost_usd, Some(1.0));
        assert_eq!(metrics.input_tokens, Some(40));
        assert_eq!(metrics.output_tokens, Some(20));
        assert_eq!(metrics.duration_ms, Some(4000));
        assert_eq!(metrics.num_turns, Some(12));
        assert_eq!(metrics.model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(
            metrics.cached_input_tokens, None,
            "unknown in every part stays unknown"
        );
    }

    #[test]
    fn a_fanout_total_is_unknown_when_any_part_is_unknown() {
        let fanout = json!({"code": "fanout_completed", "issues": [
            {"issue": 1, "usage": {"total_cost_usd": 0.25}},
            {"issue": 2, "code": "claude_timeout"}
        ]});
        assert_eq!(run_metrics(&fanout).cost_usd, None);
    }

    #[test]
    fn a_single_session_run_reads_its_own_event() {
        let run = json!({"code": "completed", "result": {"type": "result", "total_cost_usd": 0.3}});
        assert_eq!(run_metrics(&run).cost_usd, Some(0.3));
        assert_eq!(
            run_metrics(&json!({"code": "claude_timeout"})),
            RunMetrics::default()
        );
    }

    #[test]
    fn a_missing_field_is_unknown_not_zero() {
        let metrics = extract_run_metrics(&json!({"type": "result", "total_cost_usd": 0.1}));
        assert_eq!(metrics.cost_usd, Some(0.1));
        assert_eq!(metrics.input_tokens, None);
        assert_eq!(metrics.model, None);
        assert_eq!(
            extract_run_metrics(&json!("not an object")),
            RunMetrics::default()
        );
    }

    #[test]
    fn negative_or_non_numeric_values_are_rejected() {
        let metrics = extract_run_metrics(&json!({
            "total_cost_usd": -1.0,
            "duration_ms": "fast",
            "usage": {"input_tokens": -5}
        }));
        assert_eq!(metrics.cost_usd, None);
        assert_eq!(metrics.duration_ms, None);
        assert_eq!(metrics.input_tokens, None);
    }
}
