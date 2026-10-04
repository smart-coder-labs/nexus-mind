//! Network behavior of the sandbox egress proxy: a fake upstream on localhost
//! records what actually reaches it, so these tests prove the credential is
//! injected, the sandbox's own credential never leaves, and denials are enforced.

use std::sync::{Arc, Mutex};

use axum::{extract::State, http::HeaderMap, routing::any, Router};
use nexusmind::factory::{
    egress::sign_run_token,
    egress_server::{serve, AnthropicAuth, EgressConfig, GithubPackagesAuth},
};

const KEY: &[u8] = b"integration-signing-key-000000000";

#[derive(Clone, Default)]
struct Seen(Arc<Mutex<Vec<(String, HeaderMap)>>>);

async fn fake_upstream() -> (String, Seen) {
    let seen = Seen::default();
    let app = Router::new()
        .fallback(any(
            |State(seen): State<Seen>, uri: axum::http::Uri, headers: HeaderMap| async move {
                seen.0.lock().unwrap().push((uri.to_string(), headers));
                "upstream-body"
            },
        ))
        .with_state(seen.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}"), seen)
}

async fn proxy(upstream: &str, nexusmind: Option<&str>) -> String {
    proxy_with(upstream, nexusmind, 64).await
}

async fn proxy_with(upstream: &str, nexusmind: Option<&str>, max_in_flight: usize) -> String {
    proxy_with_options(upstream, nexusmind, max_in_flight, false).await
}

async fn proxy_with_options(
    upstream: &str,
    nexusmind: Option<&str>,
    max_in_flight: usize,
    allow_private_upstreams: bool,
) -> String {
    let config = EgressConfig {
        signing_key: KEY.to_vec(),
        anthropic: Some(AnthropicAuth::OAuth("real-oauth-token".into())),
        openai_api_key: Some("sk-real-openai-key".into()),
        openai_models: vec!["m".into()],
        nexusmind_keys: nexusmind
            .map(|key| {
                [("org-1".to_string(), key.to_string())]
                    .into_iter()
                    .collect()
            })
            .unwrap_or_default(),
        // 127.0.0.1:443 has no listener locally: an allow-listed but unreachable host.
        tunnel_allowlist: vec!["registry.npmjs.org".into(), "127.0.0.1".into()],
        upstream_override: Some(upstream.to_string()),
        openai_requests: Default::default(),
        max_in_flight,
        allow_private_upstreams,
        github_packages_tokens: [(
            "org-1".to_string(),
            GithubPackagesAuth {
                token: "ghp_packages_org_1".to_string(),
                scopes: vec!["@acme".to_string()],
            },
        )]
        .into_iter()
        .collect(),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, Arc::new(config)));
    format!("http://{address}")
}

fn codex_token() -> String {
    let expires = chrono::Utc::now().timestamp() + 300;
    sign_run_token(KEY, "org-1", &nexusmind::factory::egress::codex_run_id("run-42"), expires).unwrap()
}

fn run_token() -> String {
    let expires = chrono::Utc::now().timestamp() + 300;
    sign_run_token(KEY, "org-1", "run-42", expires).unwrap()
}

#[tokio::test]
async fn injects_the_credential_and_strips_the_sandbox_one() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let response = reqwest::Client::new()
        .post(format!(
            "{base}/r/{}/anthropic/v1/messages?beta=true",
            run_token()
        ))
        .header("authorization", "Bearer placeholder-not-a-real-token")
        .header("x-api-key", "sandbox-key")
        .header("anthropic-beta", "prompt-caching-2024-07-31")
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.text().await.unwrap(), "upstream-body");

    let seen = seen.0.lock().unwrap();
    let (uri, headers) = &seen[0];
    assert_eq!(uri, "/v1/messages?beta=true");
    assert_eq!(headers["authorization"], "Bearer real-oauth-token");
    assert!(
        headers.get("x-api-key").is_none(),
        "sandbox credential must not leak"
    );
    let beta = headers["anthropic-beta"].to_str().unwrap();
    assert!(beta.contains("prompt-caching-2024-07-31") && beta.contains("oauth-2025-04-20"));
}

