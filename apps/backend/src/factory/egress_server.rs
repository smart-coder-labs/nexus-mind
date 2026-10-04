//! Network side of the sandbox egress proxy (factory F1, design §1). Every
//! decision comes from [`crate::factory::egress::decide`]; this module only moves
//! bytes: it streams reverse-route responses and splices allow-listed tunnels.
//!
//! The proxy is the only pod in the sandbox namespace with internet egress, and
//! the only place the real Claude and NexusMind credentials exist.

use std::{convert::Infallible, sync::Arc, time::Duration};

use tokio::sync::Semaphore;

use bytes::Bytes;
use futures_util::{StreamExt, TryStreamExt};
use http_body_util::{combinators::BoxBody, BodyExt, Full, StreamBody};
use hyper::{
    body::{Frame, Incoming},
    header::{HeaderName, HeaderValue, PROXY_AUTHORIZATION},
    server::conn::http1,
    service::service_fn,
    Request, Response, StatusCode,
};
use hyper_util::rt::TokioIo;

use super::egress::{
    codex_tier, decide, openai_request_body, scrub_request_headers, Decision, OpenaiRequestScope, Upstream,
    MAX_OPENAI_REQUESTS_PER_RUN, MAX_OPENAI_REQUEST_BYTES,
};

/// How long a Codex request body may take to arrive (it is buffered to be checked).
const OPENAI_BODY_TIMEOUT: Duration = Duration::from_secs(30);
/// Codex request bodies one run may have in the buffer at once.
const OPENAI_BUFFERS_PER_RUN: u32 = 2;

/// Codex requests are buffered to be checked; this bounds the memory they take
/// together, so one run cannot exhaust the proxy every org shares.
static OPENAI_BUFFERS: Semaphore = Semaphore::const_new(8);

/// Largest request body forwarded upstream. Model requests are JSON well below this.
const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;

/// How the proxy authenticates to Anthropic (spike S1: subscription OAuth works).
pub enum AnthropicAuth {
    OAuth(String),
    ApiKey(String),
}

pub struct EgressConfig {
    pub signing_key: Vec<u8>,
    pub anthropic: Option<AnthropicAuth>,
    /// OpenAI API key for the Codex CLI; `None` fails the OpenAI route closed.
    pub openai_api_key: Option<String>,
    /// The Codex model of each tier (cheap, standard, frontier). A Codex token
    /// carries its tier, so a run can only use the model the worker chose.
    pub openai_models: [String; 3],
    /// NexusMind bot API key per organization (`org_id → key`). A run whose org
    /// has no key fails closed.
    pub nexusmind_keys: std::collections::HashMap<String, String>,
    pub tunnel_allowlist: Vec<String>,
    /// Tests only: send every reverse route to this base URL instead of the real host.
    pub upstream_override: Option<String>,
    /// Reverse requests served at once; beyond this they are shed with 503 so one
    /// sandbox cannot exhaust the proxy's memory or sockets.
    pub max_in_flight: usize,
    /// Read-only GitHub Packages access per organization, for private npm
    /// dependencies. A run whose org has none fails closed.
    pub github_packages_tokens: std::collections::HashMap<String, GithubPackagesAuth>,
    /// Tests only: let tunnels reach private addresses. The binary never sets it.
    pub allow_private_upstreams: bool,
    /// OpenAI requests forwarded per run (Codex spend bound), in this proxy's memory.
    pub openai_requests: std::sync::Mutex<std::collections::HashMap<String, u32>>,
    /// Codex bodies being buffered per run, so one run cannot hold every buffer.
    pub openai_buffering: std::sync::Mutex<std::collections::HashMap<String, u32>>,
}

/// Whether `run_id` still has OpenAI requests left (counted when forwarded).
fn openai_requests_left(config: &EgressConfig, run_id: &str) -> bool {
    config
        .openai_requests
        .lock()
        .is_ok_and(|counts| counts.get(run_id).copied().unwrap_or(0) < MAX_OPENAI_REQUESTS_PER_RUN)
}

