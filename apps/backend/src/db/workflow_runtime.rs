//! Versioned, non-secret workspace runtime settings. Local runner policy remains authoritative.
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub agents: Vec<Agent>,
    #[serde(default)]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub services: Vec<Service>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub role: String,
    pub image: String,
    pub model: String,
    pub model_connection: String,
    pub network: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    pub name: String,
    pub image: String,
    pub cwd: String,
    pub command: Vec<String>,
    #[serde(default)]
    pub environment_refs: Vec<String>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Service {
    pub name: String,
    pub image: String,
    pub cwd: String,
    pub command: Vec<String>,
    pub healthcheck: Vec<String>,
    #[serde(default)]
    pub environment_refs: Vec<String>,
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    #[serde(default)]
    pub remote_enabled: bool,
    #[serde(default)]
    pub applied_revision: u32,
    #[serde(default)]
    pub images: Vec<String>,
    #[serde(default)]
    pub networks: Vec<String>,
    #[serde(default)]
    pub model_connections: Vec<String>,
    #[serde(default)]
    pub environments: Vec<String>,
    #[serde(default)]
    pub error: Option<String>,
}
const ROLES: [&str; 4] = ["investigation", "test_author", "implementation", "reviewer"];
const CHECKS: [&str; 5] = ["tests", "build", "functional", "design", "security"];
fn secret_like(s: &str) -> bool {
    let value = s.to_ascii_lowercase();
    [
        "ghp_",
        "github_pat_",
        "sk-ant-",
        "sk-proj-",
        "bearer ",
        "authorization:",
        "api_key=",
        "password=",
        "token=",
        "-----begin",
    ]
    .iter()
    .any(|p| value.contains(p))
}
fn name(s: &str) -> bool {
    !s.is_empty()
        && !secret_like(s)
        && s.len() <= 80
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
}
fn image(s: &str) -> bool {
    s.rsplit_once("sha256:").is_some_and(|(prefix, d)| {
        !secret_like(s)
            && !prefix.contains("://")
            && (prefix.is_empty()
                || prefix.strip_suffix('@').is_some_and(|p| {
                    !p.is_empty()
                        && p.bytes()
                            .all(|b| b.is_ascii_alphanumeric() || b"-._/:".contains(&b))
                }))
            && d.len() == 64
            && d.bytes().all(|b| b.is_ascii_hexdigit())
    })
}
fn argv(args: &[String]) -> bool {
    !args.is_empty()
        && args.len() <= 64
        && args
            .iter()
            .all(|s| !s.is_empty() && s.len() <= 4096 && !s.contains('\0') && !secret_like(s))
}
fn refs(names: &[String]) -> bool {
    names.len() <= 16 && names.iter().all(|s| name(s))
}
pub fn validate(config: &RuntimeConfig, repos: &[String]) -> Result<()> {
    if config.agents.len() > 4 || config.checks.len() > 64 || config.services.len() > 16 {
        bail!("runtime_limits_exceeded")
    }
    let mut roles = std::collections::BTreeSet::new();
    for a in &config.agents {
        if !ROLES.contains(&a.role.as_str())
            || !roles.insert(&a.role)
            || !image(&a.image)
            || a.model.is_empty()
            || secret_like(&a.model)
            || a.model.len() > 120
            || a.model.chars().any(char::is_control)
            || !name(&a.model_connection)
            || !name(&a.network)
            || ["host", "bridge", "none", "services"].contains(&a.network.as_str())
        {
            bail!("invalid_runtime_agent")
        }
    }
    let mut names = std::collections::BTreeSet::new();
    for c in &config.checks {
        if ![
            "tests",
            "build",
            "lint",
            "typecheck",
            "format",
            "functional",
            "design",
            "security",
        ]
        .contains(&c.name.as_str())
            || !names.insert((&c.name, &c.cwd))
            || !image(&c.image)
            || !repos.contains(&c.cwd)
            || !argv(&c.command)
            || !refs(&c.environment_refs)
        {
            bail!("invalid_runtime_check")
        }
    }
    let mut services = std::collections::BTreeSet::new();
    for s in &config.services {
        if !name(&s.name)
            || !services.insert(&s.name)
            || !image(&s.image)
            || !repos.contains(&s.cwd)
            || !argv(&s.command)
            || !argv(&s.healthcheck)
            || !refs(&s.environment_refs)
        {
            bail!("invalid_runtime_service")
        }
    }
    Ok(())
}
pub fn migrate(c: &Connection) -> Result<()> {
    if c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? >= 96 {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? < 96 {
        tx.execute_batch("CREATE TABLE factory_workflow_runtime(workspace_id TEXT PRIMARY KEY REFERENCES factory_workspaces(id) ON DELETE CASCADE,revision INTEGER NOT NULL DEFAULT 0,config_json TEXT NOT NULL DEFAULT '{}',report_json TEXT NOT NULL DEFAULT '{}',reported_at TEXT,updated_by TEXT REFERENCES users(id),updated_at TEXT NOT NULL DEFAULT(datetime('now'))); PRAGMA user_version=96;")?;
    }
    tx.commit()?;
    Ok(())
}
pub fn metadata(c: &Connection, workspace: &str) -> Result<Value> {
    let row=c.query_row("SELECT revision,config_json,report_json,reported_at FROM factory_workflow_runtime WHERE workspace_id=?1",[workspace],|r|Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?))).optional()?;
    let (revision, config, report, reported_at) =
        row.unwrap_or((0, "{}".into(), "{}".into(), None));
    let config: RuntimeConfig = serde_json::from_str(&config)?;
    let report: Report = serde_json::from_str(&report)?;
    let mut missing = Vec::new();
    if !report.remote_enabled {
        missing.push("remote_setup_disabled".to_string())
    }
    for role in ROLES {
        if !config.agents.iter().any(|a| a.role == role) {
            missing.push(format!("agent:{role}"))
        }
    }
    for check in CHECKS {
        if !config.checks.iter().any(|a| a.name == check) {
            missing.push(format!("check:{check}"))
        }
    }
    if config.services.is_empty() {
        missing.push("services".into())
    }
    for a in &config.agents {
        if !report.images.contains(&a.image) {
            missing.push(format!("image:{}", a.image))
        }
        if !report.model_connections.contains(&a.model_connection) {
            missing.push(format!("model_connection:{}", a.model_connection))
        }
        if !report.networks.contains(&a.network) {
            missing.push(format!("network:{}", a.network))
        }
    }
    for (img, env) in config
        .checks
        .iter()
        .map(|c| (&c.image, &c.environment_refs))
        .chain(
            config
                .services
                .iter()
                .map(|s| (&s.image, &s.environment_refs)),
        )
    {
        if !report.images.contains(img) {
            missing.push(format!("image:{img}"))
        }
        for reference in env {
            if !report.environments.contains(reference) {
                missing.push(format!("environment:{reference}"))
            }
        }
    }
    if revision == 0 || revision != report.applied_revision {
        missing.push("runner_apply".into())
    }
    if report.error.is_some() {
        missing.push("runner_configuration_error".into())
    }
    missing.sort();
    missing.dedup();
    Ok(
        json!({"revision":revision,"config":config,"report":report,"reported_at":reported_at,"ready":missing.is_empty(),"missing":missing}),
    )
}
pub fn save(
    c: &Connection,
    org: &str,
    user: &str,
    workspace: &str,
    expected: u32,
    config: &RuntimeConfig,
) -> Result<Value> {
    let w = super::workflows::workspace_by_id(c, org, workspace)?;
    let repos = w["repositories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["path"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();
    validate(config, &repos)?;
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let version = metadata(&tx, workspace)?["revision"].as_u64().unwrap_or(0) as u32;
    if version != expected {
        bail!("runtime_revision_conflict")
    }
    tx.execute("INSERT INTO factory_workflow_runtime(workspace_id,revision,config_json,updated_by) VALUES (?1,1,?2,?3) ON CONFLICT(workspace_id) DO UPDATE SET revision=revision+1,config_json=excluded.config_json,updated_by=excluded.updated_by,updated_at=datetime('now')",params![workspace,serde_json::to_string(config)?,user])?;
    super::queries::append_audit_chained(
        &tx,
        org,
        user,
        "factory.runtime_configured",
        "factory_workspace",
        Some(workspace),
        json!({"revision":version+1}),
        None,
    )?;
    tx.commit()?;
    metadata(c, workspace)
}
pub fn report(
    c: &Connection,
    org: &str,
    runner: &str,
    workspace: &str,
    report: &Report,
) -> Result<Value> {
    let w = super::workflows::workspace_by_id(c, org, workspace)?;
    if w["runner_id"] != runner {
        bail!("not_found")
    }
    if report.images.len() > 100
        || report.images.iter().any(|s| !image(s))
        || [
            &report.networks,
            &report.model_connections,
            &report.environments,
        ]
        .iter()
        .any(|v| v.len() > 100 || v.iter().any(|s| !name(s)))
        || report.error.as_ref().is_some_and(|s| {
            ![
                "runtime_image_not_allowed",
                "runtime_network_not_allowed",
                "runtime_model_connection_missing",
                "runtime_environment_missing",
                "runtime_invalid",
            ]
            .contains(&s.as_str())
        })
    {
        bail!("invalid_runtime_report")
    }
    let desired = metadata(c, workspace)?["revision"].as_u64().unwrap_or(0);
    if u64::from(report.applied_revision) > desired {
        bail!("runtime_revision_conflict")
    }
    c.execute("INSERT INTO factory_workflow_runtime(workspace_id,report_json,reported_at) VALUES (?1,?2,datetime('now')) ON CONFLICT(workspace_id) DO UPDATE SET report_json=excluded.report_json,reported_at=excluded.reported_at",params![workspace,serde_json::to_string(report)?])?;
    metadata(c, workspace)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn revisions_reports_and_snapshots_are_scoped_and_fenced() {
        let c = super::super::connection::connect(":memory:").unwrap();
        super::super::migrations::run(&c).unwrap();
        let (org, user, _) =
            super::super::queries::bootstrap(&c, "Acme", "acme", "a@test.com", "Owner").unwrap();
        let runner = super::super::workflows::register_runner(&c, &org.id, "Runner").unwrap();
        let rid = runner["id"].as_str().unwrap();
        let w = super::super::workflows::create_workspace(&c, &org.id, rid, "App", "/work/app")
            .unwrap();
        let wid = w["id"].as_str().unwrap();
        let repo = crate::factory::workflows::Repository {
            path: "web".into(),
            remote: "https://github.com/acme/web.git".into(),
            head_sha: "a".repeat(40),
            writable: false,
        };
        super::super::workflows::report_inventory(&c, &org.id, rid, wid, &[repo.clone()]).unwrap();
        super::super::workflows::confirm_inventory(&c, &org.id, wid, 1, &[repo]).unwrap();
        let img = format!("sha256:{}", "b".repeat(64));
        let config = RuntimeConfig {
            agents: ROLES
                .iter()
                .map(|r| Agent {
                    role: (*r).into(),
                    image: img.clone(),
                    model: "sonnet".into(),
                    model_connection: "proxy".into(),
                    network: "model-net".into(),
                })
                .collect(),
            checks: CHECKS
                .iter()
                .map(|r| Check {
                    name: (*r).into(),
                    image: img.clone(),
                    cwd: "web".into(),
                    command: vec!["npm".into(), "test".into()],
                    environment_refs: vec![],
                })
                .collect(),
            services: vec![Service {
                name: "web".into(),
                image: img.clone(),
                cwd: "web".into(),
                command: vec!["npm".into(), "start".into()],
                healthcheck: vec!["true".into()],
                environment_refs: vec![],
            }],
        };
        assert_eq!(
            save(&c, &org.id, &user.id, wid, 0, &config).unwrap()["revision"],
            1
        );
        assert!(save(&c, &org.id, &user.id, wid, 0, &config).is_err());
        assert!(save(&c, "another-org", &user.id, wid, 1, &config).is_err());
        assert_eq!(metadata(&c, wid).unwrap()["ready"], false);
        let r = Report {
            remote_enabled: true,
            applied_revision: 1,
            images: vec![img],
            networks: vec!["model-net".into()],
            model_connections: vec!["proxy".into()],
            environments: vec![],
            error: None,
        };
        assert!(report(&c, &org.id, "another-runner", wid, &r).is_err());
        assert_eq!(report(&c, &org.id, rid, wid, &r).unwrap()["ready"], true);
        let definition:crate::factory::workflows::Definition=serde_json::from_value(json!({"name":"Prepare","workspace_id":wid,"steps":[{"id":"prepare","name":"Prepare","kind":"prepare","adapter":"prepare-workspace"}]})).unwrap();
        let d =
            super::super::workflows::create_definition(&c, &org.id, &user.id, &definition).unwrap();
        let run = super::super::workflows::start_run(
            &c,
            &org.id,
            d["id"].as_str().unwrap(),
            "runtime:1",
            &json!({"title":"Test"}),
        )
        .unwrap();
        save(&c, &org.id, &user.id, wid, 1, &RuntimeConfig::default()).unwrap();
        assert_eq!(metadata(&c, wid).unwrap()["ready"], false);
        let stored =
            super::super::workflows::run_detail(&c, &org.id, run["id"].as_str().unwrap()).unwrap();
        assert_eq!(stored["snapshot"]["workspace"]["runtime"]["revision"], 1);
        assert_eq!(
            stored["snapshot"]["workspace"]["runtime"]["config"]["agents"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
    #[test]
    fn settings_cannot_carry_credentials_host_paths_or_mutable_images() {
        assert!(serde_json::from_value::<RuntimeConfig>(json!({"api_key":"secret"})).is_err());
        let mut c = RuntimeConfig::default();
        c.checks.push(Check {
            name: "tests".into(),
            image: "node:latest".into(),
            cwd: "web".into(),
            command: vec!["npm".into(), "test".into()],
            environment_refs: vec![],
        });
        assert!(validate(&c, &["web".into()]).is_err());
        c.checks[0].image = format!("sha256:{}", "a".repeat(64));
        c.checks[0].cwd = "/etc".into();
        assert!(validate(&c, &["web".into()]).is_err());
        c.checks[0].cwd = "web".into();
        assert!(validate(&c, &["web".into()]).is_ok());
    }
}