#[tokio::test]
async fn the_openai_route_injects_the_api_key_and_strips_the_placeholder() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{base}/r/{}/openai/v1/responses", codex_token()))
        .header("authorization", "Bearer sandbox-placeholder-not-a-credential")
        .header("openai-organization", "org-chosen-by-sandbox")
        .body(r#"{"model":"m","store":true,"tools":[{"type":"function","name":"shell"}]}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let post = |path: &'static str, token: String, body: &'static str| {
        client.post(format!("{base}/r/{token}/openai{path}")).body(body).send()
    };
    // Another path, a hosted tool, and a Claude agent's token are all refused.
    assert_eq!(post("/v1/files", codex_token(), "{}").await.unwrap().status(), 403);
    assert_eq!(
        post("/v1/responses", codex_token(), r#"{"tools":[{"type":"mcp","server_url":"https://evil.example"}]}"#)
            .await
            .unwrap()
            .status(),
        403
    );
    assert_eq!(post("/v1/responses", run_token(), "{}").await.unwrap().status(), 403);
    assert_eq!(post("/v1/responses", codex_token(), r#"{"model":"gpt-6-pro"}"#).await.unwrap().status(), 403);

    let seen = seen.0.lock().unwrap();
    assert_eq!(seen.len(), 1, "only the allowed request reaches upstream");
    let (uri, headers) = &seen[0];
    assert_eq!(uri, "/v1/responses");
    // The body was rewritten (store forced off): its length is the new one.
    let length: usize = headers["content-length"].to_str().unwrap().parse().unwrap();
    assert_eq!(length, br#"{"max_output_tokens":64000,"model":"m","store":false,"tools":[{"name":"shell","type":"function"}]}"#.len());
    assert_eq!(headers["authorization"], "Bearer sk-real-openai-key");
    assert!(headers.get("openai-organization").is_none(), "the sandbox never picks the billed org");
}

#[tokio::test]
async fn requests_without_a_valid_run_token_never_reach_upstream() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let client = reqwest::Client::new();
    for path in ["/v1/messages", "/r/forged-token/anthropic/v1/messages"] {
        let response = client
            .post(format!("{base}{path}"))
            .body("{}")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 403, "{path}");
    }
    assert!(seen.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn an_unconfigured_upstream_fails_closed() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let response = reqwest::Client::new()
        .get(format!("{base}/r/{}/nexusmind/v1/context", run_token()))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 503);
    assert!(seen.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tunnels_to_hosts_off_the_allowlist_are_refused() {
    let (upstream, _) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let address = base.trim_start_matches("http://");
    use base64::Engine;
    let auth = base64::engine::general_purpose::STANDARD.encode(format!("run:{}", run_token()));
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(
            format!("CONNECT evil.example:443 HTTP/1.1\r\nHost: evil.example:443\r\nProxy-Authorization: Basic {auth}\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut buffer = [0u8; 64];
    let read = stream.read(&mut buffer).await.unwrap();
    let head = String::from_utf8_lossy(&buffer[..read]);
    assert!(head.starts_with("HTTP/1.1 403"), "{head}");
}

async fn raw_connect(base: &str, target: &str, auth: Option<String>) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(base.trim_start_matches("http://"))
        .await
        .unwrap();
    let auth = auth
        .map(|value| format!("Proxy-Authorization: {value}\r\n"))
        .unwrap_or_default();
    stream
        .write_all(format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n{auth}\r\n").as_bytes())
        .await
        .unwrap();
    let mut buffer = [0u8; 512];
    let read = stream.read(&mut buffer).await.unwrap();
    String::from_utf8_lossy(&buffer[..read]).to_string()
}

fn basic_auth() -> String {
    use base64::Engine;
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(format!("run:{}", run_token()))
    )
}

#[tokio::test]
async fn a_tunnel_without_credentials_gets_a_407_challenge() {
    let (upstream, _) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let head = raw_connect(&base, "registry.npmjs.org:443", None).await;
    assert!(head.starts_with("HTTP/1.1 407"), "{head}");
    assert!(
        head.to_ascii_lowercase()
            .contains("proxy-authenticate: basic"),
        "{head}"
    );
}

#[tokio::test]
async fn an_unreachable_allowlisted_host_gets_502_not_200() {
    let (upstream, _) = fake_upstream().await;
    // Private destinations allowed only to reach a local, closed port.
    let base = proxy_with_options(&upstream, None, 64, true).await;
    let head = raw_connect(&base, "127.0.0.1:443", Some(basic_auth())).await;
    assert!(head.starts_with("HTTP/1.1 502"), "{head}");
}

#[tokio::test]
async fn an_allowlisted_name_that_resolves_inside_is_refused() {
    let (upstream, _) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    // Allow-listed, but it resolves to loopback: never a tunnel destination.
    let head = raw_connect(&base, "127.0.0.1:443", Some(basic_auth())).await;
    assert!(head.starts_with("HTTP/1.1 403"), "{head}");
}

#[tokio::test]
async fn requests_beyond_the_in_flight_limit_are_shed() {
    // An upstream that never answers keeps the only slot busy.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let slow = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            let (socket, _) = listener.accept().await.unwrap();
            held.push(socket);
        }
    });
    let base = proxy_with(&slow, None, 1).await;
    let url = format!("{base}/r/{}/anthropic/v1/messages", run_token());
    let first = tokio::spawn({
        let url = url.clone();
        async move { reqwest::Client::new().post(url).body("{}").send().await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let second = reqwest::Client::new()
        .post(url)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(second.status(), 503);
    first.abort();
}

#[tokio::test]
async fn nexusmind_gets_the_bot_key_of_the_runs_organization() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, Some("nm_bot_key_org_1")).await;
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/r/{}/nexusmind/v1/context?project=web",
            run_token()
        ))
        .header("authorization", "Bearer sandbox-guess")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let seen = seen.0.lock().unwrap();
    assert_eq!(seen[0].0, "/v1/context?project=web");
    assert_eq!(seen[0].1["authorization"], "Bearer nm_bot_key_org_1");
    assert!(seen[0].1.get("anthropic-beta").is_none());
}