/// Counts one forwarded OpenAI request for `run_id`.
fn count_openai_request(config: &EgressConfig, run_id: &str) {
    if let Ok(mut counts) = config.openai_requests.lock() {
        // Entries outlive their (short-lived) runs: bound the map, not the runs.
        if counts.len() > 10_000 && !counts.contains_key(run_id) {
            counts.clear();
        }
        *counts.entry(run_id.to_string()).or_insert(0) += 1;
    }
}

/// One of a run's Codex body buffers, released when dropped.
struct RunBuffer<'a> {
    config: &'a EgressConfig,
    run_id: String,
}

impl<'a> RunBuffer<'a> {
    fn take(config: &'a EgressConfig, run_id: &str) -> Option<Self> {
        let mut buffering = config.openai_buffering.lock().ok()?;
        let count = buffering.entry(run_id.to_string()).or_insert(0);
        if *count >= OPENAI_BUFFERS_PER_RUN {
            return None;
        }
        *count += 1;
        Some(RunBuffer { config, run_id: run_id.to_string() })
    }
}

impl Drop for RunBuffer<'_> {
    fn drop(&mut self) {
        if let Ok(mut buffering) = self.config.openai_buffering.lock() {
            if let Some(count) = buffering.get_mut(&self.run_id) {
                *count -= 1;
                if *count == 0 {
                    buffering.remove(&self.run_id);
                }
            }
        }
    }
}

/// A classic PAT (GitHub Packages for npm accepts no other kind) can read every
/// package its owner can, so the proxy serves only the org's own scopes with it.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct GithubPackagesAuth {
    pub token: String,
    /// Allowed npm scopes, e.g. `@kasymir` (compared lowercase).
    pub scopes: Vec<String>,
}

/// Upper bound on a tunnel's lifetime (package downloads, not long-lived streams).
const TUNNEL_MAX: Duration = Duration::from_secs(30 * 60);
/// Upper bound on connecting to an upstream.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

type Body = BoxBody<Bytes, std::io::Error>;

fn text(status: StatusCode, body: &'static str) -> Response<Body> {
    let mut response = Response::new(
        Full::new(Bytes::from_static(body.as_bytes()))
            .map_err(|never| match never {})
            .boxed(),
    );
    *response.status_mut() = status;
    response
}

/// Accepts connections forever. Each connection is served on its own task.
pub async fn serve(
    listener: tokio::net::TcpListener,
    config: Arc<EgressConfig>,
) -> std::io::Result<()> {
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(CONNECT_TIMEOUT)
        // Model responses stream for minutes; bound each read, not the whole body.
        .read_timeout(Duration::from_secs(300))
        .build()
        .map_err(std::io::Error::other)?;
    let slots = Arc::new(Semaphore::new(config.max_in_flight.max(1)));
    loop {
        // A failed accept (e.g. descriptors exhausted) must not stop egress for
        // every run: log, back off, keep serving.
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(error) => {
                tracing::error!(target: "factory_egress", "accept failed: {error}");
                tokio::time::sleep(Duration::from_millis(250)).await;
                continue;
            }
        };
        let config = config.clone();
        let client = client.clone();
        let slots = slots.clone();
        tokio::spawn(async move {
            let service = service_fn(move |request| {
                handle(request, config.clone(), client.clone(), slots.clone())
            });
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades()
                .await;
        });
    }
}

