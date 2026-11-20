//! Runtime error monitoring (openspec `runtime-error-monitoring`).
//!
//! A Cloud Logging sink sends `severity>=ERROR` entries to Pub/Sub, whose push
//! subscription posts them to `/v1/monitor/gcp/:source_id`. A Cloud Monitoring
//! alert policy can publish its incidents to the same topic. Each error is
//! grouped by fingerprint: a new group alerts Slack and opens a GitHub issue,
//! repeats only bump a counter and alert at most once per throttle window.
//! Everything in a payload is untrusted runtime text.

pub mod delivery;
pub mod oidc;

use std::sync::OnceLock;

use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// One error, whatever produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeError {
    pub service: String,
    pub severity: String,
    /// First line of the message, at most 200 chars.
    pub title: String,
    pub message: String,
    pub stack: Option<String>,
    pub log_url: Option<String>,
    /// Groups errors without a stack (alert incidents group by policy).
    pub group_hint: Option<String>,
}

const SEVERE: [&str; 4] = ["ERROR", "CRITICAL", "ALERT", "EMERGENCY"];
const MESSAGE_MAX: usize = 8_000;

/// Decodes a Pub/Sub push body. `Ok(None)`: valid but nothing to report (a
/// low severity entry, a closed incident). `Err`: malformed.
pub fn parse_push(body: &Value) -> Result<Option<RuntimeError>, &'static str> {
    let data = body
        .pointer("/message/data")
        .and_then(Value::as_str)
        .ok_or("pubsub_data_missing")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "pubsub_data_not_base64")?;
    let payload: Value = serde_json::from_slice(&bytes).map_err(|_| "pubsub_data_not_json")?;
    if payload.get("incident").is_some() {
        Ok(from_incident(&payload["incident"]))
    } else {
        Ok(from_log_entry(&payload))
    }
}

fn text(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn capped(text: &str, max: usize) -> String {
    text.chars().take(max).collect()
}

/// A Cloud Logging `LogEntry` (Cloud Run, GKE, Cloud Functions, anything).
pub fn from_log_entry(entry: &Value) -> Option<RuntimeError> {
    let severity = text(entry, "/severity").unwrap_or_default().to_ascii_uppercase();
    if !SEVERE.contains(&severity.as_str()) {
        return None;
    }
    let labels = ["service_name", "container_name", "function_name", "job_name", "module_id"];
    let service = labels
        .iter()
        .find_map(|label| text(entry, &format!("/resource/labels/{label}")))
        .or_else(|| text(entry, "/resource/type"))
        .unwrap_or_else(|| "unknown".into());
    // Error Reporting's field first, then the shapes pino, winston and Nest log.
    let stack = ["/jsonPayload/stack_trace", "/jsonPayload/err/stack", "/jsonPayload/error/stack", "/jsonPayload/stack"]
        .iter()
        .find_map(|p| text(entry, p));
    let message = ["/jsonPayload/message", "/jsonPayload/msg", "/jsonPayload/err/message", "/jsonPayload/error/message", "/textPayload"]
        .iter()
        .find_map(|p| text(entry, p))
        .or_else(|| text(entry, "/jsonPayload/error"))
        .or_else(|| stack.clone())
        .unwrap_or_else(|| "(no message)".into());
    // A plain-text stack trace: the message is its first line, the rest the stack.
    let stack = stack.or_else(|| message.lines().nth(1).map(|_| message.clone()));
    let title = capped(message.lines().next().unwrap_or_default(), 200);
    let project = text(entry, "/resource/labels/project_id");
    let insert_id = text(entry, "/insertId").filter(|id| id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'));
    let log_url = match (project, insert_id) {
        (Some(project), Some(id)) if project.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') => Some(format!(
            "https://console.cloud.google.com/logs/query;query=insertId%3D%22{id}%22;duration=P7D?project={project}"
        )),
        _ => None,
    };
    Some(RuntimeError {
        service: capped(&service, 100),
        severity,
        title,
        message: capped(&message, MESSAGE_MAX),
        stack: stack.map(|s| capped(&s, MESSAGE_MAX)),
        log_url,
        group_hint: None,
    })
}

/// A Cloud Monitoring incident (alert policy with a Pub/Sub channel).
pub fn from_incident(incident: &Value) -> Option<RuntimeError> {
    if text(incident, "/state").as_deref() != Some("open") {
        return None;
    }
    let policy = text(incident, "/policy_name").unwrap_or_else(|| "alert policy".into());
    let summary = text(incident, "/summary").unwrap_or_else(|| policy.clone());
    let condition = text(incident, "/condition_name").unwrap_or_default();
    let severity = match text(incident, "/severity").unwrap_or_default().to_ascii_lowercase().as_str() {
        "critical" => "CRITICAL",
        _ => "ERROR",
    };
    Some(RuntimeError {
        service: capped(&text(incident, "/resource_display_name").unwrap_or_else(|| policy.clone()), 100),
        severity: severity.into(),
        title: capped(&format!("Alert: {}", summary.lines().next().unwrap_or_default()), 200),
        message: capped(&summary, MESSAGE_MAX),
        stack: None,
        log_url: text(incident, "/url")
            .filter(|u| u.starts_with("https://console.cloud.google.com/") && !u.contains(['<', '>', '|', ' ', '\n'])),
        group_hint: Some(format!("incident:{policy}:{condition}")),
    })
}

