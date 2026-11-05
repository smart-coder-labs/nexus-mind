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
    /// Required only when a project reviewer has multiple linked repositories.
    #[serde(default)]
    pub repository: Option<String>,
    /// Optional setup-only credential. Never persisted or used by reviewers.
    #[serde(default)]
    pub administration_token: Option<String>,
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
        let allowed = crate::db::autonomous_scope::reviewer_repositories(&c, &auth.org_id, &d.revision.config).map_err(fail)?;
        let repository = b.repository.as_deref().map(str::trim).filter(|repo| !repo.is_empty())
            .or_else(|| (allowed.len() == 1).then(|| allowed[0].as_str()))
            .ok_or_else(|| fail(anyhow::anyhow!("Select a linked repository for this project reviewer")))?;
        crate::db::autonomous_scope::authorize_review(&c, &auth.org_id, &d.revision.config, repository).map_err(fail)?;
        repository.to_ascii_lowercase()
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
    let administration_token = b
        .administration_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if administration_token.is_some_and(|s| s.len() > 4096) {
        return Err(fail(anyhow::anyhow!("invalid_github_administration_token")));
    }
    // An administrator may have configured the hook without granting the bot
    // repository-administration rights. Reuse only that exact stored callback.
    if administration_token.is_none() {
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
            return Ok(Json(
                json!({"id":existing.id,"metadata":existing.metadata,"login":login,"status":"configured"}),
            ));
        }
    }
    let setup_token = administration_token.unwrap_or(&token);
    let prefix = format!("/repos/{repository}/hooks");
    // Preflight before persisting any new connector: failed authorization must
    // not leave a misleading ready/orphan registration behind.
    let hooks = connectors::github_get(setup_token, &format!("{prefix}?per_page=100"))
        .await
        .map_err(github_setup_failure)?;
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
        .as_array()
        .into_iter()
        .flatten()
        .find(|h| h["config"]["url"] == url)
        .and_then(|h| h["id"].as_i64());
    let payload = json!({"name":"web","active":true,"events":["pull_request"],"config":{"url":url,"content_type":"json","secret":secret,"insecure_ssl":"0"}});
    let hook = if let Some(hook) = existing {
        connectors::github_patch(setup_token, &format!("{prefix}/{hook}"), payload).await
    } else {
        connectors::github_post(setup_token, &prefix, payload).await
    }
    .map_err(github_setup_failure)?;
    let registration_method = if administration_token.is_some() {
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
