//! Network behavior of the sandbox egress proxy: a fake upstream on localhost
//! records what actually reaches it, so these tests prove the credential is
//! injected, the sandbox's own credential never leaves, and denials are enforced.

use std::sync::{Arc, Mutex};

use axum::{extract::State, http::HeaderMap, routing::any, Router};
use nexusmind::factory::{
    egress::sign_run_token,
    egress_server::{serve, AnthropicAuth, EgressConfig},
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
    let config = EgressConfig {
        signing_key: KEY.to_vec(),
        anthropic: Some(AnthropicAuth::OAuth("real-oauth-token".into())),
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
        max_in_flight,
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(serve(listener, Arc::new(config)));
    format!("http://{address}")
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
    let base = proxy(&upstream, None).await;
    let head = raw_connect(&base, "127.0.0.1:443", Some(basic_auth())).await;
    assert!(head.starts_with("HTTP/1.1 502"), "{head}");
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
