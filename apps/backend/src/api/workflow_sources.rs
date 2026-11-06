//! Read-only Git transport. GitHub credentials remain in the backend.
use crate::{
    api::workflows::{fail, locked, runner_auth},
    db::{workflow_execution, workflows},
    factory::workflow_publication::{backend_account_enabled, github_repository},
    models::types::ApiError,
    store::sqlite::SqliteStore,
};
use anyhow::{bail, Result};
use axum::{
    body::{Body, Bytes},
    extract::{Path, RawQuery, State},
    http::{header, HeaderMap, StatusCode},
    response::Response,
    Json,
};
use futures_util::StreamExt;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::time::{Duration, Instant};

type Error = (StatusCode, Json<ApiError>);
const MAX_PACK: usize = 256 * 1024 * 1024;
static TRANSFERS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

// The route selects a snapshot index, never an arbitrary upstream URL.
fn authorize(c: &Connection, org: &str, runner: &str, source: (&str, &str, usize),
             fence: &str, operation: Option<&str>, now: i64) -> Result<String> {
    let (run, step, index) = source;
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let attempt = workflow_execution::lease(&tx, org, runner, run, step, fence, now)?;
    let raw: String = tx.query_row("SELECT snapshot_json FROM factory_workflow_runs WHERE id=?1", [run], |r| r.get(0))?;
    let snapshot: Value = serde_json::from_str(&raw)?;
    let source_step = snapshot["definition"]["steps"].as_array()
        .and_then(|steps| steps.iter().find(|s| s["id"] == step));
    if snapshot["sources_pending"] != true || !source_step.is_some_and(|s|
        s["kind"] == "prepare" && s["adapter"] == "prepare-workspace") {
        bail!("source_step_not_allowed");
    }
    let remote = snapshot["workspace"]["repositories"].as_array()
        .and_then(|repos| repos.get(index)).and_then(|r| r["remote"].as_str())
        .ok_or_else(|| anyhow::anyhow!("source_repository_not_allowed"))?;
    let repository = github_repository(remote)?;
    if let Some(operation) = operation {
        let count: u32 = tx.query_row("SELECT count(*) FROM factory_workflow_events WHERE run_id=?1 AND step_id=?2 AND kind='source.git.requested' AND json_extract(payload_json,'$.repository')=?3 AND json_extract(payload_json,'$.attempt')=?4", params![run,step,repository,attempt], |r| r.get(0))?;
        if count >= 16 { bail!("source_request_quota"); }
        workflows::event(&tx, run, Some(step), "source.git.requested", json!({"repository":repository,"attempt":attempt,"operation":operation}))?;
    }
    tx.commit()?;
    Ok(repository)
}

pub async fn advertise(State(store): State<SqliteStore>, headers: HeaderMap,
    Path((run, step, index)): Path<(String, String, usize)>, RawQuery(query): RawQuery,
) -> Result<Response, Error> {
    if query.as_deref() != Some("service=git-upload-pack") {
        return Err(fail(anyhow::anyhow!("source_git_read_only")));
    }
    proxy(store, headers, run, step, index, None).await
}

pub async fn upload_pack(State(store): State<SqliteStore>, headers: HeaderMap,
    Path((run, step, index)): Path<(String, String, usize)>, body: Bytes,
) -> Result<Response, Error> {
    if body.len() > 1024 * 1024 || headers.get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok()) != Some("application/x-git-upload-pack-request") {
        return Err(fail(anyhow::anyhow!("invalid_source_git_request")));
    }
    proxy(store, headers, run, step, index, Some(body)).await
}

fn upstream_request(repository: &str, token: String, protocol: Option<&axum::http::HeaderValue>,
                    encoding: Option<&axum::http::HeaderValue>, body: Option<Bytes>) -> Result<reqwest::RequestBuilder> {
    crate::automation::connectors::validate_repository(repository)?;
    let suffix = if body.is_some() { "git-upload-pack" } else { "info/refs?service=git-upload-pack" };
    let client = reqwest::Client::builder().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(10)).timeout(Duration::from_secs(90)).build()?;
    let method = if body.is_some() { reqwest::Method::POST } else { reqwest::Method::GET };
    let mut request = client.request(method, format!("https://github.com/{repository}.git/{suffix}"))
        .basic_auth("x-access-token", Some(token)).header(header::USER_AGENT, "NexusMind-source-read");
    if let Some(protocol) = protocol {
        if protocol != "version=2" { bail!("invalid_source_git_protocol"); }
        request = request.header("git-protocol", "version=2");
    }
    if let Some(encoding) = encoding {
        if body.is_none() || !["gzip", "identity"].iter().any(|value| encoding == *value) {
            bail!("invalid_source_git_encoding");
        }
        // Git compresses larger negotiation requests; forward only this typed
        // encoding header, never caller authorization/cookies/lease headers.
        request = request.header(header::CONTENT_ENCODING, encoding);
    }
    if let Some(body) = body {
        request = request.header(header::CONTENT_TYPE, "application/x-git-upload-pack-request").body(body);
    }
    Ok(request)
}

