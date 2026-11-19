//! Resolve reviewer scope from trusted, org-owned project/code links, never run input.
use anyhow::{bail, Result};
use rusqlite::{params, Connection};
use serde_json::Value;

pub fn reviewer_repositories(conn: &Connection, org: &str, config: &Value) -> Result<Vec<String>> {
    if config.get("project_id").is_some() {
        if config.get("repository").is_some() {
            bail!("ambiguous_reviewer_scope");
        }
        let id = config["project_id"]
            .as_str()
            .filter(|id| !id.trim().is_empty())
            .ok_or_else(|| anyhow::anyhow!("invalid_project_scope"))?;
        let project = super::queries::get_project_by_id(conn, org, id)?
            .filter(|project| project.archived_at.is_none())
            .ok_or_else(|| anyhow::anyhow!("invalid_project_scope"))?;
        // UNION terminates cycles; archived or foreign nodes prune their whole subtree.
        // Legacy canonical names apply only when there is no explicit project link.
        let mut stmt = conn.prepare(
            "WITH RECURSIVE family(id,name) AS (
                SELECT id,name FROM projects WHERE org_id=?1 AND id=?2 AND archived_at IS NULL
                UNION
                SELECT p.id,p.name FROM projects p JOIN family f ON p.parent_id=f.id
                WHERE p.org_id=?1 AND p.archived_at IS NULL
             )
             SELECT c.repo_url FROM code_projects c JOIN family f
               ON c.project_id=f.id OR (c.project_id IS NULL AND c.name=f.name)
             WHERE c.org_id=?1 AND c.archived_at IS NULL AND c.repo_url IS NOT NULL
             ORDER BY c.id",
        )?;
        let urls = stmt
            .query_map(params![org, project.id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut repos = Vec::new();
        for url in urls {
            // Non-GitHub indexes do not grant GitHub authority.
            if let Ok(repo) = crate::factory::workflow_publication::github_repository(
                url.trim().trim_end_matches('/'),
            ) {
                let repo = repo.to_ascii_lowercase();
                if !repos.contains(&repo) {
                    repos.push(repo);
                }
            }
        }
        if repos.is_empty() {
            bail!("project_repository_required");
        }
        return Ok(repos);
    }
    let repo = config["repository"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("valid_repository_required"))?;
    crate::automation::connectors::validate_repository(repo)?;
    Ok(vec![repo.to_ascii_lowercase()])
}

/// Repositories a Judge may target: its configured `repositories` plus the
/// GitHub repositories linked in Code to the project of each enabled project target.
pub fn judge_repositories(
    conn: &Connection,
    org: &str,
    definition_id: &str,
    config: &Value,
) -> Result<Vec<String>> {
    let mut repos: Vec<String> = config["repositories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|repo| repo.as_str().map(str::to_string))
        .collect();
    for target in super::queries::list_autonomous_agent_targets(conn, org, definition_id)? {
        if !target.enabled || target.kind != "project" {
            continue;
        }
        let scope = serde_json::json!({ "project_id": target.config["project_id"] });
        // An archived, foreign or unlinked project adds nothing (fails closed).
        for repo in reviewer_repositories(conn, org, &scope).unwrap_or_default() {
            if !repos.iter().any(|known| known.eq_ignore_ascii_case(&repo)) {
                repos.push(repo);
            }
        }
    }
    Ok(repos)
}

pub fn authorize_review(
    conn: &Connection,
    org: &str,
    config: &Value,
    repository: &str,
) -> Result<()> {
    crate::automation::connectors::validate_repository(repository)
        .map_err(|_| anyhow::anyhow!("repository_not_allowed"))?;
    if !reviewer_repositories(conn, org, config)?
        .iter()
        .any(|repo| repo.eq_ignore_ascii_case(repository))
    {
        bail!("repository_not_allowed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, migrations, queries};
    use crate::models::types::{
        CreateAutonomousAgentRequest, PutAutonomousAgentScheduleRequest,
        PutAutonomousAgentTargetRequest,
    };
    use serde_json::json;

    #[test]
    fn child_scope_prunes_archives_and_tenants_and_respects_explicit_links() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) =
            queries::bootstrap(&conn, "Acme", "acme", "scope@acme.test", "Scope").unwrap();
        let (other, _, _) =
            queries::create_org(&conn, "Other", "other", "scope@other.test", "Other").unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name,parent_id) VALUES ('root',?1,'Root',NULL),('web',?1,'web','root'),('api',?1,'api','root'),('outside',?1,'Outside',NULL),('archived',?1,'Archived','root'),('hidden',?1,'hidden','archived')",[&org.id]).unwrap();
        conn.execute(
            "UPDATE projects SET archived_at=datetime('now') WHERE id='archived'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO projects(id,org_id,name,parent_id) VALUES ('foreign',?1,'foreign','root')",
            [&other.id],
        )
        .unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'web','/web',NULL,'https://github.com/acme/web.git'),(?1,'api','/api','outside','https://github.com/acme/outside.git'),(?1,'hidden','/hidden',NULL,'https://github.com/acme/hidden.git'),(?1,'duplicate','/dup','web','git@github.com:ACME/WEB.git'),(?1,'non-github','/gitlab','web','https://gitlab.com/acme/not-authorized.git')",[&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'foreign','/foreign','root','https://github.com/other/foreign.git')",[&other.id]).unwrap();
        let config = json!({"project_id":"root"});
        assert_eq!(
            reviewer_repositories(&conn, &org.id, &config).unwrap(),
            ["acme/web"]
        );
        for repo in ["acme/outside", "acme/hidden", "other/foreign"] {
            assert!(authorize_review(&conn, &org.id, &config, repo).is_err());
        }
        // A real explicit association wins over the legacy canonical name.
        conn.execute(
            "UPDATE code_projects SET project_id=NULL WHERE org_id=?1 AND name='api'",
            [&org.id],
        )
        .unwrap();
        assert!(authorize_review(&conn, &org.id, &config, "acme/outside").is_ok());
        // Corrupt cycles terminate; traversal still cannot cross organization boundaries.
        conn.execute("UPDATE projects SET parent_id='web' WHERE id='root'", [])
            .unwrap();
        assert_eq!(
            reviewer_repositories(&conn, &org.id, &config)
                .unwrap()
                .len(),
            2
        );
        conn.execute(
            "UPDATE code_projects SET archived_at=datetime('now') WHERE org_id=?1 AND name='web'",
            [&org.id],
        )
        .unwrap();
        conn.execute(
            "UPDATE projects SET archived_at=datetime('now') WHERE id='web'",
            [],
        )
        .unwrap();
        assert!(authorize_review(&conn, &org.id, &config, "acme/web").is_err());
        conn.execute(
            "UPDATE projects SET archived_at=datetime('now') WHERE id='root'",
            [],
        )
        .unwrap();
        assert!(reviewer_repositories(&conn, &org.id, &config).is_err());
        assert!(reviewer_repositories(&conn, &other.id, &config).is_err());
    }
    #[test]
    fn project_scope_is_real_org_bound_and_used_for_routing() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "scope@acme.test", "Scope").unwrap();
        conn.execute(
            "INSERT INTO projects(id,org_id,name) VALUES ('project',?1,'Product')",
            [&org.id],
        )
        .unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name,parent_id) VALUES ('child',?1,'API','project'),('grandchild',?1,'web','child')", [&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'api','/api','child','https://github.com/acme/api.git'),(?1,'web','/web',NULL,'git@github.com:acme/web.git')", [&org.id]).unwrap();
        let config = json!({"project_id":"project"});
        assert_eq!(
            reviewer_repositories(&conn, &org.id, &config).unwrap(),
            ["acme/api", "acme/web"]
        );
        assert!(reviewer_repositories(&conn, "other-org", &config).is_err());
        assert!(reviewer_repositories(
            &conn,
            &org.id,
            &json!({"project_id":"project","repository":"acme/foreign"})
        )
        .is_err());
        assert!(authorize_review(&conn, &org.id, &config, "ACME/API").is_ok());
        assert!(authorize_review(&conn, &org.id, &config, "acme/foreign").is_err());
        let agent = queries::create_autonomous_agent_definition(
            &conn,
            &org.id,
            &user.id,
            &CreateAutonomousAgentRequest {
                name: "Project reviewer".into(),
                description: None,
                template_key: "github_pr_reviewer".into(),
                config: config.clone(),
                budgets: json!({}),
            },
        )
        .unwrap();
        let target = PutAutonomousAgentTargetRequest {
            kind: "project".into(),
            name: "Product".into(),
            config: config.clone(),
            credential_connector_id: None,
            enabled: true,
        };
        queries::put_autonomous_agent_target(&conn, &org.id, &agent.definition.id, &target)
            .unwrap();
        let stored: String = conn
            .query_row(
                "SELECT project_id FROM autonomous_agent_targets WHERE definition_id=?1",
                [&agent.definition.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored, "project");
        let mut bad_target = target;
        bad_target.config = json!({"project_id":"foreign"});
        assert!(queries::put_autonomous_agent_target(
            &conn,
            &org.id,
            &agent.definition.id,
            &bad_target
        )
        .is_err());
        queries::validate_autonomous_agent_definition(
            &conn,
            &org.id,
            &user.id,
            &agent.definition.id,
        )
        .unwrap();
        queries::set_autonomous_agent_status(&conn, &org.id, &agent.definition.id, "enabled")
            .unwrap();
        let schedule = PutAutonomousAgentScheduleRequest {
            kind: "interval".into(),
            expression: Some("15".into()),
            timezone: "UTC".into(),
            misfire_policy: "run_once".into(),
            enabled: true,
        };
        assert_eq!(
            queries::put_autonomous_agent_schedule(&conn, &org.id, &agent.definition.id, &schedule)
                .unwrap_err()
                .to_string(),
            "project_reviewer_requires_pr"
        );
        assert!(authorize_review(&conn, &org.id, &config, "https://github.com/acme/api").is_err());
        let sources = queries::list_github_reconciliation_sources(&conn).unwrap();
        assert!(sources.iter().any(|s| s.repository == "acme/api"));
        assert!(sources.iter().any(|s| s.repository == "acme/web"));
        let head = "a".repeat(40);
        for repo in ["acme/api", "acme/web"] {
            assert_eq!(
                queries::enqueue_github_webhook_agents(
                    &conn,
                    &org.id,
                    queries::GithubRunOrigin::Reconciliation("test"),
                    repo,
                    "github_pr",
                    12,
                    Some(&head),
                    "hash"
                )
                .unwrap(),
                1
            );
            assert_eq!(
                queries::enqueue_github_webhook_agents(
                    &conn,
                    &org.id,
                    queries::GithubRunOrigin::Reconciliation("test"),
                    repo,
                    "github_pr",
                    12,
                    Some(&head),
                    "hash"
                )
                .unwrap(),
                0
            );
        }
        assert_eq!(
            queries::enqueue_github_webhook_agents(
                &conn,
                &org.id,
                queries::GithubRunOrigin::Reconciliation("test"),
                "acme/foreign",
                "github_pr",
                12,
                Some(&head),
                "hash"
            )
            .unwrap(),
            0
        );
        conn.execute(
            "UPDATE code_projects SET archived_at=datetime('now') WHERE name='web'",
            [],
        )
        .unwrap();
        assert!(authorize_review(&conn, &org.id, &config, "acme/web").is_err());
        conn.execute(
            "UPDATE projects SET archived_at=datetime('now') WHERE id='project'",
            [],
        )
        .unwrap();
        assert!(reviewer_repositories(&conn, &org.id, &config).is_err());
        assert_eq!(
            reviewer_repositories(&conn, &org.id, &json!({"repository":"acme/api"})).unwrap(),
            ["acme/api"]
        );
    }

    #[test]
    fn issue_resolver_on_a_project_lists_every_linked_repository() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "resolver@acme.test", "Owner").unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name) VALUES ('kas',?1,'Kas')", [&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'ui','/ui','kas','https://github.com/acme/ui.git'),(?1,'api','/api','kas','https://github.com/acme/api.git')", [&org.id]).unwrap();
        let agent = queries::create_autonomous_agent_definition(&conn, &org.id, &user.id, &CreateAutonomousAgentRequest {
            name: "Resolver".into(),
            description: None,
            template_key: "github_issue_resolver".into(),
            config: json!({"repository":"acme/ui","github_auth":"server_gh_cli"}),
            budgets: json!({"wall_time_seconds": 300}),
        })
        .unwrap();
        let id = agent.definition.id;
        queries::put_autonomous_agent_target(&conn, &org.id, &id, &PutAutonomousAgentTargetRequest {
            kind: "project".into(),
            name: "Kas".into(),
            config: json!({"project_id":"kas"}),
            credential_connector_id: None,
            enabled: true,
        })
        .unwrap();
        queries::validate_autonomous_agent_definition(&conn, &org.id, &user.id, &id).unwrap();
        queries::set_autonomous_agent_status(&conn, &org.id, &id, "enabled").unwrap();
        queries::save_autonomous_runtime_health(&conn, &crate::automation::runtime::RuntimeHealth {
            status: "ready".into(),
            reason_code: None,
            claude_version: Some("test".into()),
            checked_at: None,
            last_success_at: None,
            last_failure_at: None,
        })
        .unwrap();
        queries::enqueue_autonomous_agent_run(&conn, &org.id, &id, "manual", "resolver-1", None, None).unwrap();
        let claim = queries::claim_next_autonomous_agent_run(&conn, "worker", 60).unwrap().unwrap();
        assert_eq!(claim.config["repository"], "acme/ui");
        assert_eq!(claim.config["project_repositories"], json!(["acme/ui", "acme/api"]));
    }

    #[test]
    fn product_strategist_on_a_project_studies_its_linked_repositories() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "strategist@acme.test", "Owner").unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name) VALUES ('kas',?1,'Kas')", [&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'ui','/ui','kas','https://github.com/acme/ui.git'),(?1,'api','/api','kas','https://github.com/acme/api.git')", [&org.id]).unwrap();
        let agent = queries::create_autonomous_agent_definition(
            &conn,
            &org.id,
            &user.id,
            &CreateAutonomousAgentRequest {
                name: "Strategist".into(),
                description: None,
                template_key: "product_strategist".into(),
                config: json!({"outputs":["nexusmind"]}),
                budgets: json!({"wall_time_seconds": 300}),
            },
        )
        .unwrap();
        let id = agent.definition.id;
        queries::put_autonomous_agent_target(&conn, &org.id, &id, &PutAutonomousAgentTargetRequest {
            kind: "project".into(),
            name: "Kas".into(),
            config: json!({"project_id":"kas"}),
            credential_connector_id: None,
            enabled: true,
        })
        .unwrap();
        let valid = queries::validate_autonomous_agent_definition(&conn, &org.id, &user.id, &id).unwrap().unwrap();
        assert_eq!(valid.revision.validation_status, "valid");
        queries::set_autonomous_agent_status(&conn, &org.id, &id, "enabled").unwrap();
        queries::save_autonomous_runtime_health(&conn, &crate::automation::runtime::RuntimeHealth {
            status: "ready".into(),
            reason_code: None,
            claude_version: Some("test".into()),
            checked_at: None,
            last_success_at: None,
            last_failure_at: None,
        })
        .unwrap();
        queries::enqueue_autonomous_agent_run(&conn, &org.id, &id, "manual", "strategist-1", None, None).unwrap();
        let claim = queries::claim_next_autonomous_agent_run(&conn, "worker", 60).unwrap().unwrap();
        assert_eq!(claim.config["repository"], "acme/ui");
        assert_eq!(claim.config["project_repositories"], json!(["acme/ui", "acme/api"]));
        assert_eq!(claim.config["context_repos"], json!(["acme/api"]));
    }

    #[test]
    fn judge_repositories_add_project_links_within_the_org_only() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, user, _) =
            queries::bootstrap(&conn, "Acme", "acme", "judge@acme.test", "Judge").unwrap();
        let (other, _, _) =
            queries::create_org(&conn, "Other", "other", "judge@other.test", "Other").unwrap();
        conn.execute("INSERT INTO projects(id,org_id,name) VALUES ('kas',?1,'Kas')", [&org.id]).unwrap();
        conn.execute("INSERT INTO code_projects(org_id,name,root_path,project_id,repo_url) VALUES (?1,'ui','/ui','kas','https://github.com/acme/ui.git'),(?1,'api','/api','kas','https://github.com/acme/api.git')", [&org.id]).unwrap();
        let config = json!({"repositories":["acme/ui"]});
        let agent = queries::create_autonomous_agent_definition(
            &conn,
            &org.id,
            &user.id,
            &CreateAutonomousAgentRequest {
                name: "Judge".into(),
                description: None,
                template_key: "judge".into(),
                config: config.clone(),
                budgets: json!({}),
            },
        )
        .unwrap();
        let id = agent.definition.id;
        assert_eq!(judge_repositories(&conn, &org.id, &id, &config).unwrap(), ["acme/ui"]);
        let mut target = PutAutonomousAgentTargetRequest {
            kind: "project".into(),
            name: "Kas".into(),
            config: json!({"project_id":"kas"}),
            credential_connector_id: None,
            enabled: true,
        };
        queries::put_autonomous_agent_target(&conn, &org.id, &id, &target).unwrap();
        assert_eq!(
            judge_repositories(&conn, &org.id, &id, &config).unwrap(),
            ["acme/ui", "acme/api"]
        );
        // Another org never sees this agent's project links.
        assert_eq!(judge_repositories(&conn, &other.id, &id, &config).unwrap(), ["acme/ui"]);
        target.enabled = false;
        queries::put_autonomous_agent_target(&conn, &org.id, &id, &target).unwrap();
        assert_eq!(judge_repositories(&conn, &org.id, &id, &config).unwrap(), ["acme/ui"]);
    }
}
