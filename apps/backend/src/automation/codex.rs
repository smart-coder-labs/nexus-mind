//! The Codex CLI as a factory executor (F3, API key). It runs only in a task pod:
//! its configuration (`sandbox::codex_config`) points every model request at the
//! egress proxy, which injects the OpenAI key. Its JSONL events are translated to
//! the shape of Claude Code's final `result` event, so evaluation, budgets and
//! telemetry treat both executors alike.
//!
//! Prices (`FACTORY_OPENAI_PRICES`, JSON `{"<model>": [input, cached_input,
//! output]}` in USD per million tokens) are required: a Codex run whose model
//! has no price does not start, because its cost could not be capped. Codex has
//! no turn limit: a run is bounded by its wall time, its cost cap and the
//! proxy's per-run request cap.

use serde_json::{json, Value};

/// Templates the Codex executor may run: code writing and review. Browser and
/// scanner templates depend on Claude-specific tooling.
pub const CODEX_TEMPLATES: [&str; 2] = ["github_issue_resolver", "github_pr_reviewer"];

/// The pod-side invocation: the prompt goes on stdin (`-`), never in the exec
/// request. The pod is the sandbox, so Codex's own one is bypassed (Landlock is
/// not available to an unprivileged pod user, and the pod holds no credential).
pub fn codex_invocation(model: &str, prompt: &str) -> (Vec<String>, Option<Vec<u8>>) {
    let argv = [
        "codex",
        "exec",
        "--json",
        "--ephemeral",
        "--skip-git-repo-check",
        "--dangerously-bypass-approvals-and-sandbox",
        "-C",
        crate::factory::sandbox::WORKSPACE,
        "-m",
        model,
        "-",
    ]
    .iter()
    .map(|part| part.to_string())
    .collect();
    (argv, Some(prompt.as_bytes().to_vec()))
}

/// Whether a model has a price. Codex bills a real API key, so a run whose cost
/// could not be measured (and so not capped) never starts.
pub fn priced(model: &str) -> bool {
    price(model).is_some()
}

/// USD per million tokens: input, cached input, output.
fn price(model: &str) -> Option<[f64; 3]> {
    let prices: Value = serde_json::from_str(&std::env::var("FACTORY_OPENAI_PRICES").ok()?).ok()?;
    let entry = prices.get(model)?.as_array()?;
    let at = |i: usize| entry.get(i)?.as_f64().filter(|p| p.is_finite() && *p >= 0.0);
    Some([at(0)?, at(1)?, at(2)?])
}

/// The cost of a run's usage, if the model has a price.
pub fn usage_cost(prices: Option<[f64; 3]>, input: i64, cached: i64, output: i64) -> Option<f64> {
    let [input_price, cached_price, output_price] = prices?;
    let uncached = (input - cached).max(0) as f64;
    Some((uncached * input_price + cached.max(0) as f64 * cached_price + output.max(0) as f64 * output_price) / 1e6)
}

/// Translates `codex exec --json` output into a Claude-shaped `result` event and
/// a stream summary. The last agent message is the result text.
pub fn parse_codex_event_stream(stdout: &[u8], model: &str) -> anyhow::Result<(Value, Value)> {
    parse_with_prices(stdout, model, price(model))
}