async fn proxy(store: SqliteStore, headers: HeaderMap, run: String, step: String,
               index: usize, body: Option<Bytes>) -> Result<Response, Error> {
    let fence = headers.get("x-nexusmind-lease").and_then(|v| v.to_str().ok())
        .filter(|v| !v.is_empty() && v.len() <= 256)
        .ok_or_else(|| fail(anyhow::anyhow!("lease_conflict")))?.to_string();
    let operation = if body.is_some() { "upload-pack" } else { "advertise" };
    let (runner, org, repository) = {
        let handle = store.conn();
        let c = handle.lock().map_err(|_| locked())?;
        let (runner, org) = runner_auth(&c, &headers)?;
        if !backend_account_enabled(&org) {
            return Err(fail(anyhow::anyhow!("backend_github_source_not_authorized")));
        }
        let repository = authorize(&c, &org, &runner, (&run, &step, index), &fence, Some(operation), chrono::Utc::now().timestamp()).map_err(fail)?;
        (runner, org, repository)
    };
    let permit = TRANSFERS.try_acquire().map_err(|_| fail(anyhow::anyhow!("source_transfer_busy")))?;
    let token = crate::automation::worker::server_github_token().await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_auth_required")))?;
    let expected = if body.is_some() { "application/x-git-upload-pack-result" } else { "application/x-git-upload-pack-advertisement" };
    let request = upstream_request(&repository, token, headers.get("git-protocol"), headers.get(header::CONTENT_ENCODING), body).map_err(fail)?;
    let upstream = request.send().await.map_err(|_| fail(anyhow::anyhow!("source_github_unavailable")))?;
    if !upstream.status().is_success() {
        let code = if matches!(upstream.status().as_u16(), 401 | 403 | 404) { "source_github_access_denied" } else { "source_github_unavailable" };
        return Err(fail(anyhow::anyhow!(code)));
    }
    if upstream.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()) != Some(expected)
        || upstream.content_length().is_some_and(|n| n > MAX_PACK as u64) {
        return Err(fail(anyhow::anyhow!("invalid_source_git_response")));
    }
    // Recheck after authentication/network waits and periodically while streaming.
    let check = move || -> Result<()> {
        let handle = store.conn();
        let c = handle.lock().map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
        if !backend_account_enabled(&org) { bail!("backend_github_source_not_authorized"); }
        authorize(&c, &org, &runner, (&run, &step, index), &fence, None, chrono::Utc::now().timestamp())?;
        Ok(())
    };
    check().map_err(fail)?;
    let mut total = 0usize;
    let mut checked = Instant::now();
    let stream = upstream.bytes_stream().map(move |chunk| {
        let _keep_permit = &permit;
        let chunk = chunk.map_err(|_| std::io::Error::other("source_transfer_failed"))?;
        total = total.saturating_add(chunk.len());
        if total > MAX_PACK { return Err(std::io::Error::other("source_transfer_quota")); }
        if checked.elapsed() > Duration::from_secs(5) {
            check().map_err(|_| std::io::Error::other("source_lease_revoked"))?;
            checked = Instant::now();
        }
        Ok(chunk)
    });
    Ok(Response::builder().header(header::CONTENT_TYPE, expected)
        .header(header::CACHE_CONTROL, "no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from_stream(stream)).expect("static response headers"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{db::{connection, migrations, queries}, factory::workflows::{Definition, Repository}};

    #[test]
    fn source_reads_require_current_preparation_lease_and_snapshot_scope() {
        let c = connection::connect(":memory:").unwrap();
        migrations::run(&c).unwrap();
        let (org, user, _) = queries::bootstrap(&c, "Test", "test", "test@example.com", "Owner").unwrap();
        let runner = workflows::register_runner(&c, &org.id, "Runner").unwrap();
        let rid = runner["id"].as_str().unwrap();
        let workspace = workflows::create_workspace(&c, &org.id, rid, "Project", "/project").unwrap();
        let wid = workspace["id"].as_str().unwrap();
        let repo: Repository = serde_json::from_value(json!({"path":"api","remote":"https://github.com/acme/api.git","branch":"master","head_sha":"a".repeat(40),"writable":false})).unwrap();
        workflows::report_inventory(&c, &org.id, rid, wid, &[repo.clone()]).unwrap();
        workflows::confirm_inventory(&c, &org.id, wid, 1, &[repo]).unwrap();
        let definition: Definition = serde_json::from_value(json!({"name":"Prepare","workspace_id":wid,"steps":[{"id":"prepare","name":"Prepare","kind":"prepare","adapter":"prepare-workspace"}]})).unwrap();
        let definition = workflows::create_definition(&c, &org.id, &user.id, &definition).unwrap();
        let run = workflows::start_run(&c, &org.id, definition["id"].as_str().unwrap(), "source:test", &json!({})).unwrap();
        let run = run["id"].as_str().unwrap();
        let caps = vec!["adapter:prepare-workspace".into(),"workspace-sources:v1".into(),"workspace-git-read:v1".into()];
        let job = workflows::claim(&c, &org.id, rid, &caps, 100).unwrap().unwrap();
        let fence = job["lease_token"].as_str().unwrap();
        assert_eq!(authorize(&c, &org.id, rid, (run, "prepare", 0), fence, None, 101).unwrap(), "acme/api");
        assert!(authorize(&c, "foreign", rid, (run, "prepare", 0), fence, None, 101).is_err());
        assert!(authorize(&c, &org.id, "other-runner", (run, "prepare", 0), fence, None, 101).is_err());
        assert!(authorize(&c, &org.id, rid, (run, "prepare", 1), fence, None, 101).is_err());
        assert!(authorize(&c, &org.id, rid, (run, "prepare", 0), "wrong-fence", None, 101).is_err());
        assert!(authorize(&c, &org.id, rid, (run, "agent", 0), fence, None, 101).is_err());
        assert!(authorize(&c, &org.id, rid, (run, "prepare", 0), fence, None, 10000).is_err());
        for _ in 0..16 { authorize(&c, &org.id, rid, (run, "prepare", 0), fence, Some("advertise"), 101).unwrap(); }
        assert!(authorize(&c, &org.id, rid, (run, "prepare", 0), fence, Some("advertise"), 101).is_err());
        c.execute("UPDATE factory_workflow_runs SET snapshot_json=json_set(snapshot_json,'$.sources_pending',json('false')) WHERE id=?1", [run]).unwrap();
        assert!(authorize(&c, &org.id, rid, (run, "prepare", 0), fence, None, 101).is_err());
    }

    #[tokio::test]
    async fn rejects_push_services_and_non_git_uploads_without_contacting_upstream() {
        let store = SqliteStore::new(connection::connect(":memory:").unwrap());
        let path = || Path(("run".into(), "prepare".into(), 0));
        let error = advertise(State(store.clone()), HeaderMap::new(), path(),
            RawQuery(Some("service=git-receive-pack".into()))).await.unwrap_err();
        assert_eq!(error.1.0.code, "source_git_read_only");
        let error = upload_pack(State(store), HeaderMap::new(), path(), Bytes::from_static(b"not git")).await.unwrap_err();
        assert_eq!(error.1.0.code, "invalid_source_git_request");
    }

    #[test]
    fn upstream_requests_only_read_github_and_never_include_nexus_credentials() {
        use base64::Engine;
        let protocol = axum::http::HeaderValue::from_static("version=2");
        for body in [None, Some(Bytes::from_static(b"fixture git request"))] {
            let gzip = axum::http::HeaderValue::from_static("gzip");
            let encoding = body.as_ref().map(|_| &gzip);
            let request = upstream_request("acme/api", "fixture-github-secret".into(), Some(&protocol), encoding, body).unwrap().build().unwrap();
            if request.method() == reqwest::Method::POST {
                assert_eq!(request.headers()[header::CONTENT_ENCODING], "gzip");
            }
            assert_eq!(request.url().host_str(), Some("github.com"));
            assert!(matches!(request.method().as_str(), "GET" | "POST"));
            let auth = request.headers()[header::AUTHORIZATION].to_str().unwrap().strip_prefix("Basic ").unwrap();
            assert_eq!(base64::engine::general_purpose::STANDARD.decode(auth).unwrap(), b"x-access-token:fixture-github-secret");
            assert!(!request.headers().contains_key("x-nexusmind-lease"));
            assert!(!request.headers().contains_key("cookie"));
            assert!(request.url().path().ends_with("/info/refs") || request.url().path().ends_with("/git-upload-pack"));
        }
        assert!(upstream_request("https://evil.example/repo", "fixture".into(), None, None, None).is_err());
        assert!(upstream_request("acme/api", "fixture".into(), Some(&axum::http::HeaderValue::from_static("unsupported")), None, None).is_err());
        assert!(upstream_request("acme/api", "fixture".into(), None, Some(&axum::http::HeaderValue::from_static("unsupported")), Some(Bytes::new())).is_err());
    }
}
