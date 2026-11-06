//! Shared workflow publication connectors, without changing the gh reviewer identity.
use anyhow::{bail, Result};
use rusqlite::Connection;
use serde_json::Value;
use crate::{db::queries, models::types::AutonomousAgentConnector};

pub fn connection(conn: &Connection, org: &str, template: &str, config: &Value) -> Result<Option<AutonomousAgentConnector>> {
    let Some(value) = config.get("github_publication_connector_id") else { return Ok(None); };
    let id = value.as_str().filter(|id| !id.is_empty()).ok_or_else(|| anyhow::anyhow!("github_publication_connection_invalid"))?;
    if !matches!(template, "github_issue_resolver" | "qa") { bail!("github_publication_connection_invalid"); }
    let connector = queries::list_autonomous_agent_connectors(conn, org)?.into_iter().find(|c| c.id == id)
        .ok_or_else(|| anyhow::anyhow!("github_publication_connection_invalid"))?;
    validate(&connector, config)?;
    Ok(Some(connector))
}

pub fn validate(connector: &AutonomousAgentConnector, config: &Value) -> Result<()> {
    if connector.kind != "target_secret" || connector.health != "ready" || !connector.secret_configured
        || connector.metadata["purpose"] != "factory_workflow_github"
        || !connector.scopes.iter().any(|s| s == "github:write") {
        bail!("github_publication_connection_invalid");
    }
    let mut repos = Vec::new();
    for value in [config.get("repository"), config.pointer("/trigger/repository")].into_iter().flatten() {
        let repo = value.as_str().ok_or_else(|| anyhow::anyhow!("github_publication_connection_invalid"))?;
        repos.push(repo);
    }
    if repos.is_empty() { bail!("github_publication_connection_invalid"); }
    if let Some(context) = config.get("context_repos") {
        for value in context.as_array().ok_or_else(|| anyhow::anyhow!("github_publication_connection_invalid"))? {
            repos.push(value.as_str().ok_or_else(|| anyhow::anyhow!("github_publication_connection_invalid"))?);
        }
    }
    for repo in repos {
        super::connectors::validate_repository(repo)?;
        if !connector.metadata["repositories"].as_array().is_some_and(|items| items.iter().any(|value| value.as_str().is_some_and(|allowed| allowed.eq_ignore_ascii_case(repo)))) {
            bail!("github_publication_repository_scope");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn rejects_review_identity_override_and_limits_every_repository() {
        let connector: AutonomousAgentConnector = serde_json::from_value(json!({
            "id":"publisher","kind":"target_secret","name":"PR bot","metadata":{"purpose":"factory_workflow_github","repositories":["acme/api","acme/context"]},
            "scopes":["github:write"],"health":"ready","revocation_generation":0,"secret_configured":true,"created_at":"","updated_at":""
        })).unwrap();
        assert!(validate(&connector,&json!({"repository":"ACME/API","context_repos":["acme/context"]})).is_ok());
        assert!(validate(&connector,&json!({"repository":"acme/foreign"})).is_err());
        assert!(validate(&connector,&json!({"repository":"acme/api","trigger":{"repository":"acme/foreign"}})).is_err());
        assert!(validate(&connector,&json!({"repository":"acme/api","context_repos":["acme/foreign"]})).is_err());
        let mut revoked=connector.clone();revoked.health="revoked".into();
        assert!(validate(&revoked,&json!({"repository":"acme/api"})).is_err());
        let conn=crate::db::connection::connect(":memory:").unwrap();
        crate::db::migrations::run_all(&conn).unwrap();
        let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "publisher@test.com", "Publisher").unwrap();
        conn.execute("INSERT INTO autonomous_agent_connectors(id,org_id,kind,name,secret_ciphertext,metadata_json,scopes_json,health,created_by) VALUES ('publisher',?1,'target_secret','PR bot','test-only-placeholder',?2,?3,'ready',?4)",rusqlite::params![org.id,connector.metadata.to_string(),serde_json::to_string(&connector.scopes).unwrap(),user.id]).unwrap();
        assert!(connection(&conn,&org.id,"github_issue_resolver",&json!({"repository":"acme/api","github_publication_connector_id":"publisher"})).unwrap().is_some());
        assert!(connection(&conn,"org","github_pr_reviewer",&json!({"github_publication_connector_id":"publisher"})).is_err());
        assert!(connection(&conn,"other-org","github_issue_resolver",&json!({"github_publication_connector_id":"publisher"})).is_err());
        assert!(connection(&conn,"org","github_pr_reviewer",&json!({"repository":"acme/api"})).unwrap().is_none());
    }
}
