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
fn default_harness() -> String {
    "claude".into()
}
#[derive(Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AgentBinding {
    pub id: String,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub config_hash: String,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Agent {
    pub role: String,
    #[serde(default = "default_harness")]
    pub harness: String,
    #[serde(default)]
    pub binding: Option<AgentBinding>,
    #[serde(default)]
    pub inherit_model: bool,
    #[serde(default)]
    pub instructions: String,
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
    pub harnesses: Vec<String>,
    #[serde(default)]
    pub managed_images: std::collections::BTreeMap<String, String>,
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
        "nmr_",
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
    if config.agents.len() > 4 || config.checks.len() > 256 || config.services.len() > 16 {
        bail!("runtime_limits_exceeded")
    }
    let mut roles = std::collections::BTreeSet::new();
    for a in &config.agents {
        if !ROLES.contains(&a.role.as_str())
            || !roles.insert(&a.role)
            || !name(&a.harness)
            || a.instructions.len() > 16000
            || secret_like(&a.instructions)
            || (!a.image.is_empty() && !image(&a.image))
            || (!a.model.is_empty()
                && (!a
                    .model
                    .as_bytes()
                    .first()
                    .is_some_and(u8::is_ascii_alphanumeric)
                    || !a
                        .model
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"-_.:/[]@+".contains(&b))))
            || secret_like(&a.model)
            || a.model.len() > 120
            || a.model.chars().any(char::is_control)
            || (!a.model_connection.is_empty() && !name(&a.model_connection))
            || (!a.network.is_empty() && !name(&a.network))
            || ["host", "bridge", "services"].contains(&a.network.as_str())
            || (a.network == "none" && (a.model_connection != "backend" || a.harness != "pi"))
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
            || !names.insert((&c.name, &c.cwd, &c.command))
            || !image(&c.image)
            || !repos.contains(&c.cwd)
            || !argv(&c.command)
            || !refs(&c.environment_refs)
        {
            if !image(&c.image) {
                bail!("runtime_check_image_invalid")
            }
            if !repos.contains(&c.cwd) {
                bail!("runtime_check_repository_invalid")
            }
            if !argv(&c.command) {
                bail!("runtime_check_command_invalid")
            }
            if config
                .checks
                .iter()
                .filter(|other| {
                    other.name == c.name && other.cwd == c.cwd && other.command == c.command
                })
                .count()
                > 1
            {
                bail!("runtime_check_duplicate")
            }
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
    if c.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? >= 97 {
        return Ok(());
    }
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? < 96 {
        tx.execute_batch("CREATE TABLE factory_workflow_runtime(workspace_id TEXT PRIMARY KEY REFERENCES factory_workspaces(id) ON DELETE CASCADE,revision INTEGER NOT NULL DEFAULT 0,config_json TEXT NOT NULL DEFAULT '{}',report_json TEXT NOT NULL DEFAULT '{}',reported_at TEXT,updated_by TEXT REFERENCES users(id),updated_at TEXT NOT NULL DEFAULT(datetime('now'))); PRAGMA user_version=96;")?;
    }
    if tx.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))? < 97 {
        tx.execute_batch("ALTER TABLE factory_workflow_intakes RENAME TO factory_workflow_intakes_v96; CREATE TABLE factory_workflow_intakes (id TEXT PRIMARY KEY,org_id TEXT NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,definition_id TEXT NOT NULL REFERENCES factory_workflow_definitions(id) ON DELETE CASCADE,kind TEXT NOT NULL CHECK(kind IN ('github','linear','slack','sentry')),connector_id TEXT NOT NULL,config_json TEXT NOT NULL,enabled INTEGER NOT NULL DEFAULT 1,next_poll INTEGER NOT NULL DEFAULT 0,last_error TEXT,created_by TEXT NOT NULL); INSERT INTO factory_workflow_intakes SELECT * FROM factory_workflow_intakes_v96; DROP TABLE factory_workflow_intakes_v96; CREATE INDEX workflow_intake_due ON factory_workflow_intakes(org_id,enabled,next_poll); PRAGMA user_version=97;")?;
    }
    tx.commit()?;
    Ok(())
}
pub fn metadata(c: &Connection, workspace: &str) -> Result<Value> {
    let row=c.query_row("SELECT revision,config_json,report_json,reported_at FROM factory_workflow_runtime WHERE workspace_id=?1",[workspace],|r|Ok((r.get::<_,u32>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,Option<String>>(3)?))).optional()?;
    let (revision, config, report, reported_at) =
        row.unwrap_or((0, "{}".into(), "{}".into(), None));
    let report: Report = serde_json::from_str(&report)?;
    let config = managed_defaults(serde_json::from_str(&config)?, &report);
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
    if config.agents.iter().any(|a| a.binding.is_some()) {
        let org: String = c.query_row(
            "SELECT org_id FROM factory_workspaces WHERE id=?1",
            [workspace],
            |r| r.get(0),
        )?;
        if resolve_bindings(c, &org, &config).is_err() {
            for a in config.agents.iter().filter(|a| a.binding.is_some()) {
                missing.push(format!("agent_binding:{}", a.role));
            }
        }
    }
    for a in &config.agents {
        if a.model.is_empty() {
            missing.push(format!("model:{}", a.role))
        }
        if !report.harnesses.contains(&a.harness) {
            missing.push(format!("harness:{}", a.harness))
        }
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
/// Project only the reusable, non-secret behavior. Never copy target credentials,
/// connectors, sessions or tool grants from an autonomous agent into a runner.
fn profile_model(detail: &crate::models::types::AutonomousAgentDetail) -> Option<String> {
    match super::queries::autonomous_executor(&detail.revision.config).ok()? {
        "pi" => Some(crate::automation::pi::MODEL.into()),
        "nexus" => None,
        _ => detail
            .revision
            .config
            .get("model")
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned),
    }
}
pub fn agent_profile(c: &Connection, org: &str, id: &str) -> Result<Value> {
    let detail = super::queries::get_autonomous_agent_detail(c, org, id)?
        .ok_or_else(|| anyhow::anyhow!("workflow_agent_unavailable"))?;
    let model = profile_model(&detail);
    Ok(
        json!({"id":detail.definition.id,"name":detail.definition.name,"template_key":detail.definition.template_key,"status":detail.definition.status,"validation_status":detail.revision.validation_status,"revision":{"revision":detail.revision.revision,"config_hash":detail.revision.config_hash,"config":{"executor":super::queries::autonomous_executor(&detail.revision.config)?,"model":model}}}),
    )
}
pub fn resolve_bindings(
    c: &Connection,
    org: &str,
    incoming: &RuntimeConfig,
) -> Result<RuntimeConfig> {
    let mut config = incoming.clone();
    for agent in &mut config.agents {
        if let Some(binding) = &agent.binding {
            let detail = super::queries::get_autonomous_agent_detail(c, org, &binding.id)?
                .ok_or_else(|| anyhow::anyhow!("workflow_agent_unavailable"))?;
            if detail.definition.status != "enabled" || detail.revision.validation_status != "valid"
            {
                bail!("workflow_agent_unavailable")
            }
            if !["github_issue_resolver", "github_pr_reviewer"]
                .contains(&detail.definition.template_key.as_str())
            {
                bail!("workflow_agent_template_incompatible")
            }
            if agent.inherit_model {
                agent.model = profile_model(&detail)
                    .ok_or_else(|| anyhow::anyhow!("workflow_agent_model_missing"))?;
            }
            agent.instructions = detail
                .revision
                .config
                .get("custom_instructions")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            if agent.instructions.len() > 16000 || secret_like(&agent.instructions) {
                bail!("workflow_agent_instructions_unsafe")
            }
            agent.binding = Some(AgentBinding {
                id: detail.definition.id,
                revision: detail.revision.revision,
                name: detail.definition.name,
                config_hash: detail.revision.config_hash,
            });
        } else {
            agent.instructions.clear();
            if agent.inherit_model {
                bail!("workflow_agent_model_missing")
            }
        }
    }
    Ok(config)
}
/// Empty Pi infrastructure selects the operator-provisioned SDK and backend OAuth.
/// Explicit custom images/connections are never silently replaced.
fn managed_defaults(mut config: RuntimeConfig, report: &Report) -> RuntimeConfig {
    for agent in &mut config.agents {
        if agent.harness == "pi"
            && (agent.model_connection == "backend"
                || (agent.model_connection.is_empty()
                    && agent.image.is_empty()
                    && agent.network.is_empty()))
        {
            agent.model_connection = "backend".into();
            if agent.network.is_empty() {
                agent.network = "none".into();
            }
            if agent.image.is_empty() {
                agent.image = report
                    .managed_images
                    .get("agent")
                    .cloned()
                    .unwrap_or_default();
            }
        }
    }
    config
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
    let tx = rusqlite::Transaction::new_unchecked(c, rusqlite::TransactionBehavior::Immediate)?;
    let report: Report = serde_json::from_value(w["runtime"]["report"].clone())?;
    let mut config = managed_defaults(resolve_bindings(&tx, org, config)?, &report);
    validate(&config, &repos)?;
    // Persist the managed recipe selection, not a machine-specific SDK image.
    // metadata() resolves it; each run snapshot still pins the actual digest.
    for agent in &mut config.agents {
        if agent.model_connection == "backend"
            && report.managed_images.get("agent") == Some(&agent.image)
        {
            agent.image.clear();
        }
    }
    let version = metadata(&tx, workspace)?["revision"].as_u64().unwrap_or(0) as u32;
    if version != expected {
        bail!("runtime_revision_conflict")
    }
    tx.execute("INSERT INTO factory_workflow_runtime(workspace_id,revision,config_json,updated_by) VALUES (?1,1,?2,?3) ON CONFLICT(workspace_id) DO UPDATE SET revision=revision+1,config_json=excluded.config_json,updated_by=excluded.updated_by,updated_at=datetime('now')",params![workspace,serde_json::to_string(&config)?,user])?;
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
    if report.managed_images.len() > 16
        || report
            .managed_images
            .iter()
            .any(|(key, value)| !name(key) || !image(value))
        || report.images.len() > 100
        || report.images.iter().any(|s| !image(s))
        || [
            &report.networks,
            &report.model_connections,
            &report.environments,
            &report.harnesses,
        ]
        .iter()
        .any(|v| v.len() > 100 || v.iter().any(|s| !name(s)))
        || report.error.as_ref().is_some_and(|s| {
            ![
                "runtime_image_not_allowed",
                "runtime_image_pull_failed",
                "runtime_image_provision_failed",
                "runtime_network_not_allowed",
                "runtime_model_connection_missing",
                "runtime_environment_missing",
                "runtime_invalid",
                "runtime_harness_missing",
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
    fn multi_repository_checks_fit_but_remain_bounded() {
        let repos = (0..49).map(|i| format!("repo-{i}")).collect::<Vec<_>>();
        let mut config = RuntimeConfig::default();
        for i in 0..256 {
            config.checks.push(Check {
                name: if i % 2 == 0 { "tests" } else { "build" }.into(),
                image: format!("sha256:{}", "a".repeat(64)),
                cwd: repos[i % 49].clone(),
                command: vec!["node".into(), format!("check-{i}.js")],
                environment_refs: vec![],
            });
        }
        validate(&config, &repos).unwrap();
        config.checks.push(config.checks[0].clone());
        assert_eq!(
            validate(&config, &repos).unwrap_err().to_string(),
            "runtime_limits_exceeded"
        );
    }
    #[test]
    fn managed_pi_resolves_infrastructure_without_overriding_custom_connections() {
        let mut config: RuntimeConfig=serde_json::from_value(json!({"agents":[{"role":"reviewer","harness":"pi","model":"gpt-example","image":"","network":"","model_connection":""}]})).unwrap();
        let mut report = Report::default();
        let sdk = format!("sha256:{}", "a".repeat(64));
        report.managed_images.insert("agent".into(), sdk.clone());
        let resolved = managed_defaults(config.clone(), &report);
        assert_eq!(resolved.agents[0].image, sdk);
        assert_eq!(resolved.agents[0].network, "none");
        assert_eq!(resolved.agents[0].model_connection, "backend");
        validate(&resolved, &[]).unwrap();
        config.agents[0].model_connection = "custom".into();
        assert!(managed_defaults(config, &report).agents[0].image.is_empty());
    }
    #[test]
    fn check_validation_identifies_the_actual_field_and_duplicate() {
        let mut config = RuntimeConfig::default();
        config.checks.push(Check {
            name: "tests".into(),
            image: "node:22".into(),
            cwd: "web".into(),
            command: vec!["npm".into(), "test".into()],
            environment_refs: vec![],
        });
        let repos = vec!["web".into()];
        assert_eq!(
            validate(&config, &repos).unwrap_err().to_string(),
            "runtime_check_image_invalid"
        );
        config.checks[0].image = format!("node@sha256:{}", "a".repeat(64));
        validate(&config, &repos).unwrap();
        config.checks[0].cwd = "unauthorized".into();
        assert_eq!(
            validate(&config, &repos).unwrap_err().to_string(),
            "runtime_check_repository_invalid"
        );
        config.checks[0].cwd = "web".into();
        config.checks[0].command.clear();
        assert_eq!(
            validate(&config, &repos).unwrap_err().to_string(),
            "runtime_check_command_invalid"
        );
        config.checks[0].command.push("npm".into());
        config.checks.push(config.checks[0].clone());
        assert_eq!(
            validate(&config, &repos).unwrap_err().to_string(),
            "runtime_check_duplicate"
        );
        config.checks[1].command.push("test:integration".into());
        validate(&config, &repos).unwrap();
    }
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
                    harness: "claude".into(),
                    binding: None,
                    inherit_model: false,
                    instructions: String::new(),
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
            harnesses: vec!["claude".into()],
            managed_images: Default::default(),
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
    fn catalog_bindings_are_tenant_scoped_versioned_and_project_no_credentials() {
        let c = super::super::connection::connect(":memory:").unwrap();
        super::super::migrations::run(&c).unwrap();
        let (org, user, _) =
            super::super::queries::bootstrap(&c, "Acme", "acme", "b@test.com", "Owner").unwrap();
        let request = crate::models::types::CreateAutonomousAgentRequest {
            name: "Code profile".into(),
            description: None,
            template_key: "github_issue_resolver".into(),
            config: json!({"executor":"claude","model":"sonnet","custom_instructions":"Respect acceptance criteria","target_password":"fixture-never-copy"}),
            budgets: json!({}),
        };
        let d = super::super::queries::create_autonomous_agent_definition(
            &c, &org.id, &user.id, &request,
        )
        .unwrap();
        let valid = |detail: &crate::models::types::AutonomousAgentDetail| {
            c.execute(
                "UPDATE autonomous_agent_definitions SET status='enabled' WHERE id=?1",
                [&detail.definition.id],
            )
            .unwrap();
            c.execute("INSERT INTO autonomous_agent_validations(id,org_id,definition_id,revision_id,config_hash,status,result_json,validated_by) VALUES(?1,?2,?3,?4,?5,'valid','{}',?6)",params![uuid::Uuid::new_v4().to_string(),org.id,detail.definition.id,detail.revision.id,detail.revision.config_hash,user.id]).unwrap();
        };
        valid(&d);
        let projected = agent_profile(&c, &org.id, &d.definition.id).unwrap();
        assert_eq!(projected["revision"]["config"]["model"], "sonnet");
        assert!(!projected.to_string().contains("fixture-never-copy"));
        assert!(agent_profile(&c, "other-org", &d.definition.id).is_err());
        let config:RuntimeConfig=serde_json::from_value(json!({"agents":[{"role":"reviewer","image":format!("sha256:{}","a".repeat(64)),"model":"unused","model_connection":"proxy","network":"model-net","binding":{"id":d.definition.id,"revision":999,"name":"forged"},"inherit_model":true}]})).unwrap();
        let frozen = resolve_bindings(&c, &org.id, &config).unwrap();
        assert_eq!(frozen.agents[0].model, "sonnet");
        assert_eq!(frozen.agents[0].binding.as_ref().unwrap().revision, 1);
        assert_eq!(frozen.agents[0].instructions, "Respect acceptance criteria");
        assert!(!serde_json::to_string(&frozen)
            .unwrap()
            .contains("fixture-never-copy"));
        assert!(resolve_bindings(&c, "other-org", &config).is_err());
        let next = super::super::queries::update_autonomous_agent_definition(
            &c,
            &org.id,
            &user.id,
            &d.definition.id,
            &crate::models::types::UpdateAutonomousAgentRequest {
                name: None,
                description: None,
                config: Some(json!({"executor":"claude","model":"haiku"})),
                budgets: None,
            },
        )
        .unwrap()
        .unwrap();
        assert!(resolve_bindings(&c, &org.id, &config).is_err());
        valid(&next);
        assert_eq!(
            resolve_bindings(&c, &org.id, &config).unwrap().agents[0].model,
            "haiku"
        );
        assert_eq!(frozen.agents[0].model, "sonnet");
        c.execute(
            "UPDATE autonomous_agent_definitions SET status='disabled' WHERE id=?1",
            [&d.definition.id],
        )
        .unwrap();
        assert!(resolve_bindings(&c, &org.id, &config).is_err());
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