fn parse_with_prices(stdout: &[u8], model: &str, prices: Option<[f64; 3]>) -> anyhow::Result<(Value, Value)> {
    const MAX_STREAM_BYTES: usize = 32 * 1_048_576;
    const MAX_LINE_BYTES: usize = 4 * 1_048_576;
    if stdout.len() > MAX_STREAM_BYTES {
        anyhow::bail!("codex_event_stream_too_large")
    }
    let text = super::worker::sanitize_output(stdout, MAX_STREAM_BYTES);
    let mut counts = std::collections::BTreeMap::<String, usize>::new();
    let (mut input, mut cached, mut output, mut completed) = (0i64, 0i64, 0i64, 0i64);
    let mut last_message: Option<String> = None;
    let mut failure: Option<String> = None;
    let mut last_error: Option<String> = None;
    let mut lines = 0usize;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        lines += 1;
        if lines > 100_000 || line.len() > MAX_LINE_BYTES {
            anyhow::bail!("codex_event_stream_limit_exceeded")
        }
        // Codex may print non-JSON notices; only events count.
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(kind) = event.get("type").and_then(Value::as_str) else {
            continue;
        };
        *counts.entry(kind.to_string()).or_default() += 1;
        match kind {
            "item.completed" => {
                if event.pointer("/item/type").and_then(Value::as_str) == Some("agent_message") {
                    if let Some(message) = event.pointer("/item/text").and_then(Value::as_str) {
                        last_message = Some(message.to_string());
                    }
                }
            }
            "turn.completed" => {
                completed += 1;
                let usage = |name: &str| event.pointer(&format!("/usage/{name}")).and_then(Value::as_i64).unwrap_or(0);
                input += usage("input_tokens");
                cached += usage("cached_input_tokens");
                output += usage("output_tokens");
            }
            // A bare `error` can be a transient reconnect; a failed turn is final.
            "error" => {
                last_error = event.get("message").and_then(Value::as_str).map(str::to_string);
            }
            "turn.failed" => {
                failure = Some(
                    event
                        .pointer("/error/message")
                        .and_then(Value::as_str)
                        .unwrap_or("turn failed")
                        .to_string(),
                );
            }
            _ => {}
        }
    }
    let stream = json!({"format": "codex-jsonl", "events": counts, "line_count": lines});
    // A turn that failed (rate limit, auth, context) ends the run as a failure
    // even if the model said something before; so does a run with no finished turn.
    if let Some(failure) = failure.as_deref() {
        anyhow::bail!(codex_failure_code(failure));
    }
    let Some(message) = last_message.filter(|_| completed > 0) else {
        // No finished turn: the last error, if any, says why.
        anyhow::bail!(last_error.as_deref().map_or("codex_result_missing", codex_failure_code));
    };
    let cost = usage_cost(prices, input, cached, output);
    let mut result = json!({
        "type": "result",
        "subtype": "success",
        "executor": "codex",
        "result": message,
        // Claude's convention: input excludes cache reads. Codex has no cache
        // writes to report and no comparable turn count, so none is given.
        "usage": {
            "input_tokens": (input - cached).max(0),
            "cache_read_input_tokens": cached,
            "cache_creation_input_tokens": 0,
            "output_tokens": output,
        },
        "modelUsage": {model: {"costUSD": cost.unwrap_or(0.0)}},
    });
    if let Some(cost) = cost {
        result["total_cost_usd"] = json!(cost);
    }
    Ok((result, stream))
}

