//! Log drains: Supabase and Vercel post batches of log events to
//! `/v1/monitor/drain/:source_id` with the source's bearer token. Only the
//! errors in a batch become `RuntimeError`s.

use std::io::Read as _;

use serde_json::Value;

use super::{capped, strip_log_prefix, RuntimeError, MESSAGE_MAX};

pub const PROVIDERS: [&str; 3] = ["gcp", "supabase", "vercel"];
/// More errors than this in one batch is a storm: the rest are dropped.
const MAX_ERRORS_PER_BATCH: usize = 50;
/// A gzip body may not inflate past this (the drains send at most ~1 MB).
const MAX_INFLATED: u64 = 16 * 1024 * 1024;

/// The events of a drain body: a JSON array, one JSON object, or NDJSON;
/// gzip when `gzip` is set (Supabase can compress).
pub fn events(body: &[u8], gzip: bool) -> Result<Vec<Value>, &'static str> {
    let mut inflated = Vec::new();
    let body = if gzip {
        flate2::read::GzDecoder::new(body)
            .take(MAX_INFLATED)
            .read_to_end(&mut inflated)
            .map_err(|_| "drain_body_not_gzip")?;
        &inflated[..]
    } else {
        body
    };
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Array(items)) => Ok(items),
        Ok(item @ Value::Object(_)) => Ok(vec![item]),
        Ok(_) => Err("drain_body_not_events"),
        Err(_) => std::str::from_utf8(body)
            .map_err(|_| "drain_body_not_utf8")?
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).map_err(|_| "drain_body_not_json"))
            .collect(),
    }
}

/// The errors of a batch for `provider` (`project` is the Supabase project ref
/// or the Vercel team slug, used only for log links).
pub fn errors(provider: &str, project: &str, events: &[Value]) -> Vec<RuntimeError> {
    events
        .iter()
        .filter_map(|event| match provider {
            "supabase" => from_supabase(event, project),
            "vercel" => from_vercel(event, project),
            _ => None,
        })
        .take(MAX_ERRORS_PER_BATCH)
        .collect()
}

/// Follows `path`, stepping into the first element of any array on the way
/// (Supabase nests `metadata`, `parsed`, `response`… as one-element arrays).
fn field<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        while let Value::Array(items) = current {
            current = items.first()?;
        }
        current = current.get(*key)?;
    }
    while let Value::Array(items) = current {
        current = items.first()?;
    }
    Some(current)
}

