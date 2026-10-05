//! OpenTelemetry GenAI spans (factory F3): one `invoke_agent` span per finished
//! autonomous run, so cost, tokens and outcome can be read in any OTLP backend
//! next to the rest of the stack.
//!
//! Inert by default: without `OTEL_EXPORTER_OTLP_ENDPOINT` (or the traces-only
//! `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT`) no exporter, thread or client is built
//! and recording a span is a single `OnceLock` read. When it is set, spans go
//! through a bounded batch queue exported on its own thread over OTLP/HTTP
//! protobuf; a full queue drops spans instead of waiting, so telemetry never
//! delays or fails a run. `OTEL_EXPORTER_OTLP_HEADERS` (e.g. an auth header) is
//! read by the exporter itself; `OTEL_SERVICE_NAME` defaults to
//! `nexusmind-worker`.
//!
//! Only metadata is exported: ids, codes, model names and counts. Prompts,
//! completions, repository names, issue text and errors' free text never reach
//! a span — every string attribute is either a fixed value, an id, or passes a
//! strict code/model filter.

use crate::factory::telemetry::RunMetrics;
use opentelemetry::trace::{Span, SpanKind, Status, Tracer, TracerProvider};
use opentelemetry::KeyValue;
use opentelemetry_sdk::trace::{
    BatchConfigBuilder, BatchSpanProcessor, SdkTracer, SdkTracerProvider,
};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

/// Spans waiting for export. One span per run, so this only fills when the
/// collector is down for a long time; past it, new spans are dropped.
const MAX_QUEUE_SIZE: usize = 512;
/// How often queued spans are sent.
const SCHEDULED_DELAY: Duration = Duration::from_secs(5);
/// Upper bound on one OTLP request, so a hung collector cannot pile up exports.
const EXPORT_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_SERVICE_NAME: &str = "nexusmind-worker";
const SCOPE: &str = "nexusmind.factory";

struct Exporter {
    // Kept so the batch thread lives as long as the process, and flushed on shutdown.
    provider: SdkTracerProvider,
    tracer: SdkTracer,
}

static EXPORTER: OnceLock<Option<Exporter>> = OnceLock::new();

/// What the worker knows about a finished run. Borrowed, so building it costs
/// nothing when export is off.
#[derive(Clone, Copy, Debug)]
pub struct RunSpan<'a> {
    pub run_id: &'a str,
    pub org_id: &'a str,
    pub template_key: &'a str,
    /// `claude`, `nexus` or `codex`.
    pub executor: &'a str,
    /// The router's tier (`cheap`, `standard`, `frontier`), when one was chosen.
    pub tier: Option<&'a str>,
    /// The model the router asked for, when one was chosen.
    pub request_model: Option<&'a str>,
    /// The run's final status (`succeeded`, `failed`, …).
    pub status: &'a str,
    /// The result's `code`, when it has one.
    pub outcome: Option<&'a str>,
    pub metrics: &'a RunMetrics,
    /// When the worker started running it (`started_at`), if known.
    pub started: Option<SystemTime>,
}

/// Sends the spans still queued. Blocks up to `timeout`: call it outside the
/// async runtime's worker threads (e.g. `spawn_blocking`) when the process stops.
pub fn shutdown(timeout: Duration) {
    if let Some(Some(exporter)) = EXPORTER.get() {
        if let Err(error) = exporter.provider.shutdown_with_timeout(timeout) {
            tracing::warn!("OTel shutdown: {error}");
        }
    }
}

/// Builds the exporter once, from the environment. Called when the worker
/// starts; a run finished before that (or with export off) records nothing.
pub fn init_from_env() {
    EXPORTER.get_or_init(|| {
        let env = |name: &str| std::env::var(name).ok();
        let config = ExportConfig::from_vars(env)?;
        match build_exporter(&config, None) {
            Ok(exporter) => {
                tracing::info!(service = %config.service_name, "OTel GenAI run spans enabled");
                Some(exporter)
            }
            Err(error) => {
                tracing::warn!("OTel exporter not started, run spans disabled: {error:#}");
                None
            }
        }
    });
}

/// Whether spans are exported, so callers can skip the lookups a span needs.
pub fn enabled() -> bool {
    matches!(EXPORTER.get(), Some(Some(_)))
}