/// A tenant-visible code for a Codex failure message.
pub fn codex_failure_code(message: &str) -> &'static str {
    let message = message.to_ascii_lowercase();
    if message.contains("401") || message.contains("unauthorized") || message.contains("api key") {
        "codex_auth_required"
    } else if message.contains("503") || message.contains("upstream not configured") {
        "codex_key_not_configured"
    } else if message.contains("429") || message.contains("rate limit") || message.contains("quota") {
        "codex_rate_limited"
    } else {
        "codex_failed"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUN: &str = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.completed","item":{"id":"item_0","type":"reasoning","text":"thinking"}}
{"type":"item.completed","item":{"id":"item_1","type":"command_execution","command":"ls","exit_code":0}}
{"type":"item.completed","item":{"id":"item_2","type":"agent_message","text":"{\"summary\":\"done\"}"}}
{"type":"turn.completed","usage":{"input_tokens":19355,"cached_input_tokens":7936,"output_tokens":120,"reasoning_output_tokens":40}}
"#;

    #[test]
    fn a_codex_run_becomes_a_claude_shaped_result() {
        let (result, stream) = parse_with_prices(RUN.as_bytes(), "gpt-6.1-sol", Some([2.0, 0.5, 8.0])).unwrap();
        assert_eq!(result["type"], "result");
        assert_eq!(result["result"], "{\"summary\":\"done\"}");
        assert_eq!(result["usage"]["input_tokens"], 19355 - 7936);
        assert_eq!(result["usage"]["cache_read_input_tokens"], 7936);
        assert_eq!(result["usage"]["output_tokens"], 120);
        assert_eq!(result["usage"]["cache_creation_input_tokens"], 0);
        let expected = ((19355.0 - 7936.0) * 2.0 + 7936.0 * 0.5 + 120.0 * 8.0) / 1e6;
        assert!((result["total_cost_usd"].as_f64().unwrap() - expected).abs() < 1e-12);
        assert_eq!(stream["events"]["item.completed"], 3);
        // The telemetry reader sees model, tokens and cost.
        let metrics = crate::factory::telemetry::extract_run_metrics(&result);
        assert_eq!(metrics.model.as_deref(), Some("gpt-6.1-sol"));
        assert_eq!(metrics.cached_input_tokens, Some(7936));
    }

    #[test]
    fn an_unpriced_model_has_an_unknown_cost_not_zero() {
        let (result, _) = parse_with_prices(RUN.as_bytes(), "gpt-6-luna", None).unwrap();
        assert!(result.get("total_cost_usd").is_none());
        assert_eq!(crate::factory::telemetry::extract_run_metrics(&result).cost_usd, None);
    }

    #[test]
    fn failures_map_to_codes() {
        let failed = br#"{"type":"turn.started"}
{"type":"error","message":"unexpected status 401 Unauthorized: Incorrect API key provided"}
{"type":"turn.failed","error":{"message":"unexpected status 401 Unauthorized"}}
"#;
        assert_eq!(parse_with_prices(failed, "m", None).unwrap_err().to_string(), "codex_auth_required");
        let unconfigured = br#"{"type":"turn.failed","error":{"message":"unexpected status 503 Service Unavailable: upstream not configured"}}"#;
        assert_eq!(parse_with_prices(unconfigured, "m", None).unwrap_err().to_string(), "codex_key_not_configured");
        assert_eq!(parse_with_prices(b"Reading additional input from stdin...\n", "m", None).unwrap_err().to_string(), "codex_result_missing");
        // A preamble before a failed turn is not a result.
        let preamble_then_429 = br#"{"type":"item.completed","item":{"type":"agent_message","text":"I'll look at the code."}}
{"type":"turn.failed","error":{"message":"unexpected status 429 Too Many Requests"}}
"#;
        assert_eq!(parse_with_prices(preamble_then_429, "m", None).unwrap_err().to_string(), "codex_rate_limited");
        // A transient error followed by a finished turn is a success.
        let recovered = br#"{"type":"error","message":"stream disconnected, retrying"}
{"type":"item.completed","item":{"type":"agent_message","text":"done"}}
{"type":"turn.completed","usage":{"input_tokens":1,"cached_input_tokens":0,"output_tokens":1}}
"#;
        assert_eq!(parse_with_prices(recovered, "m", None).unwrap().0["result"], "done");
        assert_eq!(codex_failure_code("429 Too Many Requests"), "codex_rate_limited");
    }

    #[test]
    fn the_prompt_goes_on_stdin() {
        let (argv, stdin) = codex_invocation("gpt-6-luna", "fix the bug");
        assert_eq!(argv.first().map(String::as_str), Some("codex"));
        assert_eq!(argv.last().map(String::as_str), Some("-"));
        assert!(argv.windows(2).any(|w| w == ["-m", "gpt-6-luna"]));
        assert!(!argv.iter().any(|a| a.contains("fix the bug")));
        assert_eq!(stdin.as_deref(), Some(&b"fix the bug"[..]));
    }
}
