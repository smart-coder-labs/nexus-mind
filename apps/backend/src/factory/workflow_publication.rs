//! Credential-free runner publication. The control plane reproduces the EXACT
//! verified Git commits through GitHub's Git Database API; never gives a runner
//! a GitHub token, merges, approves or deploys. Writes reconcile by stable refs.
use crate::{
    db::{workflow_execution, workflows},
    factory::{
        workflow_policy,
        workflows::{relative_path, valid_sha, Definition, Repository, StepKind},
    },
    store::sqlite::SqliteStore,
};
use anyhow::{bail, Result};
use base64::Engine;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct File {
    pub path: String,
    pub mode: String,
    pub content_base64: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub sha: String,
    pub tree_sha: String,
    pub parent_sha: String,
    pub message: String,
    pub files: Vec<File>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Export {
    pub path: String,
    pub commits: Vec<Commit>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub lease_token: String,
    pub revision: u32,
    pub repositories: Vec<Export>,
}

pub fn github_repository(remote: &str) -> Result<String> {
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("https://github.com/"))
        .ok_or_else(|| anyhow::anyhow!("github_remote_required"))?;
    let path = path.strip_suffix(".git").unwrap_or(path);
    crate::automation::connectors::validate_repository(path)?;
    Ok(path.into())
}
pub fn validate_exports(
    snapshot: &Value,
    heads: &Value,
    exports: &[Export],
) -> Result<Vec<String>> {
    let repos: Vec<Repository> =
        serde_json::from_value(snapshot["workspace"]["repositories"].clone())?;
    let changed: Vec<_> = repos
        .iter()
        .filter(|r| heads[&r.path] != r.head_sha)
        .collect();
    if exports.len() != changed.len() {
        bail!("incomplete_change_set")
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut paths = Vec::new();
    let mut bytes = 0;
    for e in exports {
        let repo = changed
            .iter()
            .find(|r| r.path == e.path)
            .ok_or_else(|| anyhow::anyhow!("unauthorized_repository"))?;
        if !repo.writable || !seen.insert(&e.path) || e.commits.is_empty() || e.commits.len() > 10 {
            bail!("invalid_export")
        }
        let mut parent = repo.head_sha.clone();
        for c in &e.commits {
            if c.parent_sha != parent
                || !valid_sha(&c.sha)
                || !valid_sha(&c.tree_sha)
                || c.message.len() > 1024
                || !c.message.starts_with("NexusMind ")
            {
                bail!("invalid_commit_chain")
            }
            parent = c.sha.clone();
            let mut files = std::collections::BTreeSet::new();
            for f in &c.files {
                if !relative_path(&f.path)
                    || !files.insert(&f.path)
                    || !["100644", "100755", "120000"].contains(&f.mode.as_str())
                    || f.path.split('/').any(|p| {
                        p == ".git"
                            || p == ".env"
                            || p.starts_with(".env.")
                                && !p.ends_with(".example")
                                && !p.ends_with(".sample")
                                && !p.ends_with(".template")
                    })
                {
                    bail!("unsafe_publication_path")
                }
                if let Some(encoded) = &f.content_base64 {
                    bytes += base64::engine::general_purpose::STANDARD
                        .decode(encoded)?
                        .len();
                }
                paths.push(format!("{}/{}", e.path, f.path));
                if bytes > 1024 * 1024 || paths.len() > 100 {
                    bail!("publication_budget_exceeded")
                }
            }
        }
        if heads[&e.path] != parent {
            bail!("unverified_publication_head")
        }
    }
    Ok(paths)
}
fn verified(
    c: &rusqlite::Connection,
    run: &str,
    definition: &Definition,
    step: &str,
    heads: &Value,
) -> Result<bool> {
    let mut required = std::collections::BTreeSet::new();
    let mut queue = vec![step.to_string()];
    while let Some(id) = queue.pop() {
        if required.insert(id.clone()) {
            queue.extend(
                definition
                    .steps
                    .iter()
                    .find(|s| s.id == id)
                    .ok_or_else(|| anyhow::anyhow!("step_missing"))?
                    .depends_on
                    .clone(),
            );
        }
    }
    let mut gate = false;
    let mut roles = std::collections::BTreeSet::new();
    for s in &definition.steps {
        if required.contains(&s.id) && matches!(s.kind, StepKind::Gate | StepKind::Check) {
            let (status,text):(String,Option<String>)=c.query_row("SELECT status,result_json FROM factory_workflow_steps WHERE run_id=?1 AND step_id=?2",params![run,s.id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            let result: Value = serde_json::from_str(text.as_deref().unwrap_or("null"))?;
            if status != "succeeded"
                || &result["heads"] != heads
                || result["evidence"].as_array().is_none_or(|a| a.is_empty())
            {
                return Ok(false);
            }
            if matches!(s.kind, StepKind::Check) {
                for evidence in result["evidence"].as_array().into_iter().flatten() {
                    let applicable = evidence["status"] == "pass"
                        || (s.options["allow_not_applicable"] == true
                            && evidence["status"] == "not_applicable"
                            && result["output"]["not_applicable_reason"]
                                .as_str()
                                .is_some_and(|s| !s.trim().is_empty()));
                    if applicable && &evidence["commits"] == heads {
                        if let Some(name) = evidence["check"].as_str() {
                            roles.insert(name.to_string());
                        }
                    }
                }
            }
            if matches!(s.kind, StepKind::Gate) {
                gate = true;
            }
        }
    }
    Ok(gate && roles.contains("tests") && roles.contains("build"))
}

fn qa_notes(detail: &Value) -> String {
    let mut notes = Vec::new();
    for step in detail["steps"].as_array().into_iter().flatten() {
        let result = &step["result"];
        if result["qa_report_only"] != true || result["heads"] != detail["execution"]["heads"] {
            continue;
        }
        for evidence in result["evidence"].as_array().into_iter().flatten() {
            if evidence["status"] == "fail" {
                notes.push(format!("- `{}` / `{}`: **failed**. Report artifact: `{}`.",
                    step["id"].as_str().unwrap_or("qa"),
                    evidence["check"].as_str().unwrap_or("qa"),
                    evidence["artifact_digest"].as_str().unwrap_or("unknown")));
            }
        }
    }
    if notes.is_empty() { return String::new(); }
    let total = notes.len();
    notes.truncate(50);
    format!("## QA findings (non-blocking)\n\nThese checks failed; they were not marked as passed. Build/tests remain required.\nShowing {} of {total} failed checks. Full evidence for change set revision {} is retained in NexusMind run `{}`:\n\n{}\n",
        notes.len(), detail["execution"]["revision"], detail["id"].as_str().unwrap_or(""), notes.join("\n"))
}

fn with_qa_notes(body: &str, notes: &str) -> Result<String> {
    const START: &str = "<!-- nexusmind:qa:start -->";
    const END: &str = "<!-- nexusmind:qa:end -->";
    let section = if notes.is_empty() { String::new() } else { format!("{START}\n{notes}{END}") };
    if let Some(start) = body.find(START) {
        let end = body[start..].find(END).ok_or_else(|| anyhow::anyhow!("qa_note_marker_conflict"))? + start + END.len();
        let mut updated = body.to_string();
        updated.replace_range(start..end, &section);
        return Ok(updated);
    }
    Ok(if section.is_empty() { body.to_string() } else { format!("{body}\n\n{section}") })
}
pub fn backend_account_enabled(org: &str) -> bool {
    std::env::var("FACTORY_GITHUB_PUBLISH_ORG_ID")
        .ok()
        .as_deref()
        == Some(org)
}
#[derive(PartialEq)]
enum PublicationCredential {
    Connector(String),
    Backend(String),
}
fn authorize(
    store: &SqliteStore,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    request: &Request,
    paths: &[String],
) -> Result<(Value, PublicationCredential)> {
    let h = store.conn();
    let c = h
        .lock()
        .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
    workflow_execution::lease(
        &c,
        org,
        runner,
        run,
        step,
        &request.lease_token,
        chrono::Utc::now().timestamp(),
    )?;
    let d = workflows::run_detail(&c, org, run)?;
    let definition: Definition = serde_json::from_value(d["snapshot"]["definition"].clone())?;
    let s = definition
        .steps
        .iter()
        .find(|s| s.id == step && matches!(s.kind, StepKind::Publish))
        .ok_or_else(|| anyhow::anyhow!("not_publish_step"))?;
    if d["execution"]["revision"].as_u64() != Some(u64::from(request.revision))
        || !verified(&c, run, &definition, step, &d["execution"]["heads"])?
    {
        bail!("verification_required")
    }
    workflow_policy::require(&c, org, run, "publish", paths, true)?;
    workflow_policy::require(&c, org, run, "open_pr", paths, true)?;
    if s.options["github_auth"] == "server_gh_cli" {
        if !backend_account_enabled(org) {
            bail!("backend_github_publication_not_authorized")
        }
        if s.options["connector_id"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
        {
            bail!("ambiguous_github_publication_identity")
        }
        let login = s.options["github_login"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 100)
            .ok_or_else(|| anyhow::anyhow!("backend_github_login_required"))?;
        let repos: Vec<Repository> =
            serde_json::from_value(d["snapshot"]["workspace"]["repositories"].clone())?;
        for export in &request.repositories {
            let repo = repos
                .iter()
                .find(|r| r.path == export.path && r.writable)
                .ok_or_else(|| anyhow::anyhow!("unauthorized_repository"))?;
            github_repository(&repo.remote)?;
        }
        return Ok((
            d,
            PublicationCredential::Backend(login.to_ascii_lowercase()),
        ));
    }
    let connector_id = s.options["connector_id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("github_connector_required"))?;
    let (connector, token) =
        crate::db::queries::get_autonomous_agent_connector_secret(&c, org, connector_id)?
            .ok_or_else(|| anyhow::anyhow!("connector_unavailable"))?;
    if connector.kind != "target_secret"
        || connector.metadata["purpose"] != "factory_workflow_github"
        || !connector.scopes.iter().any(|s| s == "github:write")
    {
        bail!("connector_not_for_workflow_github")
    }
    for export in &request.repositories {
        let repos: Vec<Repository> =
            serde_json::from_value(d["snapshot"]["workspace"]["repositories"].clone())?;
        let remote = &repos
            .iter()
            .find(|r| r.path == export.path)
            .ok_or_else(|| anyhow::anyhow!("unauthorized_repository"))?
            .remote;
        let repository = github_repository(remote)?;
        if !connector.metadata["repositories"]
            .as_array()
            .is_some_and(|a| a.iter().any(|r| r == &repository))
        {
            bail!("connector_repository_scope")
        }
    }
    Ok((d, PublicationCredential::Connector(token)))
}
struct Github {
    client: reqwest::Client,
    base: String,
    token: String,
}
impl Github {
    fn new(token: String) -> Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
            base: "https://api.github.com".into(),
            token,
        })
    }
    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .header("User-Agent", "nexusmind-workflow")
            .header("Accept", "application/vnd.github+json");
        if let Some(body) = body {
            request = request.json(&body);
        }
        let mut response = request
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("github_connection_failed"))?;
        let status = response.status();
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            if bytes.len() + chunk.len() > 2 * 1024 * 1024 {
                bail!("github_response_limit")
            };
            bytes.extend_from_slice(&chunk);
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(Value::Null);
        }
        if !status.is_success() {
            bail!("github_write_or_read_rejected")
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    async fn get(&self, p: &str) -> Result<Value> {
        self.call(reqwest::Method::GET, p, None).await
    }
    async fn post(&self, p: &str, b: Value) -> Result<Value> {
        self.call(reqwest::Method::POST, p, Some(b)).await
    }
}
/// True when `current` only adds commits on top of the verified base and none of
/// them touch a path the export changes: the verified commits then merge without
/// conflicts and the product CI validates the combination on the PR. Anything
/// uncertain (diverged, truncated or unreadable compare) keeps the fresh-source
/// reverification.
async fn base_advance_is_disjoint(
    github: &Github,
    prefix: &str,
    verified: &str,
    current: &str,
    export: &Export,
) -> bool {
    let Ok(compare) = github
        .get(&format!("{prefix}/compare/{verified}...{current}"))
        .await
    else {
        return false;
    };
    let Some(files) = compare["files"].as_array() else { return false };
    // GitHub lists at most 300 files per compare.
    if compare["status"] != "ahead" || files.len() >= 300 {
        return false;
    }
    let changed: std::collections::BTreeSet<&str> = export
        .commits
        .iter()
        .flat_map(|c| c.files.iter().map(|f| f.path.as_str()))
        .collect();
    !files.iter().any(|f| {
        ["filename", "previous_filename"]
            .iter()
            .any(|k| f[*k].as_str().is_some_and(|p| changed.contains(p)))
    })
}

async fn required_ci(
    github: &Github,
    prefix: &str,
    number: i64,
    head: &str,
    branch: &Value,
    options: &Value,
    check: &impl Fn() -> Result<()>,
) -> Result<Value> {
    let mut required: Vec<String> = serde_json::from_value(
        options
            .get("github_required_checks")
            .cloned()
            .unwrap_or(json!([])),
    )?;
    for name in branch
        .pointer("/protection/required_status_checks/contexts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
    {
        required.push(name.into());
    }
    if branch["protected"] == true {
        let name = branch["name"].as_str().unwrap_or("main");
        let rules = github
            .get(&format!("{prefix}/rules/branches/{name}"))
            .await?;
        for rule in rules
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["type"] == "required_status_checks")
        {
            for context in rule
                .pointer("/parameters/required_status_checks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|r| r["context"].as_str())
            {
                required.push(context.into());
            }
        }
    }
    required.sort();
    required.dedup();
    if required.is_empty() {
        return Ok(json!({"status":"not_configured"}));
    }
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(
            options["ci_timeout_seconds"]
                .as_u64()
                .unwrap_or(600)
                .clamp(5, 1800),
        );
    loop {
        check()?;
        let pull = github.get(&format!("{prefix}/pulls/{number}")).await?;
        if pull["head"]["sha"] != head {
            bail!("pr_head_changed")
        }
        let checks = github
            .get(&format!(
                "{prefix}/commits/{head}/check-runs?filter=latest&per_page=100"
            ))
            .await?;
        let mut runs = checks["check_runs"].as_array().cloned().unwrap_or_default();
        let statuses = github
            .get(&format!("{prefix}/commits/{head}/status"))
            .await?;
        if !statuses.is_null() {
            runs.extend(
                crate::automation::merge_gate::statuses_as_runs(&statuses)
                    .map_err(anyhow::Error::msg)?,
            );
        }
        if crate::automation::merge_gate::required_checks_verdict(&required, &runs).is_ok() {
            return Ok(json!({"status":"passed","required_checks":required,"head_sha":head}));
        }
        if runs.iter().any(|r| {
            required.iter().any(|n| r["name"] == *n)
                && r["status"] == "completed"
                && r["conclusion"] != "success"
        }) {
            bail!("required_ci_failed")
        }
        if tokio::time::Instant::now() >= deadline {
            bail!("required_ci_timeout")
        }
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }
}

pub async fn publish(
    store: &SqliteStore,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    request: &Request,
) -> Result<Value> {
    let (_, credential) = authorize(store, org, runner, run, step, request, &[])?;
    let token = match credential {
        PublicationCredential::Connector(token) => token,
        PublicationCredential::Backend(_) => crate::automation::worker::server_github_token()
            .await
            .map_err(|_| anyhow::anyhow!("backend_github_authentication_required"))?,
    };
    let github = Github::new(token)?;
    publish_using(store, org, runner, run, step, request, &github).await
}
async fn publish_using(
    store: &SqliteStore,
    org: &str,
    runner: &str,
    run: &str,
    step: &str,
    request: &Request,
    github: &Github,
) -> Result<Value> {
    let (initial, credential) = authorize(store, org, runner, run, step, request, &[])?;
    match &credential {
        PublicationCredential::Connector(token) if token != &github.token => {
            bail!("connector_changed")
        }
        PublicationCredential::Backend(login) => {
            let identity = github.get("/user").await?;
            if !identity["login"]
                .as_str()
                .is_some_and(|actual| actual.eq_ignore_ascii_case(login))
            {
                bail!("backend_github_identity_changed")
            }
        }
        _ => {}
    }
    if request.repositories.is_empty() {
        bail!("no_changes_to_publish")
    }
    let paths = validate_exports(
        &initial["snapshot"],
        &initial["execution"]["heads"],
        &request.repositories,
    )?;
    authorize(store, org, runner, run, step, request, &paths)?;
    let mut published = Vec::new();
    let qa = qa_notes(&initial);
    let check_authority = || -> Result<()> {
        let (_, current) = authorize(store, org, runner, run, step, request, &paths)?;
        if current != credential {
            bail!("publication_authority_changed")
        };
        Ok(())
    };
    for export in &request.repositories {
        let repos: Vec<Repository> =
            serde_json::from_value(initial["snapshot"]["workspace"]["repositories"].clone())?;
        let repo = repos
            .iter()
            .find(|r| r.path == export.path)
            .ok_or_else(|| anyhow::anyhow!("unauthorized_repository"))?;
        let repository = github_repository(&repo.remote)?;
        let prefix = format!("/repos/{repository}");
        let info = github.get(&prefix).await?;
        if matches!(credential, PublicationCredential::Backend(_))
            && info["permissions"]["push"] != true
        {
            bail!("github_repository_write_required")
        }
        let default = info["default_branch"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("github_repository_unavailable"))?;
        if !default
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/".contains(&b))
        {
            bail!("unsupported_base_branch")
        }
        let branch = github.get(&format!("{prefix}/branches/{default}")).await?;
        let current = branch["commit"]["sha"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("github_repository_unavailable"))?;
        if current != repo.head_sha
            && !base_advance_is_disjoint(github, &prefix, &repo.head_sha, current, export).await
        {
            bail!("base_moved_reverify")
        }
        for commit in &export.commits {
            let parent = github
                .get(&format!("{prefix}/git/commits/{}", commit.parent_sha))
                .await?;
            let base_tree = parent["tree"]["sha"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("github_parent_missing"))?;
            let mut entries = Vec::new();
            for file in &commit.files {
                check_authority()?;
                let sha = if let Some(content) = &file.content_base64 {
                    let blob = github
                        .post(
                            &format!("{prefix}/git/blobs"),
                            json!({"content":content,"encoding":"base64"}),
                        )
                        .await?;
                    blob["sha"].clone()
                } else {
                    Value::Null
                };
                entries.push(json!({"path":file.path,"mode":file.mode,"type":"blob","sha":sha}));
            }
            check_authority()?;
            let tree = github
                .post(
                    &format!("{prefix}/git/trees"),
                    json!({"base_tree":base_tree,"tree":entries}),
                )
                .await?;
            if tree["sha"] != commit.tree_sha {
                bail!("published_tree_mismatch")
            }
            let identity = json!({"name":"NexusMind","email":"factory@nexusmind.local","date":"2000-01-01T00:00:00Z"});
            let created=github.post(&format!("{prefix}/git/commits"),json!({"message":commit.message,"tree":commit.tree_sha,"parents":[commit.parent_sha],"author":identity,"committer":identity})).await?;
            if created["sha"] != commit.sha {
                bail!("published_commit_mismatch")
            }
        }
        let head = initial["execution"]["heads"][&export.path]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("missing_head"))?;
        use sha2::{Digest, Sha256};
        let suffix = hex::encode(Sha256::digest(export.path.as_bytes()));
        let name = format!("nexusmind/workflow-{}-{}", run, &suffix[..12]);
        let reference = github
            .get(&format!("{prefix}/git/ref/heads/{name}"))
            .await?;
        check_authority()?;
        if reference.is_null() {
            if github
                .post(
                    &format!("{prefix}/git/refs"),
                    json!({"ref":format!("refs/heads/{name}"),"sha":head}),
                )
                .await
                .is_err()
            {
                let reconciled = github
                    .get(&format!("{prefix}/git/ref/heads/{name}"))
                    .await?;
                if reconciled["object"]["sha"] != head {
                    bail!("branch_delivery_uncertain")
                }
            }
        } else if reference["object"]["sha"] != head {
            github
                .call(
                    reqwest::Method::PATCH,
                    &format!("{prefix}/git/refs/heads/{name}"),
                    Some(json!({"sha":head,"force":false})),
                )
                .await?;
        }
        let owner = repository.split('/').next().unwrap_or_default();
        let pulls = github
            .get(&format!("{prefix}/pulls?state=open&head={owner}:{name}"))
            .await?;
        let mut pull = pulls.as_array().and_then(|a| a.first()).cloned();
        if pull.is_none() {
            check_authority()?;
            let title = initial["task"]["title"]
                .as_str()
                .unwrap_or("NexusMind change");
            let body=format!("NexusMind workflow `{run}`.\n\nVerified change set revision {}.\n\n{}\n\nThis is a draft. Each repository's PR belongs to the same integrated change set; do not merge independently without checking compatibility. NexusMind does not merge or deploy this workflow.\n<!-- nexusmind:workflow:{run} -->",request.revision,initial["execution"]["heads"]);
            let body = with_qa_notes(&body, &qa)?;
            pull=github.post(&format!("{prefix}/pulls"),json!({"title":title.chars().take(200).collect::<String>(),"body":body,"head":name,"base":default,"draft":true})).await.ok();
            if pull.is_none() {
                pull = github
                    .get(&format!("{prefix}/pulls?state=open&head={owner}:{name}"))
                    .await?
                    .as_array()
                    .and_then(|a| a.first())
                    .cloned();
            }
        }
        let pull = pull.ok_or_else(|| anyhow::anyhow!("pr_delivery_uncertain"))?;
        if pull["head"]["sha"] != head {
            bail!("pr_head_changed")
        }
        let options = initial["snapshot"]["definition"]["steps"]
            .as_array()
            .and_then(|steps| steps.iter().find(|s| s["id"] == step))
            .map(|s| s["options"].clone())
            .unwrap_or(Value::Null);
        let number = pull["number"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("pr_number_missing"))?;
        let previous_body = pull["body"].as_str().unwrap_or("");
        let body = with_qa_notes(previous_body, &qa)?;
        if body != previous_body {
            check_authority()?;
            github.call(reqwest::Method::PATCH, &format!("{prefix}/pulls/{number}"), Some(json!({"body":body}))).await?;
        }
        let pending = json!({"repository":repository,"number":number,"url":pull["html_url"],"head_sha":head,"branch":name,"ci":{"status":"pending"}});
        {
            let h = store.conn();
            let c = h
                .lock()
                .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
            c.execute("INSERT INTO factory_workflow_publications(run_id,repository_path,head_sha,state,result_json) VALUES (?1,?2,?3,'awaiting_ci',?4) ON CONFLICT(run_id,repository_path) DO UPDATE SET head_sha=excluded.head_sha,state='awaiting_ci',result_json=excluded.result_json",params![run,export.path,head,pending.to_string()])?;
            workflows::event(&c, run, Some(step), "pull_request.opened", pending)?;
        }
        let ci = required_ci(
            github,
            &prefix,
            number,
            head,
            &branch,
            &options,
            &check_authority,
        )
        .await?;
        let result = json!({"repository":repository,"number":pull["number"],"url":pull["html_url"],"head_sha":head,"branch":name,"ci":ci});
        let h = store.conn();
        let c = h
            .lock()
            .map_err(|_| anyhow::anyhow!("storage_unavailable"))?;
        c.execute("INSERT INTO factory_workflow_publications(run_id,repository_path,head_sha,state,result_json) VALUES (?1,?2,?3,'published',?4) ON CONFLICT(run_id,repository_path) DO UPDATE SET head_sha=excluded.head_sha,state='published',result_json=excluded.result_json",params![run,export.path,head,result.to_string()])?;
        workflows::event(
            &c,
            run,
            Some(step),
            "pull_request.published",
            result.clone(),
        )?;
        published.push(result);
    }
    Ok(json!({"pull_requests":published,"revision":request.revision}))
}

#[cfg(test)]
#[path = "workflow_publication_tests.rs"]
mod delivery_tests;
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remotes_cannot_redirect_credentials() {
        assert_eq!(
            github_repository("git@github.com:acme/api.git").unwrap(),
            "acme/api"
        );
        for bad in [
            "https://github.com.evil/acme/api",
            "https://token@github.com/acme/api",
            "../acme/api",
            "https://github.com/a/b/../../c",
        ] {
            assert!(github_repository(bad).is_err());
        }
    }
}
