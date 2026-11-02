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
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Register {
    pub definition_id: String,
    pub public_base_url: String,
}
pub async fn list(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
) -> Response {
    let hooks = {
        let h = store.conn();
        let c = h.lock().map_err(|_| locked())?;
        require_explicit_permission(&c, &auth, None, "autonomous_agent:read")?;
        queries::list_autonomous_agent_connectors(&c, &auth.org_id)
            .map_err(fail)?
            .into_iter()
            .filter(|c| c.metadata["purpose"] == "github_cli_webhook")
            .map(|c| json!({"id":c.id,"health":c.health,"metadata":c.metadata}))
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
    let token = worker::server_github_token()
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_unavailable")))?;
    let login = worker::github_identity_for_token(&token)
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_cli_identity_unavailable")))?;
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
    let hooks = connectors::github_get(&token, &format!("{prefix}?per_page=100"))
        .await
        .map_err(|_| fail(anyhow::anyhow!("github_webhook_permissions_required")))?;
    let existing = hooks
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["config"]["url"] == url)
        .and_then(|h| h["id"].as_i64());
    let payload = json!({"name":"web","active":true,"events":["pull_request"],"config":{"url":url,"content_type":"json","secret":secret,"insecure_ssl":"0"}});
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
    let mut deleted = false;
    if let (Some(repo), Some(hook)) = (
        metadata["repository"].as_str(),
        metadata["hook_id"].as_i64(),
    ) {
        if let Ok(token) = worker::server_github_token().await {
            deleted = connectors::github_delete(&token, &format!("/repos/{repo}/hooks/{hook}"))
                .await
                .is_ok();
        }
    }
    Ok(Json(json!({"disconnected":true,"remote_deleted":deleted})))
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