async fn handle(
    request: Request<Incoming>,
    config: Arc<EgressConfig>,
    client: reqwest::Client,
    slots: Arc<Semaphore>,
) -> Result<Response<Body>, Infallible> {
    let target = request.uri().to_string();
    let proxy_authorization = request
        .headers()
        .get(PROXY_AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let allowlist: Vec<&str> = config.tunnel_allowlist.iter().map(String::as_str).collect();
    let decision = decide(
        &config.signing_key,
        request.method().as_str(),
        &target,
        proxy_authorization.as_deref(),
        &allowlist,
        chrono::Utc::now().timestamp(),
    );
    match decision {
        Decision::Deny { reason, run_id } => {
            tracing::warn!(target: "factory_egress", run_id = run_id.as_deref().unwrap_or("-"), reason, method = %request.method(), "denied");
            if reason == "no_run_token" && request.method() == hyper::Method::CONNECT {
                // Challenge, so clients that only send credentials after a 407 retry.
                let mut response = text(
                    StatusCode::PROXY_AUTHENTICATION_REQUIRED,
                    "proxy credentials required\n",
                );
                response.headers_mut().insert(
                    hyper::header::PROXY_AUTHENTICATE,
                    HeaderValue::from_static("Basic realm=\"factory\""),
                );
                return Ok(response);
            }
            Ok(text(StatusCode::FORBIDDEN, "egress denied\n"))
        }
        Decision::Tunnel { run, host, port } => {
            // Resolve, keep only public addresses and connect to one of those: the
            // name is judged by where it points, and the checked address is the
            // one used, so DNS rebinding cannot swap it afterwards.
            let resolved: Vec<std::net::SocketAddr> = match tokio::time::timeout(
                CONNECT_TIMEOUT,
                tokio::net::lookup_host((host.as_str(), port)),
            )
            .await
            {
                Ok(Ok(addresses)) => addresses.collect(),
                _ => Vec::new(),
            };
            let allowed: Vec<std::net::SocketAddr> = resolved
                .iter()
                .copied()
                .filter(|address| {
                    config.allow_private_upstreams || super::egress::is_public_ip(address.ip())
                })
                .collect();
            if allowed.is_empty() {
                let reason = if resolved.is_empty() {
                    "unresolvable"
                } else {
                    "private_address"
                };
                tracing::warn!(target: "factory_egress", run_id = %run.run_id, %host, reason, "tunnel denied");
                return Ok(text(StatusCode::FORBIDDEN, "egress denied\n"));
            }
            // Connect first: only a working tunnel is answered with 200.
            let server = match tokio::time::timeout(
                CONNECT_TIMEOUT,
                tokio::net::TcpStream::connect(allowed.as_slice()),
            )
            .await
            {
                Ok(Ok(server)) => server,
                _ => {
                    tracing::warn!(target: "factory_egress", run_id = %run.run_id, %host, "tunnel upstream unreachable");
                    return Ok(text(StatusCode::BAD_GATEWAY, "upstream unreachable\n"));
                }
            };
            tracing::info!(target: "factory_egress", run_id = %run.run_id, %host, "tunnel");
            tokio::spawn(async move {
                let Ok(upgraded) = hyper::upgrade::on(request).await else {
                    return;
                };
                let mut server = server;
                let mut client_side = TokioIo::new(upgraded);
                let _ = tokio::time::timeout(
                    TUNNEL_MAX,
                    tokio::io::copy_bidirectional(&mut client_side, &mut server),
                )
                .await;
            });
            Ok(Response::new(
                Full::new(Bytes::new())
                    .map_err(|never| match never {})
                    .boxed(),
            ))
        }
        Decision::Reverse {
            run,
            upstream,
            path_and_query,
        } => {
            // Shed load instead of queueing: a saturated proxy answers fast.
            let Ok(slot) = slots.try_acquire_owned() else {
                tracing::warn!(target: "factory_egress", run_id = %run.run_id, "shed: too many requests in flight");
                return Ok(text(StatusCode::SERVICE_UNAVAILABLE, "egress busy\n"));
            };
            Ok(reverse(
                request,
                &config,
                &client,
                &run,
                upstream,
                &path_and_query,
                slot,
            )
            .await)
        }
    }
}

async fn reverse(
    request: Request<Incoming>,
    config: &EgressConfig,
    client: &reqwest::Client,
    run: &super::egress::RunToken,
    upstream: Upstream,
    path_and_query: &str,
    // Held until the response body finishes or is dropped, so long streams count.
    slot: tokio::sync::OwnedSemaphorePermit,
) -> Response<Body> {
    let run_id = run.run_id.as_str();
    // Credential first: an upstream without one fails closed, before any byte is sent.
    let mut injected: Vec<(String, String)> = Vec::new();
    match (
        upstream,
        &config.anthropic,
        config.nexusmind_keys.get(&run.org_id),
    ) {
        (Upstream::Anthropic, Some(AnthropicAuth::OAuth(token)), _) => {
            injected.push(("authorization".into(), format!("Bearer {token}")));
        }
        (Upstream::Anthropic, Some(AnthropicAuth::ApiKey(key)), _) => {
            injected.push(("x-api-key".into(), key.clone()));
        }
        (Upstream::Nexusmind, _, Some(token)) => {
            injected.push(("authorization".into(), format!("Bearer {token}")));
        }
        (Upstream::Openai, _, _) if config.openai_api_key.is_some() => {
            let key = config.openai_api_key.as_deref().unwrap_or_default();
            injected.push(("authorization".into(), format!("Bearer {key}")));
        }
        (Upstream::GithubPackages, _, _) => match config.github_packages_tokens.get(&run.org_id) {
            Some(auth) => {
                let allowed =
                    super::egress::github_packages_scope(path_and_query).is_some_and(|scope| {
                        auth.scopes
                            .iter()
                            .any(|allowed| allowed.eq_ignore_ascii_case(&scope))
                    });
                if !allowed {
                    tracing::warn!(target: "factory_egress", run_id, ?upstream, "github packages scope not allowed");
                    return text(StatusCode::FORBIDDEN, "egress denied\n");
                }
                injected.push(("authorization".into(), format!("Bearer {}", auth.token)));
            }
            None => {
                tracing::warn!(target: "factory_egress", run_id, ?upstream, "upstream not configured");
                return text(StatusCode::SERVICE_UNAVAILABLE, "upstream not configured\n");
            }
        },
        _ => {
            tracing::warn!(target: "factory_egress", run_id, ?upstream, "upstream not configured");
            return text(StatusCode::SERVICE_UNAVAILABLE, "upstream not configured\n");
        }
    }
    let base = config
        .upstream_override
        .clone()
        .unwrap_or_else(|| format!("https://{}", upstream.host()));
    let method = request.method().clone();
    let headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.to_string(), v.to_string()))
        })
        .collect();
    // A declared length over the cap is refused up front; otherwise it is forwarded
    // so the upstream receives a plain (non-chunked) body, as the client sent it.
    let declared = headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
        .map(|(_, value)| value.parse::<usize>());
    match declared {
        Some(Ok(length)) if length > MAX_REQUEST_BYTES => {
            return text(StatusCode::PAYLOAD_TOO_LARGE, "request too large\n")
        }
        Some(Err(_)) => return text(StatusCode::BAD_REQUEST, "bad content-length\n"),
        _ => {}
    }
    let mut headers = scrub_request_headers(&headers);
    if matches!(
        (upstream, &config.anthropic),
        (Upstream::Anthropic, Some(AnthropicAuth::OAuth(_)))
    ) {
        // Subscription OAuth requires this beta flag; keep the client's own flags.
        const OAUTH_BETA: &str = "oauth-2025-04-20";
        match headers
            .iter_mut()
            .find(|(name, _)| name.eq_ignore_ascii_case("anthropic-beta"))
        {
            Some((_, value)) if !value.split(',').any(|flag| flag.trim() == OAUTH_BETA) => {
                value.push(',');
                value.push_str(OAUTH_BETA);
            }
            Some(_) => {}
            None => headers.push(("anthropic-beta".into(), OAUTH_BETA.into())),
        }
    }
    headers.extend(injected);

    let body = if upstream == Upstream::Openai {
        // Codex requests are read whole and checked (see `openai_request_body`),
        // at most MAX_OPENAI_REQUESTS_PER_RUN forwarded per run.
        let Some(tier) = codex_tier(run_id) else {
            return text(StatusCode::FORBIDDEN, "egress denied\n");
        };
        if !openai_requests_left(config, run_id) {
            tracing::warn!(target: "factory_egress", run_id, ?upstream, "openai request cap reached");
            return text(StatusCode::TOO_MANY_REQUESTS, "egress request cap reached\n");
        }
        // The body is checked as plain JSON; an encoded one cannot be.
        if headers.iter().any(|(name, _)| name.eq_ignore_ascii_case("content-encoding")) {
            return text(StatusCode::FORBIDDEN, "egress denied: encoded body\n");
        }
        let Some(_run_buffer) = RunBuffer::take(config, run_id) else {
            return text(StatusCode::SERVICE_UNAVAILABLE, "egress busy for this run\n");
        };
        let Ok(_buffer) = OPENAI_BUFFERS.try_acquire() else {
            return text(StatusCode::SERVICE_UNAVAILABLE, "egress busy\n");
        };
        let limited = http_body_util::Limited::new(request.into_body(), MAX_OPENAI_REQUEST_BYTES).collect();
        let collected = match tokio::time::timeout(OPENAI_BODY_TIMEOUT, limited).await {
            Ok(Ok(collected)) => collected.to_bytes(),
            Ok(Err(_)) => return text(StatusCode::PAYLOAD_TOO_LARGE, "request too large\n"),
            Err(_) => return text(StatusCode::REQUEST_TIMEOUT, "request body too slow\n"),
        };
        let scope = OpenaiRequestScope {
            model: &config.openai_models[tier],
            org_id: &run.org_id,
            generation: path_and_query.split('?').next() == Some("/v1/responses"),
        };
        match openai_request_body(&collected, &scope) {
            Ok(filtered) => {
                headers.retain(|(name, _)| !name.eq_ignore_ascii_case("content-length"));
                count_openai_request(config, run_id);
                reqwest::Body::from(filtered)
            }
            Err(reason) => {
                tracing::warn!(target: "factory_egress", run_id, ?upstream, reason, "openai body refused");
                return text(
                    StatusCode::FORBIDDEN,
                    if reason == "model_not_allowed" { "egress denied: model not allowed\n" } else { "egress denied\n" },
                );
            }
        }
    } else {
        // Streamed, never buffered: 64 in-flight requests must fit the proxy's memory.
        let mut forwarded = 0usize;
        reqwest::Body::wrap_stream(request.into_body().into_data_stream().map(move |chunk| {
            let chunk = chunk.map_err(std::io::Error::other)?;
            forwarded += chunk.len();
            if forwarded > MAX_REQUEST_BYTES {
                return Err(std::io::Error::other("request too large"));
            }
            Ok::<_, std::io::Error>(chunk)
        }))
    };
    let mut outbound = client.request(method.clone(), format!("{base}{path_and_query}"));
    for (name, value) in &headers {
        outbound = outbound.header(name, value);
    }
    let upstream_response = match outbound.body(body).send().await {
        Ok(response) => response,
        Err(error) => {
            tracing::warn!(target: "factory_egress", run_id, ?upstream, "upstream error: {error}");
            return text(StatusCode::BAD_GATEWAY, "upstream unreachable\n");
        }
    };
    let status = upstream_response.status();
    tracing::info!(target: "factory_egress", run_id, ?upstream, %method, path = path_and_query.split('?').next().unwrap_or(""), status = status.as_u16(), "reverse");

    let mut response = Response::builder().status(status.as_u16());
    for (name, value) in upstream_response.headers() {
        let hop_by_hop = matches!(
            name.as_str(),
            "connection" | "keep-alive" | "transfer-encoding" | "content-length" | "upgrade"
        );
        if !hop_by_hop {
            if let (Ok(name), Ok(value)) = (
                HeaderName::from_bytes(name.as_str().as_bytes()),
                HeaderValue::from_bytes(value.as_bytes()),
            ) {
                response = response.header(name, value);
            }
        }
    }
    // Stream the body: model responses are server-sent events.
    let stream = upstream_response
        .bytes_stream()
        .map_ok(move |chunk| {
            // The permit lives as long as this closure, i.e. as long as the stream.
            let _slot = &slot;
            Frame::data(chunk)
        })
        .map_err(std::io::Error::other);
    response
        .body(BodyExt::boxed(StreamBody::new(stream)))
        .unwrap_or_else(|_| text(StatusCode::BAD_GATEWAY, "bad upstream response\n"))
}