/// Records one finished run. A no-op unless export is enabled.
pub fn record_run_span(run: &RunSpan<'_>) {
    if let Some(Some(exporter)) = EXPORTER.get() {
        emit_run_span(&exporter.tracer, run, SystemTime::now());
    }
}

/// The settings export needs; `None` means export is off.
#[derive(Debug, PartialEq)]
struct ExportConfig {
    service_name: String,
}

impl ExportConfig {
    /// Reads the standard OTel variables through `var`, so tests need not touch
    /// the process environment. Endpoint and headers themselves are resolved by
    /// the OTLP exporter (`/v1/traces` is appended to the base endpoint).
    fn from_vars(var: impl Fn(&str) -> Option<String>) -> Option<ExportConfig> {
        let set = |name: &str| var(name).filter(|v| !v.trim().is_empty());
        if set("OTEL_SDK_DISABLED").is_some_and(|v| v.trim().eq_ignore_ascii_case("true")) {
            return None;
        }
        set("OTEL_EXPORTER_OTLP_ENDPOINT").or_else(|| set("OTEL_EXPORTER_OTLP_TRACES_ENDPOINT"))?;
        let service_name = set("OTEL_SERVICE_NAME")
            .map(|v| v.trim().to_string())
            .unwrap_or_else(|| DEFAULT_SERVICE_NAME.to_string());
        Some(ExportConfig { service_name })
    }
}

/// `traces_url` overrides the endpoint from the environment (tests only).
fn build_exporter(config: &ExportConfig, traces_url: Option<&str>) -> anyhow::Result<Exporter> {
    use opentelemetry_otlp::WithExportConfig;
    let mut builder = opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
        .with_timeout(EXPORT_TIMEOUT);
    if let Some(url) = traces_url {
        builder = builder.with_endpoint(url);
    }
    let exporter = builder.build()?;
    let processor = BatchSpanProcessor::builder(exporter)
        .with_batch_config(
            BatchConfigBuilder::default()
                .with_max_queue_size(MAX_QUEUE_SIZE)
                .with_scheduled_delay(SCHEDULED_DELAY)
                .build(),
        )
        .build();
    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name(config.service_name.clone())
        .build();
    let provider = SdkTracerProvider::builder()
        .with_span_processor(processor)
        .with_resource(resource)
        .build();
    let tracer = provider.tracer(SCOPE);
    Ok(Exporter { provider, tracer })
}

/// Emits the run's span on `tracer`, ending at `end`. It starts when the
/// worker started the run; without that, `duration_ms` earlier (the agent's own
/// time), else it is zero-length at `end`. `duration_ms` is not wall time for a
/// fan-out run (its sessions run in parallel and are summed), so it is also
/// sent as its own attribute rather than trusted as the span's length.
pub(crate) fn emit_run_span<T: Tracer>(tracer: &T, run: &RunSpan<'_>, end: SystemTime) {
    let start = run
        .started
        .filter(|started| *started <= end)
        .or_else(|| {
            run.metrics
                .duration_ms
                .and_then(|ms| end.checked_sub(Duration::from_millis(ms.max(0) as u64)))
        })
        .unwrap_or(end);
    let mut span = tracer
        .span_builder(span_name(run))
        .with_kind(SpanKind::Internal)
        .with_start_time(start)
        .with_attributes(span_attributes(run))
        .start(tracer);
    if !matches!(run.status, "succeeded" | "partial") {
        // Only the sanitized code: an error's free text may quote the repo.
        let code = run
            .outcome
            .and_then(safe_code)
            .or_else(|| safe_code(run.status))
            .unwrap_or("other");
        span.set_attribute(KeyValue::new("error.type", code.to_string()));
        span.set_status(Status::error(code.to_string()));
    }
    span.end_with_timestamp(end);
}

/// `invoke_agent {gen_ai.agent.name}`, as the GenAI conventions name it.
fn span_name(run: &RunSpan<'_>) -> String {
    format!(
        "invoke_agent {}",
        safe_code(run.template_key).unwrap_or("agent")
    )
}

