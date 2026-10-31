//! Pi with ChatGPT OAuth. Local-only: task pods must remain credential-free.
use serde_json::{json, Value};
use tokio::process::Command;

pub const TEMPLATES: [&str; 2] = ["github_issue_resolver", "github_pr_reviewer"];
pub const MODEL: &str = "gpt-6.1-sol";

fn base_command() -> Command {
    let mut command = Command::new(std::env::var("AUTONOMOUS_PI_BIN").unwrap_or_else(|_| "pi".into()));
    super::worker::restrict_claude_environment(&mut command);
    command.env_remove("NEXUSMIND_API_KEY");
    command
}

/// Check only readiness metadata, never request or emit the credential itself.
pub async fn preflight() -> Result<(), &'static str> {
    let mut command = base_command();
    command.args(["auth", "check", "--provider", "openai-codex", "--json"]);
    command.stdin(std::process::Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(std::time::Duration::from_secs(30), command.output())
        .await.map_err(|_| "pi_auth_check_timeout")?
        .map_err(|_| "pi_cli_unavailable")?;
    let value: Value = serde_json::from_slice(&output.stdout).map_err(|_| "pi_auth_check_failed")?;
    if output.status.success() && value["status"] == "ready" && value["authType"] == "oauth" {
        Ok(())
    } else {
        Err("pi_oauth_required")
    }
}

pub fn command(reviewer: bool) -> Command {
    let mut command = base_command();
    // Do not load untrusted repository extensions/configuration or unrelated MCPs.
    command.args([
        "--mode", "json", "--print", "--no-session", "--no-extensions",
        "--no-skills", "--no-prompt-templates", "--no-themes", "--no-mcp",
        "--no-context-files", "--no-approve",
        "--tools", if reviewer { "read,grep,find,ls" } else { "read,bash,edit,write,grep,find,ls" },
    ]);
    command.arg("--model").arg(format!("openai-codex/{MODEL}"));
    // Prompt is delivered over stdin, not exposed in process arguments.
    command.stdin(std::process::Stdio::piped());
    command
}

/// Normalize Pi's authoritative message_end events, not duplicated agent_end messages.
pub fn parse(stdout: &[u8]) -> anyhow::Result<(Value, Value)> {
    anyhow::ensure!(stdout.len() <= 32 * 1_048_576, "pi_event_stream_too_large");
    let mut last = None;
    let mut settled = false;
    let mut aborted = false;
    let mut usage = [0i64; 4];
    let mut turns = 0;
    let mut model = String::new();
    let mut failed = false;
    for (index, line) in stdout.split(|b| *b == b'\n').filter(|line| !line.is_empty()).enumerate() {
        anyhow::ensure!(index < 100_000 && line.len() <= 4 * 1_048_576, "pi_event_stream_limit_exceeded");
        let event: Value = serde_json::from_slice(line)?;
        match event["type"].as_str() {
            Some("agent_settled") => {
                settled = true;
                aborted = event["aborted"].as_bool().unwrap_or(true);
            }
            Some("message_end") if event["message"]["role"] == "assistant" => {
                let message = &event["message"];
                failed = message["stopReason"].as_str() != Some("stop");
                turns += 1;
                model = message["model"].as_str().unwrap_or_default().to_string();
                for (i, key) in ["input", "cacheRead", "cacheWrite", "output"].iter().enumerate() {
                    usage[i] += message["usage"][key].as_i64().unwrap_or(0).max(0);
                }
                last = message["content"].as_array().map(|blocks| blocks.iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str()).collect::<Vec<_>>().join("\n"));
            }
            _ => {}
        }
    }
    anyhow::ensure!(settled && !aborted && !failed, "pi_run_incomplete_or_failed");
    let text = last.filter(|text| !text.trim().is_empty()).ok_or_else(|| anyhow::anyhow!("pi_result_missing"))?;
    // Subscription usage is not an API invoice: do not report a fictitious USD cost.
    Ok((json!({"type":"result", "subtype":"success", "executor":"pi", "result":text,
        "num_turns":turns, "modelUsage":{model: {}},
        "usage":{"input_tokens":usage[0], "cache_read_input_tokens":usage[1],
            "cache_creation_input_tokens":usage[2], "output_tokens":usage[3]}}),
        json!({"format":"pi-jsonl", "turns":turns})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalizes_completed_run_without_claiming_subscription_cost() {
        let bytes = br#"{"type":"message_end","message":{"role":"assistant","model":"gpt-6.1-sol","stopReason":"stop","content":[{"type":"text","text":"ok"}],"usage":{"input":12,"output":3,"cacheRead":4}}}
{"type":"agent_end","messages":[]}
{"type":"agent_settled","aborted":false}
"#;
        let (result, _) = parse(bytes).unwrap();
        assert_eq!(result["result"], "ok");
        assert_eq!(result["usage"]["input_tokens"], 12);
        assert!(result.get("total_cost_usd").is_none());
        assert_eq!(result["modelUsage"]["gpt-6.1-sol"], json!({}));
        assert!(parse(b"{\"type\":\"agent_end\"}\n").is_err());
    }
    #[test]
    fn reviewer_has_only_read_tools_and_no_project_resources() {
        let command = command(true);
        let args: Vec<_> = command.as_std().get_args().map(|arg| arg.to_string_lossy().to_string()).collect();
        assert!(args.windows(2).any(|pair| pair == ["--tools", "read,grep,find,ls"]));
        assert!(args.windows(2).any(|pair| pair == ["--model", "openai-codex/gpt-6.1-sol"]));
        for flag in ["--no-approve", "--no-extensions", "--no-mcp", "--no-context-files", "--no-session"] {
            assert!(args.iter().any(|arg| arg == flag));
        }
    }

    #[test]
    fn counts_multiple_assistant_messages_not_turn_or_agent_duplicates() {
        let bytes = br#"{"type":"message_end","message":{"role":"assistant","model":"gpt-6.1-sol","stopReason":"toolUse","content":[{"type":"toolCall","name":"read"}],"usage":{"input":10,"output":2,"cacheRead":5}}}
{"type":"turn_end","message":{"role":"assistant","usage":{"input":10,"output":2}}}
{"type":"message_end","message":{"role":"assistant","model":"gpt-6.1-sol","stopReason":"stop","content":[{"type":"text","text":"final"}],"usage":{"input":20,"output":3,"cacheRead":6}}}
{"type":"agent_end","messages":[]}
{"type":"agent_settled","aborted":false}"#;
        let (result, _) = parse(bytes).unwrap();
        assert_eq!(result["result"], "final");
        assert_eq!(result["num_turns"], 2);
        assert_eq!(result["usage"]["input_tokens"], 30);
        assert_eq!(result["usage"]["cache_read_input_tokens"], 11);
        assert_eq!(result["usage"]["output_tokens"], 5);
    }

    #[test]
    fn rejects_aborted_and_error_runs() {
        assert!(parse(br#"{"type":"agent_settled","aborted":true}"#).is_err());
        assert!(parse(br#"{"type":"message_end","message":{"role":"assistant","stopReason":"error","content":[{"type":"text","text":"partial"}]}}
{"type":"agent_settled","aborted":false}"#).is_err());
    }
}
