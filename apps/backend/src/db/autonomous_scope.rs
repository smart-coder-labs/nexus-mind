//! Resolve reviewer scope from trusted, org-owned project/code links, never run input.
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde_json::Value;

pub fn reviewer_repositories(conn: &Connection, org: &str, config: &Value) -> Result<Vec<String>> {
    if config.get("project_id").is_some() {
        if config.get("repository").is_some() {
            bail!("ambiguous_reviewer_scope");
        }
        let id = config["project_id"].as_str().filter(|id| !id.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("invalid_project_scope"))?;
        let project = super::queries::get_project_by_id(conn, org, id)?
            .filter(|project| project.archived_at.is_none())
            .ok_or_else(|| anyhow::anyhow!("invalid_project_scope"))?;
        let mut stmt = conn.prepare("SELECT repo_url FROM code_projects WHERE org_id=?1 AND project_id=?2 AND archived_at IS NULL AND repo_url IS NOT NULL ORDER BY id")?;
        let urls = stmt.query_map(params![org, project.id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut repos = Vec::new();
        for url in urls {
            // Non-GitHub indexes do not grant GitHub authority.
            if let Ok(repo) = crate::factory::workflow_publication::github_repository(url.trim().trim_end_matches('/')) {
                let repo = repo.to_ascii_lowercase();
                if !repos.contains(&repo) { repos.push(repo); }
            }
        }
        if repos.is_empty() { bail!("project_repository_required"); }
        return Ok(repos);
    }
    let repo = config["repository"].as_str().ok_or_else(|| anyhow::anyhow!("valid_repository_required"))?;
    crate::automation::connectors::validate_repository(repo)?;
    Ok(vec![repo.to_ascii_lowercase()])
}

pub fn authorize_review(conn: &Connection, org: &str, config: &Value, repository: &str) -> Result<()> {
    crate::automation::connectors::validate_repository(repository)
        .map_err(|_| anyhow::anyhow!("repository_not_allowed"))?;
    if !reviewer_repositories(conn, org, config)?.iter().any(|repo| repo.eq_ignore_ascii_case(repository)) {
        bail!("repository_not_allowed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations, queries};
    use crate::models::types::{CreateAutonomousAgentRequest, PutAutonomousAgentTargetRequest, PutAutonomousAgentScheduleRequest};
    use serde_json::json;

    #[test]
    fn project_scope_is_real_org_bound_and_used_for_routing() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) = queries::bootstrap(&conn, "Acme", "acme", "scope@acme.test", "Scope").unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name) VALUES ('project',?1,'Product')", [&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'api','/api','project','https://github.com/acme/api.git'),(?1,'web','/web','project','git@github.com:acme/web.git')", [&org.id]).unwrap();
        let config = json!({"project_id":"project"});
        assert_eq!(reviewer_repositories(&conn, &org.id, &config).unwrap(), ["acme/api", "acme/web"]);
        assert!(reviewer_repositories(&conn, "other-org", &config).is_err());
        assert!(reviewer_repositories(&conn, &org.id, &json!({"project_id":"project","repository":"acme/foreign"})).is_err());
        assert!(authorize_review(&conn, &org.id, &config, "ACME/API").is_ok());
        assert!(authorize_review(&conn, &org.id, &config, "acme/foreign").is_err());
        let agent = queries::create_autonomous_agent_definition(&conn, &org.id, &user.id, &CreateAutonomousAgentRequest {
            name: "Project reviewer".into(), description: None, template_key: "github_pr_reviewer".into(), config: config.clone(), budgets: json!({}),
        }).unwrap();
        let target = PutAutonomousAgentTargetRequest {kind:"project".into(),name:"Product".into(),config:config.clone(),credential_connector_id:None,enabled:true};
        queries::put_autonomous_agent_target(&conn, &org.id, &agent.definition.id, &target).unwrap();
        let stored: String = conn.query_row("SELECT project_id FROM autonomous_agent_targets WHERE definition_id=?1", [&agent.definition.id], |r|r.get(0)).unwrap();
        assert_eq!(stored,"project");
        let mut bad_target = target;
        bad_target.config = json!({"project_id":"foreign"});
        assert!(queries::put_autonomous_agent_target(&conn, &org.id, &agent.definition.id, &bad_target).is_err());
        queries::validate_autonomous_agent_definition(&conn, &org.id, &user.id, &agent.definition.id).unwrap();
        queries::set_autonomous_agent_status(&conn, &org.id, &agent.definition.id, "enabled").unwrap();
        let schedule = PutAutonomousAgentScheduleRequest {kind:"interval".into(),expression:Some("15".into()),timezone:"UTC".into(),misfire_policy:"run_once".into(),enabled:true};
        assert_eq!(queries::put_autonomous_agent_schedule(&conn,&org.id,&agent.definition.id,&schedule).unwrap_err().to_string(),"project_reviewer_requires_pr");
        assert!(authorize_review(&conn,&org.id,&config,"https://github.com/acme/api").is_err());
        let sources=queries::list_github_reconciliation_sources(&conn).unwrap();
        assert!(sources.iter().any(|s|s.repository=="acme/api"));
        assert!(sources.iter().any(|s|s.repository=="acme/web"));
        let head="a".repeat(40);
        for repo in ["acme/api","acme/web"] {
            assert_eq!(queries::enqueue_github_webhook_agents(&conn,&org.id,queries::GithubRunOrigin::Reconciliation("test"),repo,"github_pr",12,Some(&head),"hash").unwrap(),1);
            assert_eq!(queries::enqueue_github_webhook_agents(&conn,&org.id,queries::GithubRunOrigin::Reconciliation("test"),repo,"github_pr",12,Some(&head),"hash").unwrap(),0);
        }
        assert_eq!(queries::enqueue_github_webhook_agents(&conn,&org.id,queries::GithubRunOrigin::Reconciliation("test"),"acme/foreign","github_pr",12,Some(&head),"hash").unwrap(),0);
        conn.execute("UPDATE code_projects SET archived_at=datetime('now') WHERE name='web'", []).unwrap();
        assert!(authorize_review(&conn,&org.id,&config,"acme/web").is_err());
        conn.execute("UPDATE projects SET archived_at=datetime('now') WHERE id='project'", []).unwrap();
        assert!(reviewer_repositories(&conn,&org.id,&config).is_err());
        assert_eq!(reviewer_repositories(&conn,&org.id,&json!({"repository":"acme/api"})).unwrap(),["acme/api"]);
    }
}