/// Which GenAI provider an executor calls. Nexus runs on Claude.
fn provider_name(executor: &str) -> &'static str {
    match executor {
        "codex" => "openai",
        "claude" | "nexus" => "anthropic",
        _ => "unknown",
    }
}

/// The span's attributes (OTel GenAI semantic conventions plus `nexusmind.*`).
/// An unknown value is left out, never reported as 0.
pub(crate) fn span_attributes(run: &RunSpan<'_>) -> Vec<KeyValue> {
    let metrics = run.metrics;
    let provider = provider_name(run.executor);
    let mut attributes = vec![
        KeyValue::new("gen_ai.operation.name", "invoke_agent"),
        // `gen_ai.system` is the pre-1.37 name; both are sent so older and
        // newer backends group runs by provider.
        KeyValue::new("gen_ai.provider.name", provider),
        KeyValue::new("gen_ai.system", provider),
        KeyValue::new("nexusmind.run.id", run.run_id.to_string()),
        KeyValue::new("nexusmind.org.id", run.org_id.to_string()),
        KeyValue::new(
            "nexusmind.executor",
            safe_code(run.executor).unwrap_or("other").to_string(),
        ),
        KeyValue::new(
            "nexusmind.run.status",
            safe_code(run.status).unwrap_or("other").to_string(),
        ),
    ];
    let mut push = |key: &'static str, value: Option<opentelemetry::Value>| {
        if let Some(value) = value {
            attributes.push(KeyValue::new(key, value));
        }
    };
    let text = |value: Option<&str>| value.map(|v| opentelemetry::Value::from(v.to_string()));
    let int = |value: Option<i64>| value.map(opentelemetry::Value::from);
    push("gen_ai.agent.name", text(safe_code(run.template_key)));
    push("nexusmind.template_key", text(safe_code(run.template_key)));
    push("nexusmind.tier", text(run.tier.and_then(safe_code)));
    push(
        "nexusmind.run.outcome",
        text(run.outcome.and_then(safe_code)),
    );
    let response_model = metrics.model.as_deref().and_then(safe_model);
    push(
        "gen_ai.request.model",
        text(run.request_model.and_then(safe_model).or(response_model)),
    );
    push("gen_ai.response.model", text(response_model));
    // The conventions count cached tokens inside `input_tokens`; run metrics
    // (Claude's convention, which Codex results follow) keep them apart. The
    // total is only reported when every part of it is known.
    let input_total = match (
        metrics.input_tokens,
        metrics.cached_input_tokens,
        metrics.cache_write_tokens,
    ) {
        (Some(input), Some(read), Some(write)) => Some(input + read + write),
        _ => None,
    };
    push("gen_ai.usage.input_tokens", int(input_total));
    push("gen_ai.usage.output_tokens", int(metrics.output_tokens));
    push(
        "gen_ai.usage.cache_read.input_tokens",
        int(metrics.cached_input_tokens),
    );
    push(
        "gen_ai.usage.cache_creation.input_tokens",
        int(metrics.cache_write_tokens),
    );
    push("nexusmind.num_turns", int(metrics.num_turns));
    push("nexusmind.agent_duration_ms", int(metrics.duration_ms));
    push(
        "nexusmind.cost_usd",
        metrics.cost_usd.map(opentelemetry::Value::from),
    );
    attributes
}

/// A status, outcome, tier or template code: one snake_case word. Anything
/// else (an error message, a host, a path, a `code:detail`) is refused, not
/// truncated: some result codes are raw error strings.
fn safe_code(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= 64
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_'))
    .then_some(value)
}