#[tokio::test]
async fn a_streaming_response_keeps_its_slot_until_it_finishes() {
    // An upstream that sends headers at once and then streams forever.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let streaming = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;
        let mut held = Vec::new();
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let _ = tokio::io::AsyncReadExt::read(&mut socket, &mut buffer).await;
            socket
                .write_all(b"HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ntransfer-encoding: chunked\r\n\r\n5\r\nhello\r\n")
                .await
                .unwrap();
            held.push(socket);
        }
    });
    let base = proxy_with(&streaming, None, 1).await;
    let url = format!("{base}/r/{}/anthropic/v1/messages", run_token());
    let first = reqwest::Client::new()
        .post(&url)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        first.status(),
        200,
        "headers arrive, the body keeps streaming"
    );
    let second = reqwest::Client::new()
        .post(&url)
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        503,
        "the open stream still holds the only slot"
    );
    drop(first);
}

/// An upstream that echoes the body it received and how it was framed.
async fn echo_upstream() -> String {
    let app = Router::new().fallback(any(
        |headers: HeaderMap, body: axum::body::Bytes| async move {
            let framing = if headers.contains_key("content-length") {
                "length"
            } else {
                "chunked"
            };
            format!("{framing}:{}", String::from_utf8_lossy(&body))
        },
    ));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    format!("http://{address}")
}

#[tokio::test]
async fn request_bodies_are_forwarded_intact_whether_sized_or_streamed() {
    let base = proxy(&echo_upstream().await, None).await;
    let url = format!("{base}/r/{}/anthropic/v1/messages", run_token());
    let client = reqwest::Client::new();
    let payload = "x".repeat(300_000);

    let sized = client
        .post(&url)
        .body(payload.clone())
        .send()
        .await
        .unwrap();
    assert_eq!(sized.status(), 200);
    assert_eq!(sized.text().await.unwrap(), format!("length:{payload}"));

    let chunks: Vec<Result<_, std::io::Error>> = vec![Ok("part-1,"), Ok("part-2")];
    let streamed = client
        .post(&url)
        .body(reqwest::Body::wrap_stream(futures_util::stream::iter(
            chunks,
        )))
        .send()
        .await
        .unwrap();
    assert_eq!(streamed.status(), 200);
    assert_eq!(streamed.text().await.unwrap(), "chunked:part-1,part-2");
}

#[tokio::test]
async fn an_oversized_declared_body_is_refused_before_reaching_upstream() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let address = base.trim_start_matches("http://");
    let path = format!("/r/{}/anthropic/v1/messages", run_token());
    let mut stream = tokio::net::TcpStream::connect(address).await.unwrap();
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    stream
        .write_all(
            format!("POST {path} HTTP/1.1\r\nHost: proxy\r\nContent-Length: 999999999\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut head = [0u8; 12];
    stream.read_exact(&mut head).await.unwrap();
    assert_eq!(&head, b"HTTP/1.1 413");
    assert!(seen.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn the_anthropic_credential_only_reaches_inference_endpoints() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let response = reqwest::Client::new()
        .get(format!(
            "{base}/r/{}/anthropic/api/oauth/profile",
            run_token()
        ))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);
    assert!(seen.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn github_packages_gets_the_orgs_read_token_even_for_registry_tokens() {
    let (upstream, seen) = fake_upstream().await;
    let base = proxy(&upstream, None).await;
    let expires = chrono::Utc::now().timestamp() + 300;
    let registry = sign_run_token(
        KEY,
        "org-1",
        &nexusmind::factory::egress::registry_only_run_id("run-42"),
        expires,
    )
    .unwrap();
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{base}/r/{registry}/ghpkg/@acme%2fui"))
        .header("authorization", "Bearer sandbox-guess")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    {
        let seen = seen.0.lock().unwrap();
        assert_eq!(seen[0].0, "/@acme%2fui");
        assert_eq!(seen[0].1["authorization"], "Bearer ghp_packages_org_1");
    }
    // Only the org's allowed scopes: never other packages the token can read,
    // nor non-package endpoints.
    for path in [
        "@othercorp%2fsecret",
        "-/whoami",
        "download/@othercorp/secret/1.0.0/x",
    ] {
        let refused = client
            .get(format!("{base}/r/{registry}/ghpkg/{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status(), 403, "{path}");
    }
    // Writes never reach the registry.
    let publish = client
        .put(format!("{base}/r/{registry}/ghpkg/@acme%2fui"))
        .body("{}")
        .send()
        .await
        .unwrap();
    assert_eq!(publish.status(), 403);
    // An org without a token fails closed.
    let other = sign_run_token(KEY, "org-2", "run-7", expires).unwrap();
    let unconfigured = client
        .get(format!("{base}/r/{other}/ghpkg/@acme%2fui"))
        .send()
        .await
        .unwrap();
    assert_eq!(unconfigured.status(), 503);
    assert_eq!(seen.0.lock().unwrap().len(), 1);
}
