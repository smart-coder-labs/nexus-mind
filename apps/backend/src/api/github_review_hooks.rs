//! Signed repository webhooks. Management tokens are encrypted and repository /
//! organization scoped; the reviewer continues to use the backend's gh identity.
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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub definition_id: String,
    pub public_base_url: String,
    pub setup_token: Option<String>,
}
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SetupToken {
    pub setup_token: Option<String>,
}
fn saved_management_token(
    c: &rusqlite::Connection,
    org: &str,
    repo: &str,
) -> anyhow::Result<Option<String>> {
    let existing = queries::list_autonomous_agent_connectors(c, org)?
        .into_iter()
        .find(|c| c.kind == "target_secret" && c.health != "revoked"
            && c.metadata["purpose"] == "github_webhook_management"
            && c.metadata["repository"].as_str().is_some_and(|r| r.eq_ignore_ascii_case(repo)));
    match existing {
        Some(cn) => Ok(queries::get_autonomous_agent_connector_secret(c, org, &cn.id)?
            .map(|(_, secret)| secret)),
        None => Ok(None),
    }
}
async fn management_token(
    store: &SqliteStore,
    auth: &AuthContext,
    repo: &str,
    supplied: Option<&str>,
) -> anyhow::Result<String> {
    let supplied = supplied.map(str::trim).filter(|s| !s.is_empty());
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        if let Some(token) = supplied {
            // A PAT is a single HTTP header value. Do not persist malformed/unbounded input.
            if token.len() > 4096 || !token.bytes().all(|b| b.is_ascii_graphic()) {
                anyhow::bail!("invalid_setup_token");
            }
            queries::put_autonomous_agent_connector(&c, &auth.org_id, &auth.user_id,
                &PutAutonomousAgentConnectorRequest {
                    kind: "target_secret".into(),
                    name: format!("GitHub webhook administration: {}", repo.to_ascii_lowercase()),
                    secret: Some(token.to_string()),
                    metadata: json!({"purpose":"github_webhook_management","repository":repo.to_ascii_lowercase()}),
                    scopes: vec!["target:use".into(), "github:webhook".into()],
                })?;
            queries::log_audit(&c, &auth.org_id, &auth.user_id, "github.webhook_credential_saved",
                "repository", None, json!({"repository":repo}))?;
            return Ok(token.to_string());
        }
        if let Some(token) = saved_management_token(&c, &auth.org_id, repo)? {
            return Ok(token);
        }
    }
    worker::server_github_token().await
}
async fn repository_hooks(token: &str, repo: &str) -> anyhow::Result<Vec<Value>> {
    let mut hooks = Vec::new();
    for page in 1.. {
        let value = connectors::github_get(token, &format!("/repos/{repo}/hooks?per_page=100&page={page}")).await?;
        let batch = value.as_array().ok_or_else(|| anyhow::anyhow!("invalid_github_hooks"))?;
        hooks.extend(batch.iter().cloned());
        if batch.len() < 100 {
            return Ok(hooks);
        }
    }
    unreachable!()
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
            .map(|hook| {
                let credential = hook.metadata["repository"].as_str().and_then(|repo| {
                    // Display presence only; reading the token is unnecessary here.
                    connectors.iter().find(|cn| cn.kind == "target_secret" && cn.health != "revoked"
                            && cn.secret_configured && cn.metadata["purpose"] == "github_webhook_management"
                            && cn.metadata["repository"].as_str().is_some_and(|r| r.eq_ignore_ascii_case(repo)))
                });
                json!({"id":hook.id,"health":hook.health,"metadata":hook.metadata,"management_credential_id":credential.map(|c| &c.id)})
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
    let repository = {
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
        d.revision.config["repository"]
            .as_str()
            .ok_or_else(|| fail(anyhow::anyhow!("reviewer_repository_missing")))?
            .to_ascii_lowercase()
    };
    connectors::validate_repository(&repository).map_err(fail)?;
    if !crate::crypto::is_configured() {
        return Err(fail(anyhow::anyhow!("credential_encryption_required")));
    }
    let login = worker::server_github_identity()
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_identity_unavailable")))?;
    let token = management_token(&store, &auth, &repository, b.setup_token.as_deref())
        .await.map_err(|_| fail(anyhow::anyhow!("github_webhook_credential_unavailable")))?;
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
    let prefix = format!("/repos/{repository}/hooks");
    let hooks = repository_hooks(&token, &repository)
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_webhook_permissions_required")))?;
    let existing = hooks
        .iter()
        .find(|h| h["config"]["url"] == url)
        .and_then(|h| h["id"].as_i64());
    let payload = json!({"name":"web","active":true,"events":["pull_request","issues"],"config":{"url":url,"content_type":"json","secret":secret,"insecure_ssl":"0"}});
    let hook = if let Some(hook) = existing {
        connectors::github_patch(&token, &format!("{prefix}/{hook}"), payload).await
    } else {
        connectors::github_post(&token, &prefix, payload).await
    }
    .map_err(|_| fail(anyhow::anyhow!("github_webhook_registration_failed")))?;
    let metadata = json!({"purpose":"github_cli_webhook","repository":repository,"definition_id":b.definition_id,"reviewer_login":login,"callback_url":url,"hook_id":hook["id"]});
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
    Ok(Json(json!({"id":id,"metadata":metadata,"login":login})))
}
pub async fn disconnect(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    body: Option<Json<SetupToken>>,
) -> Response {
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
    let supplied = body.and_then(|Json(b)| b.setup_token);
    let cleanup = async {
        let repo = metadata["repository"].as_str()
            .ok_or_else(|| anyhow::anyhow!("webhook_repository_missing"))?;
        connectors::validate_repository(repo)?;
        let token = management_token(&store, &auth, repo, supplied.as_deref()).await?;
        // Listing verifies access: a bare DELETE 404 could also mean permission denied.
        // Match the callback too, recovering registrations whose response was lost.
        let hooks = repository_hooks(&token, repo).await?;
        for hook in hooks.iter().filter(|h| {
            (metadata["hook_id"].as_i64().is_some() && h["id"] == metadata["hook_id"])
                || (metadata["callback_url"].as_str().is_some() && h["config"]["url"] == metadata["callback_url"])
                || h["config"]["url"].as_str().is_some_and(|url| url.ends_with(&format!("/v1/autonomous-agents/github/webhook/{id}")))
        }) {
            let hook = hook["id"].as_i64().ok_or_else(|| anyhow::anyhow!("invalid_github_hook_id"))?;
            connectors::github_delete(&token, &format!("/repos/{repo}/hooks/{hook}")).await?;
        }
        Ok::<(), anyhow::Error>(())
    }.await;
    let deleted = cleanup.is_ok();
    {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        c.execute("UPDATE autonomous_agent_connectors SET metadata_json=json_set(metadata_json,'$.remote_deleted',json(?3)),updated_at=datetime('now') WHERE org_id=?1 AND id=?2", rusqlite::params![auth.org_id,id,if deleted {"true"} else {"false"}]).map_err(|e| fail(e.into()))?;
        queries::log_audit(&c, &auth.org_id, &auth.user_id, "github.webhook_disconnected",
            "autonomous_agent_connector", Some(&id), json!({"remote_deleted":deleted})).map_err(fail)?;
    }
    Ok(Json(json!({"disconnected":true,"remote_deleted":deleted,"cleanup_pending":!deleted})))
}
#[cfg(test)]
mod tests {
    use super::*;
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
