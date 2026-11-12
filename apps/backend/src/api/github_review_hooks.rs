//! Register signed repository webhooks using the account authenticated in the
//! BACKEND's gh CLI. Browser sessions and agent/model credentials are unrelated.
use crate::{
    api::{
        helpers::{require_explicit_permission, AppJson},
        workflows::{fail, locked},
    },
    automation::{connectors, worker},
    db::queries,
    models::types::{ApiError, AuthContext, PutAutonomousAgentConnectorRequest},
    store::sqlite::SqliteStore,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
type Response = Result<Json<Value>, (StatusCode, Json<ApiError>)>;
fn callback(base: &str, id: &str) -> anyhow::Result<String> {
    let url = reqwest::Url::parse(base)?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("public_https_webhook_base_required")
    }
    Ok(format!(
        "{}/v1/autonomous-agents/github/webhook/{id}",
        base.trim_end_matches('/')
    ))
}
fn registration_repositories(
    allowed: Vec<String>,
    selected: Option<&str>,
) -> anyhow::Result<Vec<String>> {
    if let Some(repo) = selected.map(str::trim).filter(|r| !r.is_empty()) {
        let repo = repo.to_ascii_lowercase();
        if !allowed.contains(&repo) {
            anyhow::bail!("review_repository_out_of_scope");
        }
        return Ok(vec![repo]);
    }
    if allowed.is_empty() {
        anyhow::bail!("reviewer_repository_missing");
    }
    Ok(allowed)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub definition_id: String,
    pub public_base_url: String,
    /// Optional subset; omitted means every repository in the reviewer's scope.
    #[serde(default)]
    pub repository: Option<String>,
    /// Optional webhook-management credential, encrypted per org/repository. Never used by reviewers.
    #[serde(default)]
    pub administration_token: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdministrationToken {
    pub administration_token: Option<String>,
}
fn validate_token(token: Option<&str>) -> anyhow::Result<Option<&str>> {
    let token = token.map(str::trim).filter(|s| !s.is_empty());
    if token.is_some_and(|s| s.len() > 4096 || !s.bytes().all(|b| b.is_ascii_graphic())) {
        anyhow::bail!("invalid_github_administration_token");
    }
    Ok(token)
}
fn credential_for<'a>(connectors: &'a [crate::models::types::AutonomousAgentConnector], repo: &str) -> Option<&'a crate::models::types::AutonomousAgentConnector> {
    connectors.iter().find(|c| c.kind == "target_secret" && c.health != "revoked" && c.secret_configured
        && c.metadata["purpose"] == "github_webhook_management"
        && c.metadata["repository"].as_str().is_some_and(|r| r.eq_ignore_ascii_case(repo)))
}
fn saved_token(c: &rusqlite::Connection, org: &str, repo: &str) -> anyhow::Result<Option<String>> {
    let connectors = queries::list_autonomous_agent_connectors(c, org)?;
    let Some(credential) = credential_for(&connectors, repo) else { return Ok(None) };
    Ok(queries::get_autonomous_agent_connector_secret(c, org, &credential.id)?.map(|(_, s)| s))
}
fn save_token(c: &rusqlite::Connection, auth: &AuthContext, repo: &str, token: &str) -> anyhow::Result<()> {
    let tx = c.unchecked_transaction()?;
    queries::put_autonomous_agent_connector(&tx, &auth.org_id, &auth.user_id, &PutAutonomousAgentConnectorRequest {
        kind: "target_secret".into(), name: format!("GitHub webhook administration: {}", repo.to_ascii_lowercase()), secret: Some(token.into()),
        metadata: json!({"purpose":"github_webhook_management","repository":repo.to_ascii_lowercase()}),
        scopes: vec!["target:use".into(), "github:webhook".into()],
    })?;
    queries::append_audit_chained(&tx, &auth.org_id, &auth.user_id, "github.webhook_credential_saved", "repository", None, json!({"repository":repo}), None)?;
    tx.commit()?;
    Ok(())
}
async fn repository_hooks(token: &str, repo: &str) -> anyhow::Result<Vec<Value>> {
    let mut hooks = Vec::new();
    let mut page = 1;
    loop {
        let value = connectors::github_get(token, &format!("/repos/{repo}/hooks?per_page=100&page={page}")).await?;
        let batch = value.as_array().ok_or_else(|| anyhow::anyhow!("invalid_github_hooks"))?;
        hooks.extend(batch.iter().cloned());
        if batch.len() < 100 { return Ok(hooks) }
        page += 1;
    }
}
fn matching_hook(h: &Value, metadata: &Value, connector: &str) -> bool {
    (metadata["hook_id"].as_i64().is_some_and(|id| id > 0) && h["id"] == metadata["hook_id"])
        || (metadata["callback_url"].as_str().is_some() && h["config"]["url"] == metadata["callback_url"])
        || h["config"]["url"].as_str().is_some_and(|url| url.ends_with(&format!("/v1/autonomous-agents/github/webhook/{connector}")))
}
fn externally_registered(metadata: &Value, url: &str) -> bool {
    metadata["registration_method"] == "repository_admin"
        && metadata["callback_url"] == url
        && metadata["hook_id"].as_i64().is_some_and(|id| id > 0)
}
fn github_setup_failure(error: anyhow::Error) -> (StatusCode, Json<ApiError>) {
    let status = error
        .downcast_ref::<reqwest::Error>()
        .and_then(|e| e.status())
        .map(|s| s.as_u16())
        .or_else(|| {
            error
                .to_string()
                .strip_prefix("github_api_")
                .and_then(|s| s.split(':').next())
                .and_then(|s| s.parse::<u16>().ok())
        });
    let (http,code,message)=match status {
        Some(403|404)=>(StatusCode::FORBIDDEN,"github_webhook_permissions_required","GitHub denied webhook administration. NexusMind superuser permissions do not grant GitHub permissions. Supply a setup-only token with webhook administration access to this repository, or have its GitHub administrator configure the hook. The reviewer account does not need to become a repository administrator."),
        Some(401)=>(StatusCode::SERVICE_UNAVAILABLE,"github_webhook_authentication_required","GitHub rejected the setup credential. Check the administration token or the backend gh authentication."),
        _=>(StatusCode::BAD_GATEWAY,"github_webhook_unavailable","GitHub webhook setup is unavailable. Retry later; this is not a NexusMind role-permission error."),
    };
    (
        http,
        Json(ApiError {
            code: code.into(),
            error: message.into(),
        }),
    )
}
pub async fn list(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Response {
    let hooks = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        require_explicit_permission(&c, &auth, None, "autonomous_agent:read")?;
        let connectors = queries::list_autonomous_agent_connectors(&c, &auth.org_id).map_err(fail)?;
        connectors.iter()
            .filter(|c| c.metadata["purpose"] == "github_cli_webhook")
            .map(|c| {
                let credential = c.metadata["repository"].as_str().and_then(|repo| credential_for(&connectors, repo));
                json!({"id":c.id,"health":c.health,"metadata":c.metadata,"management_credential_id":credential.map(|c| &c.id)})
            })
            .collect::<Vec<_>>()
    };
    let login = worker::server_github_identity().await.ok();
    Ok(Json(
        json!({"login":login,"hooks":hooks,"identity_source":"backend_gh_cli"}),
    ))
}
pub async fn register(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    AppJson(b): AppJson<Register>,
) -> Response {
    callback(&b.public_base_url, "validate").map_err(fail)?;
    let repositories = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
        require_explicit_permission(&c, &auth, None, "autonomous_agent:manage_connectors")?;
        let d = queries::get_autonomous_agent_detail(&c, &auth.org_id, &b.definition_id)
            .map_err(fail)?
            .ok_or_else(|| fail(anyhow::anyhow!("not_found")))?;
        if d.definition.template_key != "github_pr_reviewer"
            || d.definition.status != "enabled"
            || d.revision.validation_status != "valid"
        {
            return Err(fail(anyhow::anyhow!("enabled_valid_reviewer_required")));
        }
        let allowed = crate::db::autonomous_scope::reviewer_repositories(
            &c,
            &auth.org_id,
            &d.revision.config,
        )
        .map_err(fail)?;
        registration_repositories(allowed, b.repository.as_deref()).map_err(fail)?
    };
    if !crate::crypto::is_configured() {
        return Err(fail(anyhow::anyhow!("credential_encryption_required")));
    }
    let token = worker::server_github_token()
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_unavailable")))?;
    let login = worker::github_identity_for_token(&token)
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_identity_unavailable")))?;
    let administration_token = validate_token(b.administration_token.as_deref()).map_err(fail)?;
    let mut registered = Vec::new();
    for repository in repositories {
        let saved = {
            let h = store.conn();
            let c = h.lock().map_err(|_| locked())?;
            saved_token(&c, &auth.org_id, &repository).map_err(fail)?
        };
        // An administrator may have configured the hook without granting the bot
        // repository-administration rights. Reuse only that exact stored callback.
        if administration_token.is_none() && saved.is_none() {
            let h = store.conn();
            let c = h.lock().map_err(|_| locked())?;
            if let Some(existing) = queries::list_autonomous_agent_connectors(&c, &auth.org_id)
                .map_err(fail)?
                .into_iter()
                .find(|c| {
                    c.kind == "target_secret"
                        && c.health == "ready"
                        && c.metadata["purpose"] == "github_cli_webhook"
                        && c.metadata["repository"] == repository
                        && callback(&b.public_base_url, &c.id)
                            .is_ok_and(|url| externally_registered(&c.metadata, &url))
                })
            {
                registered.push(
                    json!({"id":existing.id,"metadata":existing.metadata,"status":"configured"}),
                );
                continue;
            }
        }
        let setup_token = administration_token.or(saved.as_deref()).unwrap_or(&token);
        let prefix = format!("/repos/{repository}/hooks");
        // Preflight before persisting any new connector: failed authorization must
        // not leave a misleading ready/orphan registration behind.
        let hooks = repository_hooks(setup_token, &repository).await.map_err(github_setup_failure)?;
        if let Some(token) = administration_token {
            let h = store.conn();
            let c = h.lock().map_err(|_| locked())?;
            save_token(&c, &auth, &repository, token).map_err(fail)?;
        }
        let (id, secret) = {
            let h = store.conn();
            let c = h.lock().map_err(|_| locked())?;
            let existing = queries::list_autonomous_agent_connectors(&c, &auth.org_id)
                .map_err(fail)?
                .into_iter()
                .find(|c| {
                    c.kind == "target_secret"
                        && c.health != "revoked"
                        && c.metadata["purpose"] == "github_cli_webhook"
                        && c.metadata["repository"] == repository
                });
            if let Some(existing) = existing {
                let (_, secret) =
                    queries::get_autonomous_agent_connector_secret(&c, &auth.org_id, &existing.id)
                        .map_err(fail)?
                        .ok_or_else(|| fail(anyhow::anyhow!("webhook_secret_unavailable")))?;
                (existing.id, secret)
            } else {
                let secret = format!(
                    "{}{}",
                    uuid::Uuid::new_v4().simple(),
                    uuid::Uuid::new_v4().simple()
                );
                let connector=queries::put_autonomous_agent_connector(&c,&auth.org_id,&auth.user_id,&PutAutonomousAgentConnectorRequest{kind:"target_secret".into(),name:format!("GitHub reviews: {repository}"),secret:Some(secret.clone()),metadata:json!({"purpose":"github_cli_webhook","repository":repository,"definition_id":b.definition_id,"reviewer_login":login}),scopes:vec!["target:use".into(),"github:webhook".into()]}).map_err(fail)?;
                (connector.id, secret)
            }
        };
        let url = callback(&b.public_base_url, &id).map_err(fail)?;
        let existing = hooks
            .iter()
            .find(|h| h["config"]["url"] == url)
            .and_then(|h| h["id"].as_i64());
        let payload = json!({"name":"web","active":true,"events":["pull_request","issues"],"config":{"url":url,"content_type":"json","secret":secret,"insecure_ssl":"0"}});
        let hook = if let Some(hook) = existing {
            connectors::github_patch(setup_token, &format!("{prefix}/{hook}"), payload).await
        } else {
            connectors::github_post(setup_token, &prefix, payload).await
        }
        .map_err(github_setup_failure)?;
        let registration_method = if administration_token.is_some() || saved.is_some() {
            "repository_admin"
        } else {
            "backend_gh_cli"
        };
        let metadata = json!({"purpose":"github_cli_webhook","repository":repository,"definition_id":b.definition_id,"reviewer_login":login,"callback_url":url,"hook_id":hook["id"],"registration_method":registration_method});
        {
            let h = store.conn();
            let c = h.lock().map_err(|_| locked())?;
            c.execute("UPDATE autonomous_agent_connectors SET metadata_json=?1,health='ready',updated_at=datetime('now') WHERE id=?2 AND org_id=?3 AND health!='revoked'",rusqlite::params![metadata.to_string(),id,auth.org_id]).map_err(|e|fail(e.into()))?;
            queries::log_audit(
                &c,
                &auth.org_id,
                &auth.user_id,
                "github.webhook_registered",
                "autonomous_agent_connector",
                Some(&id),
                json!({"repository":repository,"reviewer_login":login,"hook_id":hook["id"]}),
            )
            .map_err(fail)?;
        }
        registered.push(json!({"id":id,"metadata":metadata}));
    }
    // Retain the single-hook fields for clients using an explicit repository.
    let mut result = registered[0].clone();
    result["login"] = json!(login);
    result["hooks"] = json!(registered);
    Ok(Json(result))
}
pub async fn disconnect(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if body.len() > 8192 { return Err(fail(anyhow::anyhow!("invalid_github_administration_token"))) }
    // Old clients send an empty DELETE body, sometimes with application/json.
    let request: AdministrationToken = if body.is_empty() { AdministrationToken::default() }
        else { serde_json::from_slice(&body).map_err(|_| fail(anyhow::anyhow!("invalid_github_administration_token")))? };
    let supplied = request.administration_token;
    let supplied = validate_token(supplied.as_deref()).map_err(fail)?;
    let metadata = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        require_explicit_permission(&c, &auth, None, "factory_policy:write")?;
        require_explicit_permission(&c, &auth, None, "autonomous_agent:manage_connectors")?;
        let connector = queries::list_autonomous_agent_connectors(&c, &auth.org_id)
            .map_err(fail)?
            .into_iter()
            .find(|c| c.id == id && c.metadata["purpose"] == "github_cli_webhook")
            .ok_or_else(|| fail(anyhow::anyhow!("not_found")))?;
        queries::revoke_autonomous_agent_connector(&c, &auth.org_id, &id).map_err(fail)?;
        connector.metadata
    };
    let cleanup = async {
        let repo = metadata["repository"].as_str().ok_or_else(|| anyhow::anyhow!("webhook_repository_missing"))?;
        connectors::validate_repository(repo)?;
        let saved = {
            let h = store.conn();
            let c = h.lock().map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
            saved_token(&c, &auth.org_id, repo)?
        };
        let token = match supplied.or(saved.as_deref()) {
            Some(token) => token.to_string(),
            None => worker::server_github_token().await?,
        };
        // A DELETE 404 can mean access denied. Verify listing access before claiming absence.
        let hooks = repository_hooks(&token, repo).await?;
        if let Some(token) = supplied {
            let h = store.conn();
            let c = h.lock().map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
            save_token(&c, &auth, repo, token)?;
        }
        for hook in hooks.iter().filter(|h| matching_hook(h, &metadata, &id)) {
            let hook = hook["id"].as_i64().filter(|id| *id>0).ok_or_else(|| anyhow::anyhow!("invalid_github_hook_id"))?;
            connectors::github_delete(&token, &format!("/repos/{repo}/hooks/{hook}")).await?;
        }
        Ok::<(), anyhow::Error>(())
    }.await;
    let deleted = cleanup.is_ok();
    let cleanup_error = cleanup.err().map(|e| github_setup_failure(e).1.0);
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        c.execute("UPDATE autonomous_agent_connectors SET metadata_json=json_set(metadata_json,'$.remote_deleted',json(?3)),updated_at=datetime('now') WHERE org_id=?1 AND id=?2",rusqlite::params![auth.org_id,id,if deleted {"true"} else {"false"}]).map_err(|e| fail(e.into()))?;
        queries::log_audit(&c,&auth.org_id,&auth.user_id,"github.webhook_disconnected","autonomous_agent_connector",Some(&id),json!({"remote_deleted":deleted})).map_err(fail)?;
    }
    Ok(Json(json!({"disconnected":true,"remote_deleted":deleted,"cleanup_pending":!deleted,"cleanup_error":cleanup_error})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn administration_tokens_are_encrypted_scoped_rotatable_and_survive_local_disconnect() {
        // Isolate crypto's process-wide environment from concurrently running tests.
        if std::env::var_os("NM_WEBHOOK_CREDENTIAL_TEST_CHILD").is_none() {
            let output=std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact","api::github_review_hooks::tests::administration_tokens_are_encrypted_scoped_rotatable_and_survive_local_disconnect"])
                .env("NM_WEBHOOK_CREDENTIAL_TEST_CHILD","1")
                .env("NEXUSMIND_TOKEN_ENCRYPTION_KEY","000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
                .output().unwrap();
            assert!(output.status.success(),"{}\n{}",String::from_utf8_lossy(&output.stdout),String::from_utf8_lossy(&output.stderr));
            return;
        }
        let c = crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run(&c).unwrap();
        let (o,u,_) = queries::bootstrap(&c,"Org","org","owner@test.com","Owner").unwrap();
        let auth = AuthContext{org_id:o.id.clone(),user_id:u.id.clone(),role:"super_user".parse().unwrap()};
        save_token(&c,&auth,"ACME/API","setup-token-fixture").unwrap();
        assert_eq!(saved_token(&c,&o.id,"acme/api").unwrap().as_deref(),Some("setup-token-fixture"));
        assert!(saved_token(&c,&o.id,"acme/other").unwrap().is_none());
        assert!(saved_token(&c,"other-org","acme/api").unwrap().is_none());
        let connectors=queries::list_autonomous_agent_connectors(&c,&o.id).unwrap();
        let credential=credential_for(&connectors,"acme/api").unwrap();
        let id=credential.id.clone();
        let ciphertext:String=c.query_row("SELECT secret_ciphertext FROM autonomous_agent_connectors WHERE id=?1",[&id],|r|r.get(0)).unwrap();
        assert_ne!(ciphertext,"setup-token-fixture");
        assert!(!serde_json::to_string(&connectors).unwrap().contains("setup-token-fixture"));
        save_token(&c,&auth,"acme/api","replacement-token").unwrap();
        assert_eq!(saved_token(&c,&o.id,"acme/api").unwrap().as_deref(),Some("replacement-token"));
        assert_eq!(queries::list_autonomous_agent_connectors(&c,&o.id).unwrap().len(),1);
        // Missing remote metadata simulates a failed registration/cleanup without GitHub access.
        c.execute("INSERT INTO autonomous_agent_connectors(id,org_id,kind,name,health,metadata_json,created_by) VALUES('hook',?1,'target_secret','Hook','ready','{\"purpose\":\"github_cli_webhook\"}',?2)",rusqlite::params![o.id,u.id]).unwrap();
        let store=SqliteStore::new(c);
        for _ in 0..2 {
            let result=disconnect(State(store.clone()),Extension(auth.clone()),Path("hook".into()),axum::body::Bytes::new()).await.unwrap().0;
            assert_eq!(result["remote_deleted"],false);
            assert_eq!(result["cleanup_pending"],true);
        }
        {
            let h=store.conn(); let c=h.lock().unwrap();
            assert_eq!(c.query_row("SELECT health FROM autonomous_agent_connectors WHERE id='hook'",[],|r|r.get::<_,String>(0)).unwrap(),"revoked");
            assert_eq!(saved_token(&c,&o.id,"acme/api").unwrap().as_deref(),Some("replacement-token"));
            queries::revoke_autonomous_agent_connector(&c,&o.id,&id).unwrap();
            assert!(saved_token(&c,&o.id,"acme/api").unwrap().is_none());
            assert!(c.query_row("SELECT secret_ciphertext IS NULL FROM autonomous_agent_connectors WHERE id=?1",[id],|r|r.get::<_,bool>(0)).unwrap());
        }
    }
    #[test]
    fn token_validation_and_cleanup_matching_do_not_accept_other_hooks() {
        assert!(validate_token(Some("bad\r\ntoken")).is_err());
        assert!(validate_token(Some(&"a".repeat(4097))).is_err());
        assert!(validate_token(Some("  ")).unwrap().is_none());
        let metadata=json!({"hook_id":42,"callback_url":"https://api.example.com/v1/autonomous-agents/github/webhook/local"});
        assert!(matching_hook(&json!({"id":42}),&metadata,"local"));
        assert!(matching_hook(&json!({"id":43,"config":{"url":"https://api.example.com/v1/autonomous-agents/github/webhook/local"}}),&metadata,"local"));
        assert!(!matching_hook(&json!({"id":43,"config":{"url":"https://api.example.com/v1/autonomous-agents/github/webhook/other"}}),&metadata,"local"));
        assert!(!matching_hook(&json!({}),&json!({}),"local"));
    }
    #[test]
    fn project_registration_defaults_to_all_authorized_repositories() {
        let repos = vec!["acme/first".into(), "acme/second".into()];
        assert_eq!(
            registration_repositories(repos.clone(), None).unwrap(),
            repos
        );
        assert_eq!(
            registration_repositories(repos.clone(), Some(" ")).unwrap(),
            repos
        );
        assert_eq!(
            registration_repositories(repos.clone(), Some("ACME/SECOND")).unwrap(),
            vec!["acme/second"]
        );
        assert!(registration_repositories(repos, Some("other/private")).is_err());
        assert!(registration_repositories(vec![], None).is_err());
    }
    #[test]
    fn external_registration_is_bound_to_callback_and_real_hook_id() {
        let mut m = json!({"registration_method":"repository_admin","callback_url":"https://api.example.com/hook","hook_id":42});
        assert!(externally_registered(&m, "https://api.example.com/hook"));
        assert!(!externally_registered(
            &m,
            "https://another.example.com/hook"
        ));
        m["hook_id"] = json!(null);
        assert!(!externally_registered(&m, "https://api.example.com/hook"));
    }
    #[test]
    fn upstream_authorization_and_transport_errors_are_not_app_role_errors() {
        let (status, Json(error)) = github_setup_failure(anyhow::anyhow!("github_api_403: denied"));
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(error.code, "github_webhook_permissions_required");
        assert!(error.error.contains("NexusMind superuser"));
        let (status, Json(error)) = github_setup_failure(anyhow::anyhow!("connection failed"));
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(error.code, "github_webhook_unavailable");
    }
    #[test]
    fn setup_token_is_optional_and_separate_from_reviewer_identity() {
        let b: Register = serde_json::from_value(
            json!({"definition_id":"reviewer","public_base_url":"https://api.example.com"}),
        )
        .unwrap();
        assert!(b.administration_token.is_none());
        let b:Register=serde_json::from_value(json!({"definition_id":"reviewer","public_base_url":"https://api.example.com","administration_token":"setup-only"})).unwrap();
        assert_eq!(b.administration_token.as_deref(), Some("setup-only"));
    }
    #[test]
    fn callback_rejects_credentials_and_non_tls() {
        for bad in [
            "http://example.com",
            "https://token@example.com",
            "https://example.com?key=secret",
        ] {
            assert!(callback(bad, "id").is_err());
        }
        assert_eq!(
            callback("https://api.example.com/", "abc").unwrap(),
            "https://api.example.com/v1/autonomous-agents/github/webhook/abc"
        );
    }
}