/// Replaces the parts of a message that change between occurrences of the
/// same error (ids, numbers, emails, quoted values) with placeholders.
pub fn normalize_message(message: &str) -> String {
    static RULES: OnceLock<Vec<(regex::Regex, &'static str)>> = OnceLock::new();
    let rules = RULES.get_or_init(|| {
        [
            (r"[\w.+-]+@[\w-]+\.[\w.-]+", "<email>"),
            (r"(?i)\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b", "<uuid>"),
            (r"(?i)\b(0x)?[0-9a-f]{12,}\b", "<hex>"),
            (r#""[^"]*"|'[^']*'|`[^`]*`"#, "<str>"),
            (r"\d+(\.\d+)?", "<n>"),
        ]
        .into_iter()
        .map(|(pattern, placeholder)| (regex::Regex::new(pattern).expect("valid pattern"), placeholder))
        .collect()
    });
    rules
        .iter()
        .fold(message.trim().to_string(), |text, (rule, placeholder)| rule.replace_all(&text, *placeholder).into_owned())
}

/// The first stack frame from the service's own code, without line and
/// column numbers (they move on every deploy).
pub fn top_frame(stack: &str) -> Option<String> {
    static POSITION: OnceLock<regex::Regex> = OnceLock::new();
    let position = POSITION.get_or_init(|| regex::Regex::new(r"(:\d+)+\)?$|, line \d+").expect("valid pattern"));
    stack
        .lines()
        .skip(1)
        .map(str::trim)
        .filter(|line| line.starts_with("at ") || line.starts_with("File \""))
        .find(|line| !line.contains("node_modules") && !line.contains("node:internal") && !line.contains("site-packages"))
        .map(|line| position.replace(line, "").into_owned())
}

/// Same source + service + kind of error + place in the code → same group.
pub fn fingerprint(source_id: &str, error: &RuntimeError) -> String {
    let key = match &error.group_hint {
        Some(hint) => hint.clone(),
        None => format!(
            "{}|{}",
            normalize_message(&error.title),
            error.stack.as_deref().and_then(top_frame).unwrap_or_default()
        ),
    };
    hex::encode(Sha256::digest(format!("{source_id}|{}|{key}", error.service).as_bytes()))
}

/// What one more occurrence of a group should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// First occurrence: alert and open an issue.
    New,
    /// Still happening and the throttle window passed: alert again.
    Notify,
    /// Was resolved and came back: alert and open a regression issue.
    Regressed,
    /// Count it, say nothing.
    Quiet,
}

/// `status` and `last_alert_at` of the existing group, if any.
pub fn decide(existing: Option<(&str, Option<DateTime<Utc>>)>, throttle_minutes: i64, now: DateTime<Utc>) -> Action {
    match existing {
        None => Action::New,
        Some(("muted", _)) => Action::Quiet,
        Some(("resolved", _)) => Action::Regressed,
        Some((_, Some(last))) if now - last < chrono::Duration::minutes(throttle_minutes) => Action::Quiet,
        Some(_) => Action::Notify,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn push(payload: Value) -> Value {
        let data = base64::engine::general_purpose::STANDARD.encode(payload.to_string());
        json!({"message": {"data": data, "messageId": "1"}, "subscription": "projects/p/subscriptions/s"})
    }

    #[test]
    fn cloud_run_json_error_with_stack_is_parsed() {
        let entry = json!({
            "severity": "ERROR", "insertId": "abc123",
            "resource": {"type": "cloud_run_revision", "labels": {"service_name": "sales", "project_id": "my-proj"}},
            "jsonPayload": {"message": "Order 42 not found", "stack_trace": "Error: Order 42 not found\n    at find (/app/dist/orders.js:10:5)"}
        });
        let error = parse_push(&push(entry)).unwrap().unwrap();
        assert_eq!(error.service, "sales");
        assert_eq!(error.title, "Order 42 not found");
        assert!(error.stack.unwrap().contains("orders.js"));
        assert_eq!(
            error.log_url.unwrap(),
            "https://console.cloud.google.com/logs/query;query=insertId%3D%22abc123%22;duration=P7D?project=my-proj"
        );
    }

    #[test]
    fn gke_text_payload_stack_splits_title_and_stack() {
        let entry = json!({
            "severity": "CRITICAL",
            "resource": {"type": "k8s_container", "labels": {"container_name": "api"}},
            "textPayload": "TypeError: x is undefined\n    at handler (/srv/app.js:3:1)"
        });
        let error = from_log_entry(&entry).unwrap();
        assert_eq!(error.service, "api");
        assert_eq!(error.title, "TypeError: x is undefined");
        assert!(error.stack.is_some());
        assert_eq!(error.log_url, None);
    }

    #[test]
    fn pino_shape_and_low_severity() {
        let entry = json!({"severity": "ERROR", "resource": {"labels": {"function_name": "fn"}},
            "jsonPayload": {"msg": "boom", "err": {"stack": "Error: boom\n    at a (/x.js:1:1)"}}});
        let error = from_log_entry(&entry).unwrap();
        assert_eq!((error.service.as_str(), error.title.as_str()), ("fn", "boom"));
        assert!(from_log_entry(&json!({"severity": "WARNING", "textPayload": "meh"})).is_none());
        assert!(from_log_entry(&json!({"textPayload": "no severity"})).is_none());
    }

    #[test]
    fn unsafe_log_url_parts_are_dropped() {
        let entry = json!({"severity": "ERROR", "insertId": "a\"b", "textPayload": "x",
            "resource": {"labels": {"project_id": "p"}}});
        assert_eq!(from_log_entry(&entry).unwrap().log_url, None);
    }

    #[test]
    fn open_incidents_report_and_closed_ones_do_not() {
        let incident = json!({"incident": {"state": "open", "policy_name": "Restarts", "condition_name": "restart > 3",
            "summary": "Container restarted 5 times", "resource_display_name": "sales", "url": "https://console.cloud.google.com/monitoring/x"}});
        let error = parse_push(&push(incident)).unwrap().unwrap();
        assert_eq!(error.title, "Alert: Container restarted 5 times");
        assert_eq!(error.group_hint.as_deref(), Some("incident:Restarts:restart > 3"));
        let closed = json!({"incident": {"state": "closed", "policy_name": "Restarts"}});
        assert_eq!(parse_push(&push(closed)).unwrap(), None);
    }

    #[test]
    fn malformed_push_bodies_are_errors() {
        assert!(parse_push(&json!({})).is_err());
        assert!(parse_push(&json!({"message": {"data": "%%%"}})).is_err());
        let not_json = base64::engine::general_purpose::STANDARD.encode("hello");
        assert!(parse_push(&json!({"message": {"data": not_json}})).is_err());
    }

    #[test]
    fn messages_differing_only_in_values_normalize_alike() {
        assert_eq!(normalize_message("User 123 not found"), normalize_message("User 456 not found"));
        assert_eq!(
            normalize_message("bad id 3f2b8c1e-1d2a-4b5c-9d8e-7f6a5b4c3d2e for a@b.com"),
            "bad id <uuid> for <email>"
        );
        assert_eq!(normalize_message(r#"key "abc" missing"#), "key <str> missing");
        assert_eq!(normalize_message("token deadbeefdeadbeef"), "token <hex>");
        assert_ne!(normalize_message("User not found"), normalize_message("Order not found"));
    }

    #[test]
    fn top_frame_skips_dependencies_and_drops_positions() {
        let stack = "Error: x\n    at Socket.emit (node:internal/streams:1:1)\n    at q (/app/node_modules/pg/x.js:2:2)\n    at save (/app/dist/orders.js:10:5)";
        assert_eq!(top_frame(stack).as_deref(), Some("at save (/app/dist/orders.js"));
        let python = "Traceback (most recent call last):\n  File \"/app/main.py\", line 12, in run";
        assert_eq!(top_frame(python).as_deref(), Some("File \"/app/main.py\", in run"));
    }

    fn error(title: &str, stack: Option<&str>) -> RuntimeError {
        RuntimeError {
            service: "sales".into(),
            severity: "ERROR".into(),
            title: title.into(),
            message: title.into(),
            stack: stack.map(str::to_string),
            log_url: None,
            group_hint: None,
        }
    }

    #[test]
    fn fingerprint_groups_by_message_shape_and_code_location() {
        let a = fingerprint("s", &error("User 1 not found", Some("E\n    at f (/app/a.js:1:1)")));
        let b = fingerprint("s", &error("User 2 not found", Some("E\n    at f (/app/a.js:9:9)")));
        let other_place = fingerprint("s", &error("User 1 not found", Some("E\n    at g (/app/b.js:1:1)")));
        let other_source = fingerprint("t", &error("User 1 not found", Some("E\n    at f (/app/a.js:1:1)")));
        assert_eq!(a, b);
        assert_ne!(a, other_place);
        assert_ne!(a, other_source);
    }

    #[test]
    fn decide_alerts_once_per_window() {
        let now = Utc::now();
        assert_eq!(decide(None, 30, now), Action::New);
        assert_eq!(decide(Some(("open", Some(now - chrono::Duration::minutes(5)))), 30, now), Action::Quiet);
        assert_eq!(decide(Some(("open", Some(now - chrono::Duration::minutes(31)))), 30, now), Action::Notify);
        assert_eq!(decide(Some(("open", None)), 30, now), Action::Notify);
        assert_eq!(decide(Some(("muted", None)), 30, now), Action::Quiet);
        assert_eq!(decide(Some(("resolved", Some(now))), 30, now), Action::Regressed);
    }
}