fn field_text(value: &Value, path: &[&str]) -> Option<String> {
    field(value, path).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

fn field_number(value: &Value, path: &[&str]) -> Option<i64> {
    field(value, path).and_then(|v| v.as_i64().or_else(|| v.as_str()?.trim().parse().ok()))
}

/// Ids we put in a log URL path or query: plain slugs only.
fn slug(text: &str) -> bool {
    !text.is_empty() && text.len() <= 100 && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `HTTP 500 GET /path` without the query string (it can carry tokens).
fn request_line(status: i64, method: Option<String>, path: Option<String>) -> String {
    let path = path.unwrap_or_default();
    let path = path.split(['?', '#']).next().unwrap_or_default();
    format!("HTTP {status} {} {}", method.unwrap_or_else(|| "?".into()), capped(path, 300)).trim_end().to_string()
}

fn runtime_error(service: String, severity: &str, message: String, log_url: Option<String>) -> RuntimeError {
    let message = strip_log_prefix(&message);
    let stack = message.lines().nth(1).map(|_| capped(&message, MESSAGE_MAX));
    RuntimeError {
        service: capped(&service, 100),
        severity: severity.into(),
        title: capped(message.lines().next().unwrap_or_default(), 200),
        message: capped(&message, MESSAGE_MAX),
        stack,
        log_url,
        group_hint: None,
    }
}

/// One Supabase log event. Postgres errors carry `error_severity`; HTTP
/// sources (API gateway, Edge Functions, auth, storage) a 5xx status or an
/// `error` level.
pub fn from_supabase(event: &Value, project_ref: &str) -> Option<RuntimeError> {
    let meta = event.get("metadata").unwrap_or(&Value::Null);
    let pg_severity = field_text(meta, &["parsed", "error_severity"]).map(|s| s.to_ascii_uppercase());
    let level = field_text(meta, &["level"])
        .or_else(|| field_text(event, &["severity_text"]))
        .map(|s| s.to_ascii_lowercase());
    let status = field_number(meta, &["response", "status_code"])
        .or_else(|| field_number(meta, &["status"]))
        .or_else(|| field_number(meta, &["res", "statusCode"]));
    let severity = match (pg_severity.as_deref(), level.as_deref(), status) {
        (Some("PANIC" | "FATAL"), _, _) | (_, Some("fatal" | "panic"), _) => "CRITICAL",
        (Some("ERROR"), _, _) | (_, Some("error"), _) => "ERROR",
        (_, _, Some(s)) if s >= 500 => "ERROR",
        _ => return None,
    };
    let service = match (&pg_severity, field_text(meta, &["function_id"]), field_text(event, &["source"])) {
        (Some(_), _, _) => format!("postgres{}", field_text(meta, &["parsed", "database_name"]).map(|d| format!(":{d}")).unwrap_or_default()),
        (_, Some(function), _) => format!("function:{function}"),
        (_, _, Some(source)) => source.trim_end_matches("_logs").to_string(),
        _ => "supabase".into(),
    };
    let message = field_text(event, &["event_message"])
        .filter(|m| !m.starts_with("HTTP/") || status.is_none())
        .or_else(|| {
            status.map(|s| {
                request_line(s, field_text(meta, &["request", "method"]), field_text(meta, &["request", "path"]))
            })
        })
        .unwrap_or_else(|| "(no message)".into());
    let log_url = slug(project_ref).then(|| format!("https://supabase.com/dashboard/project/{project_ref}/logs/explorer"));
    Some(runtime_error(format!("supabase-{service}"), severity, message, log_url))
}

/// One Vercel log line. Errors are `error`/`fatal` levels, 5xx responses and
/// crashed functions (`statusCode` -1).
pub fn from_vercel(entry: &Value, team: &str) -> Option<RuntimeError> {
    let level = field_text(entry, &["level"]).unwrap_or_default().to_ascii_lowercase();
    let status = field_number(entry, &["statusCode"]).or_else(|| field_number(entry, &["proxy", "statusCode"]));
    let crashed = status == Some(-1);
    let severity = match level.as_str() {
        "fatal" => "CRITICAL",
        "error" => "ERROR",
        _ if crashed => "CRITICAL",
        _ if status.is_some_and(|s| s >= 500) => "ERROR",
        _ => return None,
    };
    let project = field_text(entry, &["projectName"]).unwrap_or_else(|| "vercel".into());
    let message = field_text(entry, &["message"]).unwrap_or_else(|| {
        let method = field_text(entry, &["proxy", "method"]);
        let path = field_text(entry, &["proxy", "path"]).or_else(|| field_text(entry, &["path"]));
        match status {
            Some(-1) => format!("Function crashed {} {}", method.unwrap_or_default(), path.unwrap_or_default().split('?').next().unwrap_or_default()),
            Some(s) => request_line(s, method, path),
            None => "(no message)".into(),
        }
    });
    let request = field_text(entry, &["requestId"]).filter(|id| slug(id));
    let log_url = match request {
        Some(id) if slug(team) && slug(&project) => Some(format!("https://vercel.com/{team}/{project}/logs?requestIds={id}")),
        _ => None,
    };
    Some(runtime_error(format!("vercel-{project}"), severity, message, log_url))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn bodies_are_arrays_objects_ndjson_or_gzip() {
        assert_eq!(events(br#"[{"a":1},{"a":2}]"#, false).unwrap().len(), 2);
        assert_eq!(events(br#"{"a":1}"#, false).unwrap().len(), 1);
        assert_eq!(events(b"{\"a\":1}\n\n{\"a\":2}\n", false).unwrap().len(), 2);
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        std::io::Write::write_all(&mut gz, br#"[{"a":1}]"#).unwrap();
        assert_eq!(events(&gz.finish().unwrap(), true).unwrap().len(), 1);
        assert!(events(b"not json", false).is_err());
        assert!(events(b"42", false).is_err());
    }

    #[test]
    fn supabase_postgres_errors_and_5xx_are_errors_info_is_not() {
        let pg = json!({"event_message": "ERROR:  relation \"orders\" does not exist",
            "metadata": [{"parsed": [{"error_severity": "ERROR", "database_name": "postgres"}]}]});
        let e = from_supabase(&pg, "abcdefghijklmnopqrst").unwrap();
        assert_eq!((e.service.as_str(), e.severity.as_str()), ("supabase-postgres:postgres", "ERROR"));
        assert_eq!(e.title, "ERROR:  relation \"orders\" does not exist");
        assert_eq!(e.log_url.as_deref(), Some("https://supabase.com/dashboard/project/abcdefghijklmnopqrst/logs/explorer"));

        let api = json!({"source": "edge_logs", "event_message": "HTTP/1.1 GET 502",
            "metadata": [{"request": [{"method": "GET", "path": "/rest/v1/orders?apikey=secret"}], "response": [{"status_code": 502}]}]});
        let e = from_supabase(&api, "../evil").unwrap();
        assert_eq!(e.title, "HTTP 502 GET /rest/v1/orders");
        assert_eq!(e.service, "supabase-edge");
        assert!(e.log_url.is_none(), "an unsafe project ref never builds a link");

        let function = json!({"event_message": "boom", "metadata": {"function_id": "send-mail", "level": "error"}});
        assert_eq!(from_supabase(&function, "x").unwrap().service, "supabase-function:send-mail");

        let fatal = json!({"event_message": "FATAL: too many connections", "metadata": [{"parsed": [{"error_severity": "FATAL"}]}]});
        assert_eq!(from_supabase(&fatal, "x").unwrap().severity, "CRITICAL");

        assert!(from_supabase(&json!({"event_message": "ok", "metadata": [{"response": [{"status_code": 200}]}]}), "x").is_none());
        assert!(from_supabase(&json!({"event_message": "LOG: checkpoint", "metadata": [{"parsed": [{"error_severity": "LOG"}]}]}), "x").is_none());
    }

    #[test]
    fn vercel_error_levels_5xx_and_crashes_are_errors() {
        let error = json!({"projectName": "shop", "level": "error", "message": "TypeError: x is undefined\n    at handler (/var/task/api/users.js:3:9)",
            "statusCode": 500, "requestId": "643af4e3-975a-4cc7", "source": "lambda"});
        let e = from_vercel(&error, "acme").unwrap();
        assert_eq!((e.service.as_str(), e.severity.as_str()), ("vercel-shop", "ERROR"));
        assert_eq!(e.title, "TypeError: x is undefined");
        assert!(e.stack.is_some());
        assert_eq!(e.log_url.as_deref(), Some("https://vercel.com/acme/shop/logs?requestIds=643af4e3-975a-4cc7"));

        let crash = json!({"projectName": "shop", "level": "info", "statusCode": -1, "proxy": {"method": "POST", "path": "/api/pay?card=1"}});
        let e = from_vercel(&crash, "acme").unwrap();
        assert_eq!((e.severity.as_str(), e.title.as_str()), ("CRITICAL", "Function crashed POST /api/pay"));

        let gateway = json!({"projectName": "shop", "level": "info", "proxy": {"statusCode": 504, "method": "GET", "path": "/slow?t=1"}});
        assert_eq!(from_vercel(&gateway, "").unwrap().title, "HTTP 504 GET /slow");

        assert!(from_vercel(&json!({"level": "warning", "message": "slow", "statusCode": 200}), "acme").is_none());
        assert!(from_vercel(&json!({"level": "info", "statusCode": 404}), "acme").is_none());
    }

    #[test]
    fn a_storm_is_capped() {
        let batch: Vec<Value> = (0..200).map(|i| json!({"level": "error", "message": format!("e{i}")})).collect();
        assert_eq!(errors("vercel", "acme", &batch).len(), MAX_ERRORS_PER_BATCH);
        assert!(errors("gcp", "p", &batch).is_empty());
    }
}