/// A model id or alias (`claude-sonnet-5`, `opus[1m]`, `gpt-6.1-sol`).
fn safe_model(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value.len() <= 96
        && value.bytes().all(|b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'_' | b'-' | b'.' | b':' | b'/' | b'@' | b'[' | b']')
        }))
    .then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry::Value;
    use opentelemetry_sdk::error::OTelSdkResult;
    use opentelemetry_sdk::trace::{SpanData, SpanExporter};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Default)]
    struct Capture(Arc<Mutex<Vec<SpanData>>>);

    impl SpanExporter for Capture {
        async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
            self.0.lock().unwrap().extend(batch);
            Ok(())
        }
    }

    fn metrics() -> RunMetrics {
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
    }

    fn run<'a>(metrics: &'a RunMetrics) -> RunSpan<'a> {
        RunSpan {
            run_id: "run-1",
            org_id: "org-1",
            template_key: "github_issue_resolver",
            executor: "claude",
            tier: Some("standard"),
            request_model: Some("sonnet"),
            status: "succeeded",
            outcome: Some("completed"),
            metrics,
            started: None,
        }
    }

    fn map(attributes: &[KeyValue]) -> HashMap<String, Value> {
        attributes
            .iter()
            .map(|kv| (kv.key.to_string(), kv.value.clone()))
            .collect()
    }

    fn capture(run: &RunSpan<'_>) -> SpanData {
        capture_at(run, SystemTime::now())
    }

    fn capture_at(run: &RunSpan<'_>, end: SystemTime) -> SpanData {
        let capture = Capture::default();
        let provider = SdkTracerProvider::builder()
            .with_simple_exporter(capture.clone())
            .build();
        emit_run_span(&provider.tracer(SCOPE), run, end);
        let mut spans = capture.0.lock().unwrap().clone();
        assert_eq!(spans.len(), 1);
        spans.remove(0)
    }

    #[test]
    fn maps_a_claude_run_to_genai_attributes() {
        let metrics = metrics();
        let attributes = map(&span_attributes(&run(&metrics)));
        let expect =
            |key: &str, value: Value| assert_eq!(attributes.get(key), Some(&value), "{key}");
        expect("gen_ai.operation.name", "invoke_agent".into());
        expect("gen_ai.provider.name", "anthropic".into());
        expect("gen_ai.system", "anthropic".into());
        expect("gen_ai.agent.name", "github_issue_resolver".into());
        expect("gen_ai.request.model", "sonnet".into());
        expect("gen_ai.response.model", "claude-sonnet-5".into());
        expect(
            "gen_ai.usage.input_tokens",
            (1200i64 + 90000 + 15000).into(),
        );
        expect("gen_ai.usage.output_tokens", 3400i64.into());
        expect("gen_ai.usage.cache_read.input_tokens", 90000i64.into());
        expect("gen_ai.usage.cache_creation.input_tokens", 15000i64.into());
        expect("nexusmind.run.id", "run-1".into());
        expect("nexusmind.org.id", "org-1".into());
        expect("nexusmind.template_key", "github_issue_resolver".into());
        expect("nexusmind.tier", "standard".into());
        expect("nexusmind.cost_usd", 0.4213.into());
        expect("nexusmind.run.status", "succeeded".into());
        expect("nexusmind.run.outcome", "completed".into());
        expect("nexusmind.executor", "claude".into());
    }

    #[test]
    fn codex_is_openai_and_nexus_is_anthropic() {
        let metrics = metrics();
        for (executor, provider) in [
            ("codex", "openai"),
            ("nexus", "anthropic"),
            ("claude", "anthropic"),
        ] {
            let attributes = map(&span_attributes(&RunSpan {
                executor,
                ..run(&metrics)
            }));
            assert_eq!(
                attributes["gen_ai.provider.name"],
                Value::from(provider),
                "{executor}"
            );
        }
    }

    #[test]
    fn unknown_values_are_left_out_not_zero() {
        let metrics = RunMetrics {
            input_tokens: Some(10),
            ..RunMetrics::default()
        };
        let attributes = map(&span_attributes(&RunSpan {
            tier: None,
            request_model: None,
            outcome: None,
            ..run(&metrics)
        }));
        for key in [
            "gen_ai.usage.input_tokens", // cached part unknown, so the total is
            "gen_ai.usage.output_tokens",
            "gen_ai.request.model",
            "gen_ai.response.model",
            "nexusmind.cost_usd",
            "nexusmind.tier",
            "nexusmind.run.outcome",
        ] {
            assert!(!attributes.contains_key(key), "{key} should be absent");
        }
    }

    #[test]
    fn the_request_model_falls_back_to_the_model_used() {
        let metrics = metrics();
        let attributes = map(&span_attributes(&RunSpan {
            request_model: None,
            ..run(&metrics)
        }));
        assert_eq!(
            attributes["gen_ai.request.model"],
            Value::from("claude-sonnet-5")
        );
    }

    #[test]
    fn export_is_off_unless_an_endpoint_is_set() {
        let vars = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == name)
                    .map(|(_, v)| v.to_string())
            }
        };
        assert_eq!(ExportConfig::from_vars(vars(&[])), None);
        assert_eq!(
            ExportConfig::from_vars(vars(&[("OTEL_EXPORTER_OTLP_ENDPOINT", "  ")])),
            None
        );
        assert_eq!(
            ExportConfig::from_vars(vars(&[("OTEL_SERVICE_NAME", "x")])),
            None
        );
        assert_eq!(
            ExportConfig::from_vars(vars(&[
                ("OTEL_EXPORTER_OTLP_ENDPOINT", "http://collector:4318"),
                ("OTEL_SDK_DISABLED", "true"),
            ])),
            None
        );
        assert_eq!(
            ExportConfig::from_vars(vars(&[(
                "OTEL_EXPORTER_OTLP_ENDPOINT",
                "http://collector:4318"
            )])),
            Some(ExportConfig {
                service_name: "nexusmind-worker".into()
            })
        );
        assert_eq!(
            ExportConfig::from_vars(vars(&[
                (
                    "OTEL_EXPORTER_OTLP_TRACES_ENDPOINT",
                    "http://collector:4318/v1/traces"
                ),
                ("OTEL_SERVICE_NAME", "factory"),
            ])),
            Some(ExportConfig {
                service_name: "factory".into()
            })
        );
    }

    #[test]
    fn recording_without_an_exporter_is_a_noop() {
        // The test process never calls `init_from_env`, so nothing is built.
        assert!(!enabled());
        let metrics = metrics();
        record_run_span(&run(&metrics));
        assert!(EXPORTER.get().is_none());
    }

    #[test]
    fn a_finished_run_becomes_one_timed_span() {
        let metrics = metrics();
        let span = capture(&run(&metrics));
        assert_eq!(span.name, "invoke_agent github_issue_resolver");
        assert_eq!(span.span_kind, SpanKind::Internal);
        assert_eq!(span.status, Status::Unset);
        assert_eq!(
            span.end_time.duration_since(span.start_time).unwrap(),
            Duration::from_millis(48210)
        );
        assert!(map(&span.attributes).contains_key("gen_ai.usage.output_tokens"));
    }

    /// The real exporter, built inside the async runtime like the worker does,
    /// posts OTLP/HTTP protobuf to `/v1/traces` without blocking the caller.
    #[tokio::test]
    async fn the_otlp_exporter_posts_protobuf_to_v1_traces() {
        use std::io::{BufRead, BufReader, Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = Vec::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line.trim().is_empty() {
                    break;
                }
                head.push(line.trim().to_ascii_lowercase());
            }
            let length: usize = head
                .iter()
                .find_map(|h| h.strip_prefix("content-length:"))
                .map_or(0, |v| v.trim().parse().unwrap());
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut stream = stream;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n")
                .unwrap();
            (head, body)
        });
        let config = ExportConfig {
            service_name: "nexusmind-worker".into(),
        };
        let exporter =
            build_exporter(&config, Some(&format!("http://{address}/v1/traces"))).unwrap();
        let metrics = metrics();
        emit_run_span(&exporter.tracer, &run(&metrics), SystemTime::now());
        exporter.provider.force_flush().unwrap();
        let (head, body) = server.join().unwrap();
        assert_eq!(head[0], "post /v1/traces http/1.1");
        assert!(
            head.contains(&"content-type: application/x-protobuf".to_string()),
            "{head:?}"
        );
        let body = String::from_utf8_lossy(&body);
        for expected in [
            "nexusmind-worker",
            "invoke_agent github_issue_resolver",
            "gen_ai.usage.output_tokens",
        ] {
            assert!(
                body.contains(expected),
                "{expected} missing from the export"
            );
        }
    }

    #[test]
    fn a_failed_run_carries_only_its_code() {
        let metrics = RunMetrics::default();
        let span = capture(&RunSpan {
            status: "failed",
            outcome: Some("claude_timeout"),
            ..run(&metrics)
        });
        assert_eq!(span.status, Status::error("claude_timeout"));
        assert_eq!(
            map(&span.attributes)["error.type"],
            Value::from("claude_timeout")
        );
    }

    #[test]
    fn free_text_and_secrets_never_reach_a_span() {
        // A hostile result: a code that is really an error message, a model
        // name carrying a token, a template key with a path.
        let secret = "ghp_SECRETTOKEN1234";
        let metrics = RunMetrics {
            model: Some(format!("model with spaces {secret}")),
            ..RunMetrics::default()
        };
        let request_model = format!("sonnet; Authorization: Bearer {secret}");
        let outcome = format!(
            "fatal: could not read Username for 'https://github.com/acme/private': {secret}"
        );
        let run = RunSpan {
            template_key: "../../etc/passwd",
            tier: Some("Frontier Tier!"),
            request_model: Some(&request_model),
            status: "failed",
            outcome: Some(&outcome),
            ..run(&metrics)
        };
        let span = capture(&run);
        let dump = format!("{:?} {:?} {:?}", span.name, span.attributes, span.status);
        for leak in [
            secret,
            "github.com",
            "passwd",
            "Authorization",
            "fatal",
            "Frontier Tier",
        ] {
            assert!(!dump.contains(leak), "{leak} leaked: {dump}");
        }
        let attributes = map(&span.attributes);
        // The unsafe outcome is dropped; the run's status still says what happened.
        assert_eq!(attributes["error.type"], Value::from("failed"));
        assert!(!attributes.contains_key("gen_ai.request.model"));
        assert!(!attributes.contains_key("gen_ai.response.model"));
        assert!(!attributes.contains_key("nexusmind.template_key"));
        assert_eq!(span.name, "invoke_agent agent");
    }

    #[test]
    fn only_metadata_keys_are_exported() {
        // A new attribute must be added here on purpose, after checking it
        // cannot carry prompt, completion, repository or secret text.
        let metrics = metrics();
        let mut keys: Vec<String> = span_attributes(&run(&metrics))
            .iter()
            .map(|kv| kv.key.to_string())
            .collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "gen_ai.agent.name",
                "gen_ai.operation.name",
                "gen_ai.provider.name",
                "gen_ai.request.model",
                "gen_ai.response.model",
                "gen_ai.system",
                "gen_ai.usage.cache_creation.input_tokens",
                "gen_ai.usage.cache_read.input_tokens",
                "gen_ai.usage.input_tokens",
                "gen_ai.usage.output_tokens",
                "nexusmind.agent_duration_ms",
                "nexusmind.cost_usd",
                "nexusmind.executor",
                "nexusmind.num_turns",
                "nexusmind.org.id",
                "nexusmind.run.id",
                "nexusmind.run.outcome",
                "nexusmind.run.status",
                "nexusmind.template_key",
                "nexusmind.tier",
            ]
        );
    }

    #[test]
    fn the_span_starts_when_the_worker_started_the_run() {
        let metrics = RunMetrics { duration_ms: Some(50 * 60 * 1000), ..RunMetrics::default() };
        let end = SystemTime::UNIX_EPOCH + Duration::from_secs(10_000);
        let started = end - Duration::from_secs(600);
        // A fan-out run's summed duration (50 min) is not its wall time (10 min).
        let span = capture_at(&RunSpan { started: Some(started), ..run(&metrics) }, end);
        assert_eq!(span.start_time, started);
        assert_eq!(map(&span.attributes)["nexusmind.agent_duration_ms"], Value::from(50 * 60 * 1000_i64));
        // A start after the end (clock skew) is ignored.
        let skewed = capture_at(&RunSpan { started: Some(end + Duration::from_secs(5)), ..run(&metrics) }, end);
        assert_eq!(skewed.start_time, end - Duration::from_secs(50 * 60));
    }

    #[test]
    fn codes_are_single_snake_case_words() {
        assert_eq!(safe_code("cost_limit_exceeded"), Some("cost_limit_exceeded"));
        for unsafe_code in ["proxy.acme-internal.corp:8443", "class_not_auto_startable:security", "a b", "Upper", ""] {
            assert_eq!(safe_code(unsafe_code), None, "{unsafe_code}");
        }
    }
}
