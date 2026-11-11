use anyhow::Result;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Serialize)]
struct GitHubAppClaims {
    iat: i64,
    exp: i64,
    iss: String,
}

#[derive(Deserialize)]
struct InstallationTokenResponse {
    token: String,
}

fn repository_parts(repository: &str) -> Result<(&str, &str)> {
    let mut parts = repository.split('/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if owner.is_empty()
        || repo.is_empty()
        || parts.next().is_some()
        || !owner
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        || !repo
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        anyhow::bail!("invalid_repository");
    }
    Ok((owner, repo))
}

pub fn validate_repository(repository: &str) -> Result<()> {
    repository_parts(repository).map(|_| ())
}

pub fn validate_slack_webhook(webhook_url: &str) -> Result<()> {
    let url =
        reqwest::Url::parse(webhook_url).map_err(|_| anyhow::anyhow!("invalid_slack_webhook"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("hooks.slack.com")
        || !url.path().starts_with("/services/")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        anyhow::bail!("invalid_slack_webhook")
    }
    Ok(())
}

pub async fn github_installation_token(
    app_id: &str,
    installation_id: i64,
    private_key_pem: &str,
) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    let claims = GitHubAppClaims {
        iat: now - 60,
        exp: now + 540,
        iss: app_id.to_string(),
    };
    let jwt = encode(
        &Header::new(Algorithm::RS256),
        &claims,
        &EncodingKey::from_rsa_pem(private_key_pem.as_bytes())?,
    )?;
    let response = reqwest::Client::new()
        .post(format!(
            "https://api.github.com/app/installations/{installation_id}/access_tokens"
        ))
        .header("Authorization", format!("Bearer {jwt}"))
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<InstallationTokenResponse>().await?.token)
}

pub(crate) async fn github_post(token: &str, path: &str, body: Value) -> Result<Value> {
    github_post_at("https://api.github.com",token,path,body).await
}

async fn github_post_at(base_url:&str,token:&str,path:&str,body:Value)->Result<Value>{
    let response = reqwest::Client::new()
        .post(format!("{base_url}{path}"))
        .timeout(GITHUB_TIMEOUT)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        // Surface GitHub's actual error message instead of a bare status.
        anyhow::bail!(
            "github_api_{}: {}",
            status.as_u16(),
            text.chars().take(300).collect::<String>()
        );
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

pub(crate) async fn github_patch(token: &str, path: &str, body: Value) -> Result<Value> {
    Ok(reqwest::Client::new()
        .patch(format!("https://api.github.com{path}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .json(&body)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// The HTTP status behind a failed GitHub call, if it got that far.
fn http_status(error: &anyhow::Error) -> Option<u16> {
    error
        .downcast_ref::<reqwest::Error>()
        .and_then(|e| e.status())
        .map(|s| s.as_u16())
}

/// No GitHub read may hang the worker: callers run on the tick loop.
const GITHUB_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

pub(crate) async fn github_get(token: &str, path: &str) -> Result<Value> {
    github_get_at("https://api.github.com",token,path).await
}

async fn github_get_at(base_url:&str,token:&str,path:&str)->Result<Value>{
    Ok(reqwest::Client::new()
        .get(format!("{base_url}{path}"))
        .timeout(GITHUB_TIMEOUT)
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

/// GET a resource as raw text under a caller-chosen media type (e.g. the diff
/// or patch representations that GitHub renders server-side). Mirrors
/// `github_get` but returns the body verbatim instead of parsing JSON, and
/// surfaces a truncated body on failure so the error stays diagnosable.
async fn github_get_text(token: &str, path: &str, accept: &str) -> Result<String> {
    let response = reqwest::Client::new()
        .get(format!("https://api.github.com{path}"))
        .bearer_auth(token)
        .header("Accept", accept)
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "github_api_{}: {}",
            status.as_u16(),
            text.chars().take(200).collect::<String>()
        );
    }
    Ok(text)
}

async fn github_put(token: &str, path: &str, body: Value) -> Result<Value> {
    let response = reqwest::Client::new()
        .put(format!("https://api.github.com{path}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .json(&body)
        .send()
        .await?;
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "github_api_{}: {}",
            status.as_u16(),
            text.chars().take(300).collect::<String>()
        );
    }
    Ok(serde_json::from_str(&text).unwrap_or(Value::Null))
}

/// Dedicated, never-merged branch that holds QA screenshot evidence. Referencing
/// images via github.com/.../raw/<branch>/... renders them inline in issues of a
/// private repo (served with the viewer's session), unlike raw.githubusercontent.
const EVIDENCE_BRANCH: &str = "nexusmind-qa-assets";

async fn ensure_evidence_branch(token: &str, owner: &str, repo: &str) -> Result<()> {
    if github_get(
        token,
        &format!("/repos/{owner}/{repo}/git/ref/heads/{EVIDENCE_BRANCH}"),
    )
    .await
    .is_ok()
    {
        return Ok(());
    }
    let info = github_get(token, &format!("/repos/{owner}/{repo}")).await?;
    let default = info
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or("main");
    let head = github_get(
        token,
        &format!("/repos/{owner}/{repo}/git/ref/heads/{default}"),
    )
    .await?;
    let sha = head
        .pointer("/object/sha")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("default_head_sha_missing"))?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/git/refs"),
        json!({"ref": format!("refs/heads/{EVIDENCE_BRANCH}"), "sha": sha}),
    )
    .await?;
    Ok(())
}

/// Mirror an image fetched from `source_url` (e.g. its R2 URL) into the repo's
/// evidence branch so it renders permanently inside the issue; returns the
/// github raw URL to embed.
pub async fn mirror_evidence_to_repo(
    token: &str,
    repository: &str,
    key: &str,
    source_url: &str,
) -> Result<String> {
    use base64::Engine as _;
    let (owner, repo) = repository_parts(repository)?;
    let bytes = reqwest::get(source_url)
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    if bytes.len() > 25_000_000 {
        anyhow::bail!("evidence_too_large")
    }
    ensure_evidence_branch(token, owner, repo).await?;
    let path = format!("qa-evidence/{key}");
    let content = base64::engine::general_purpose::STANDARD.encode(&bytes);
    github_put(
        token,
        &format!("/repos/{owner}/{repo}/contents/{path}"),
        json!({
            "message": format!("chore(qa): evidence {key}"),
            "content": content,
            "branch": EVIDENCE_BRANCH,
        }),
    )
    .await?;
    Ok(format!(
        "https://github.com/{owner}/{repo}/raw/{EVIDENCE_BRANCH}/{path}"
    ))
}

pub async fn get_github_issue(token: &str, repository: &str, number: i64) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(token, &format!("/repos/{owner}/{repo}/issues/{number}")).await
}

/// The repository itself (`private`, `visibility`, `default_branch`, …).
pub async fn get_github_repository(token: &str, repository: &str) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(token, &format!("/repos/{owner}/{repo}")).await
}

pub async fn get_github_pull(token: &str, repository: &str, number: i64) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(token, &format!("/repos/{owner}/{repo}/pulls/{number}")).await
}

/// Merge a pull request with the given method ("squash" | "merge" | "rebase"),
/// keeping the branch. Used by the PR reviewer's opt-in auto-merge; a failed
/// merge (e.g. not mergeable, protected branch) surfaces GitHub's message.
pub async fn merge_github_pull(
    token: &str,
    repository: &str,
    number: i64,
    method: &str,
    sha: Option<&str>,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    // With `sha`, GitHub refuses (409) if the head moved since it was verified.
    let mut body = json!({ "merge_method": method });
    if let Some(sha) = sha {
        body["sha"] = json!(sha);
    }
    github_put(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}/merge"),
        body,
    )
    .await
}

/// Every file changed by a pull request, following pagination. Fails with
/// `too_many_files` past `MAX_PULL_FILES` rather than returning a partial list.
pub async fn list_github_pull_files(
    token: &str,
    repository: &str,
    number: i64,
) -> Result<Vec<super::merge_gate::ChangedFile>> {
    use super::merge_gate::{parse_changed_files, MAX_PULL_FILES};
    let (owner, repo) = repository_parts(repository)?;
    let mut files = Vec::new();
    for page in 1.. {
        let value = github_get(
            token,
            &format!("/repos/{owner}/{repo}/pulls/{number}/files?per_page=100&page={page}"),
        )
        .await?;
        let batch = parse_changed_files(&value).map_err(|reason| anyhow::anyhow!(reason))?;
        let last = batch.len() < 100;
        files.extend(batch);
        if files.len() > MAX_PULL_FILES {
            anyhow::bail!("too_many_files")
        }
        if last {
            break;
        }
    }
    Ok(files)
}

/// Three-dot diff (`merge-base(base, head)…head`) for a pull request, pinned to a
/// specific `head_sha` so it matches exactly the snapshot the run checked out.
///
/// Uses the compare API with `Accept: application/vnd.github.diff`, which GitHub
/// renders server-side. That means: (1) it needs no local git history, so it can
/// run against a shallow `--depth 1` checkout; (2) it works even when the PR is
/// conflicting/dirty (`merge_commit_sha` null), unlike a local `git diff base...HEAD`
/// which fails with "no merge base"; and (3) pinning the right side to `head_sha`
/// keeps the diff consistent with the checked-out code even if the PR head moved.
pub async fn get_github_pull_diff(
    token: &str,
    repository: &str,
    base: &str,
    head_sha: &str,
) -> Result<String> {
    let (owner, repo) = repository_parts(repository)?;
    if base.is_empty() || base.contains("..") || base.starts_with('/') {
        anyhow::bail!("invalid_base_branch")
    }
    if head_sha.len() != 40 || !head_sha.chars().all(|value| value.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_commit_sha")
    }
    github_get_text(
        token,
        &format!("/repos/{owner}/{repo}/compare/{base}...{head_sha}"),
        "application/vnd.github.diff",
    )
    .await
}

pub async fn get_github_branch(token: &str, repository: &str, branch: &str) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    if branch.is_empty() || branch.contains("..") || branch.starts_with('/') {
        anyhow::bail!("invalid_branch")
    };
    github_get(token, &format!("/repos/{owner}/{repo}/branches/{branch}")).await
}

/// Open issues carrying `label`, newest first (at most 100). Pull requests are
/// included by the API; callers skip entries with a `pull_request` key.
pub async fn list_labeled_github_issues(
    token: &str,
    repository: &str,
    label: &str,
) -> Result<Vec<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    if label.is_empty()
        || !label
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        anyhow::bail!("invalid_label")
    }
    let value = github_get(
        token,
        &format!("/repos/{owner}/{repo}/issues?state=open&labels={label}&sort=updated&direction=desc&per_page=100"),
    )
    .await?;
    value
        .as_array()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("issues_unreadable"))
}

/// Workflow intake must not silently starve older issues behind the first page.
pub async fn list_labeled_github_issues_bounded(token:&str,repository:&str,label:&str)->Result<Vec<Value>> {
    let mut issues=list_labeled_github_issues(token,repository,label).await?;
    if issues.len()<100 {return Ok(issues)}
    let(owner,repo)=repository_parts(repository)?;
    for page in 2..=10 {
        let value=github_get(token,&format!("/repos/{owner}/{repo}/issues?state=open&labels={label}&sort=updated&direction=desc&per_page=100&page={page}")).await?;
        let batch=value.as_array().ok_or_else(||anyhow::anyhow!("issues_unreadable"))?;
        issues.extend(batch.iter().cloned());
        if batch.len()<100 {return Ok(issues)}
    }
    anyhow::bail!("github_issue_limit_exceeded")
}
pub async fn list_recent_github_issues(token: &str, repository: &str) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(
        token,
        &format!("/repos/{owner}/{repo}/issues?state=open&sort=updated&direction=desc&per_page=50"),
    )
    .await
}

/// Login of the account the bearer token authenticates as (the gh CLI account the
/// server is logged in with). Used to scope the resolver to its own assigned work.
pub async fn github_authenticated_login(token: &str) -> Result<String> {
    let me = github_get(token, "/user").await?;
    me.get("login")
        .and_then(|value| value.as_str())
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("github_login_unavailable"))
}

/// Open issues in a repo assigned to `assignee`, most-recently-updated first.
pub async fn list_assigned_open_issues(
    token: &str,
    repository: &str,
    assignee: &str,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(
        token,
        &format!(
            "/repos/{owner}/{repo}/issues?state=open&assignee={assignee}&sort=updated&direction=desc&per_page=50"
        ),
    )
    .await
}

pub async fn list_recent_github_pulls(token: &str, repository: &str) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_get(
        token,
        &format!("/repos/{owner}/{repo}/pulls?state=open&sort=updated&direction=desc&per_page=50"),
    )
    .await
}

/// Merged pull requests of `repository` merged at or after `since` (RFC 3339),
/// newest first, at most `max`. Pages by recent update, which bounds the walk.
pub async fn list_merged_pulls(
    token: &str,
    repository: &str,
    since: &str,
    max: usize,
) -> Result<Vec<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    let mut merged = Vec::new();
    for page in 1..=20 {
        let batch = github_get(
            token,
            &format!(
                "/repos/{owner}/{repo}/pulls?state=closed&sort=updated&direction=desc&per_page=100&page={page}"
            ),
        )
        .await?;
        let items = batch.as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            break;
        }
        let mut older_page = true;
        for pull in items {
            let updated = pull.get("updated_at").and_then(|v| v.as_str()).unwrap_or_default();
            if updated >= since {
                older_page = false;
            }
            let merged_at = pull.get("merged_at").and_then(|v| v.as_str()).unwrap_or_default();
            if !merged_at.is_empty() && merged_at >= since {
                merged.push(pull);
            }
        }
        if older_page || merged.len() >= max {
            break;
        }
    }
    merged.sort_by(|a, b| {
        let at = |v: &Value| v.get("merged_at").and_then(|x| x.as_str()).unwrap_or_default().to_string();
        at(b).cmp(&at(a))
    });
    merged.truncate(max);
    Ok(merged)
}

/// Commits on `branch` between `since` and `until` (RFC 3339). More than 500 is
/// an error, not a partial list: GitHub returns newest first, so a cut would
/// drop exactly the commits right after a merge.
pub async fn list_branch_commits(
    token: &str,
    repository: &str,
    branch: &str,
    since: &str,
    until: &str,
) -> Result<Vec<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    let mut commits = Vec::new();
    for page in 1..=5 {
        let batch = github_get(
            token,
            &format!(
                "/repos/{owner}/{repo}/commits?sha={branch}&since={since}&until={until}&per_page=100&page={page}"
            ),
        )
        .await?;
        let items = batch.as_array().cloned().unwrap_or_default();
        let full = items.len() == 100;
        commits.extend(items);
        if !full {
            return Ok(commits);
        }
    }
    anyhow::bail!("too_many_commits")
}

/// Status check names `branch` requires before merging, from repository
/// rulesets and classic branch protection. None configured is an empty list.
pub async fn required_status_checks(
    token: &str,
    repository: &str,
    branch: &str,
) -> Result<Vec<String>> {
    let (owner, repo) = repository_parts(repository)?;
    let mut required = Vec::new();
    // Private repositories on plans without rulesets answer 403 ("Upgrade to
    // GitHub Pro"), and 404 means none: both mean no ruleset requires a check.
    // A rate-limited 403 cannot hide here: the branch read below fails too.
    let rules = match github_get(token, &format!("/repos/{owner}/{repo}/rules/branches/{branch}")).await {
        Ok(rules) => rules,
        Err(error) if http_status(&error).is_some_and(|s| s == 403 || s == 404) => Value::Array(Vec::new()),
        Err(error) => return Err(error),
    };
    for rule in rules.as_array().into_iter().flatten() {
        if rule.get("type").and_then(|v| v.as_str()) == Some("required_status_checks") {
            for check in rule
                .pointer("/parameters/required_status_checks")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
            {
                if let Some(name) = check.get("context").and_then(|v| v.as_str()) {
                    required.push(name.to_string());
                }
            }
        }
    }
    let branch_info = github_get(token, &format!("/repos/{owner}/{repo}/branches/{branch}")).await?;
    let protection = branch_info.pointer("/protection/required_status_checks");
    for name in protection
        .and_then(|p| p.get("contexts"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
    {
        required.push(name.to_string());
    }
    for name in protection
        .and_then(|p| p.get("checks"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("context").and_then(|v| v.as_str()))
    {
        required.push(name.to_string());
    }
    required.sort();
    required.dedup();
    Ok(required)
}

/// Creates a labelled issue with exactly one POST and no unlabelled retry: a
/// timeout may hide a created issue, which the caller finds by its marker.
pub async fn ensure_github_issue_labels(token:&str,repository:&str,labels:&[String])->Result<()> {
    let (owner,repo)=repository_parts(repository)?;
    for label in labels {ensure_github_label(token,owner,repo,label).await;}
    Ok(())
}

pub async fn post_github_issue_once(token:&str,repository:&str,title:&str,body:&str,labels:&[String])->Result<Value>{
    post_github_issue_once_at("https://api.github.com",token,repository,title,body,labels).await
}

pub(crate) async fn post_github_issue_once_at(base_url:&str,token:&str,repository:&str,title:&str,body:&str,labels:&[String])->Result<Value>{
    let (owner,repo)=repository_parts(repository)?;
    github_post_at(base_url,token,&format!("/repos/{owner}/{repo}/issues"),json!({"title":title,"body":body,"labels":labels})).await
}

pub async fn create_github_issue_once(
    token: &str,
    repository: &str,
    title: &str,
    body: &str,
    labels: &[String],
) -> Result<Value> {
    create_github_issue_once_at("https://api.github.com",token,repository,title,body,labels).await
}

async fn create_github_issue_once_at(base_url:&str,token:&str,repository:&str,title:&str,body:&str,labels:&[String])->Result<Value>{
    ensure_github_issue_labels(token,repository,labels).await?;
    let (owner,repo)=repository_parts(repository)?;
    github_post_at(base_url,token,&format!("/repos/{owner}/{repo}/issues"),json!({"title":title,"body":body,"labels":labels})).await
}

/// A recent open issue whose body carries `marker` (and only that: a matching
/// title is not enough).
pub async fn find_github_issue_with_marker(token: &str, repository: &str, marker: &str) -> Result<Option<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    let value = github_get(
        token,
        &format!("/repos/{owner}/{repo}/issues?state=open&sort=created&direction=desc&per_page=100"),
    )
    .await?;
    Ok(value.as_array().into_iter().flatten().find(|issue| {
        issue.get("pull_request").is_none()
            && issue.get("body").and_then(|b| b.as_str()).is_some_and(|body| body.contains(marker))
    }).cloned())
}

pub async fn find_github_issue_by_marker(token:&str,repository:&str,marker:&str)->Result<Option<Value>> {
    find_github_issue_by_marker_at("https://api.github.com",token,repository,marker).await
}

pub async fn find_github_issues_by_marker(token:&str,repositories:&[String],marker:&str)->Result<Vec<(String,Value)>> {
    find_github_issues_by_marker_at("https://api.github.com",token,repositories,marker).await
}

async fn find_github_issues_by_marker_at(base_url:&str,token:&str,repositories:&[String],marker:&str)->Result<Vec<(String,Value)>> {
    let mut found=Vec::new();
    for repository in repositories {
        if let Some(issue)=find_github_issue_by_marker_at(base_url,token,repository,marker).await? {
            found.push((repository.clone(),issue));
        }
    }
    Ok(found)
}

async fn find_github_issue_by_marker_at(base_url:&str,token:&str,repository:&str,marker:&str)->Result<Option<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    let fingerprint=normalize_fingerprint(&extract_fingerprint(marker));
    // Scan all issue states: a lost POST response can leave a closed issue, and
    // creating a replacement is still a duplicate. Stop rather than publish if
    // the bounded scan cannot prove the marker absent.
    let mut issues=Vec::new();
    for page in 1..=100 {
        let value=github_get_at(base_url,token,&format!("/repos/{owner}/{repo}/issues?state=all&sort=created&direction=desc&per_page=100&page={page}")).await?;
        let Some(rows)=value.as_array() else {anyhow::bail!("github_issue_search_invalid")};
        if let Some(issue)=rows.iter().find(|issue|issue.get("pull_request").is_none() && issue.get("body").and_then(Value::as_str).is_some_and(|body|body.contains(marker) || (!fingerprint.is_empty() && normalize_fingerprint(&extract_fingerprint(body))==fingerprint))) {return Ok(Some(issue.clone()));}
        issues.extend(rows.iter().filter(|issue|issue.get("pull_request").is_none()).cloned());
        if rows.len()<100 {return Ok(find_issue_marker(&json!(issues),marker));}
    }
    anyhow::bail!("github_issue_search_incomplete")
}

pub async fn find_github_pull_by_head(
    token: &str,
    repository: &str,
    branch: &str,
) -> Result<Option<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    if branch.is_empty()
        || branch.contains("..")
        || branch.starts_with('/')
        || !branch
            .chars()
            .all(|value| value.is_ascii_alphanumeric() || matches!(value, '-' | '_' | '/' | '.'))
    {
        anyhow::bail!("invalid_branch")
    }
    let value = github_get(
        token,
        &format!("/repos/{owner}/{repo}/pulls?state=all&head={owner}:{branch}&per_page=100"),
    )
    .await?;
    Ok(value.as_array().and_then(|items| items.first()).cloned())
}

pub async fn find_github_review_by_marker(
    token: &str,
    repository: &str,
    number: i64,
    marker: &str,
) -> Result<Option<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    let value = github_get(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}/reviews?per_page=100"),
    )
    .await?;
    Ok(find_body_marker(&value, marker))
}

/// Pull the fingerprint value out of a `<!-- nexusmind-fingerprint:<fp> -->`
/// marker (or any text that embeds it). Fingerprints are kebab-case and never
/// contain whitespace, so we stop at the first whitespace after the prefix.
fn extract_fingerprint(text: &str) -> String {
    const PREFIX: &str = "nexusmind-fingerprint:";
    match text.find(PREFIX) {
        Some(idx) => text[idx + PREFIX.len()..]
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_string(),
        None => String::new(),
    }
}

/// Case/separator-insensitive fingerprint form so `pos-toast-uuid`,
/// `pos_toast_uuid` and `POS-Toast-UUID` all compare equal.
fn normalize_fingerprint(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

fn find_issue_marker(value: &Value, marker: &str) -> Option<Value> {
    let fingerprint=normalize_fingerprint(&extract_fingerprint(marker));
    value.as_array().and_then(|items|items.iter().find(|item|{
        if item.get("pull_request").is_some(){return false}
        let body=item.get("body").and_then(Value::as_str).unwrap_or_default();
        (!marker.is_empty() && body.contains(marker)) || (!fingerprint.is_empty() && normalize_fingerprint(&extract_fingerprint(body))==fingerprint)
    }).cloned())
}

fn find_body_marker(value: &Value, marker: &str) -> Option<Value> {
    value.as_array().and_then(|items| {
        items
            .iter()
            .find(|item| {
                item.get("body")
                    .and_then(|body| body.as_str())
                    .is_some_and(|body| body.contains(marker))
            })
            .cloned()
    })
}

pub async fn get_github_check_runs(token: &str, repository: &str, sha: &str) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    if sha.len() != 40 || !sha.chars().all(|value| value.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_commit_sha")
    }
    github_get(
        token,
        &format!("/repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100"),
    )
    .await
}

/// Every CI signal on a commit, for merge decisions: all check-run pages plus the
/// combined commit status (Vercel, CircleCI and other non-Checks integrations)
/// converted to check-run shape. Fails closed past `MAX_CHECKS` or on any
/// unreadable page instead of judging a partial list.
pub async fn list_commit_ci_runs(token: &str, repository: &str, sha: &str) -> Result<Vec<Value>> {
    const MAX_CHECKS: usize = 1000;
    let (owner, repo) = repository_parts(repository)?;
    if sha.len() != 40 || !sha.chars().all(|value| value.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_commit_sha")
    }
    let mut runs = Vec::new();
    for page in 1.. {
        let value = github_get(
            token,
            &format!("/repos/{owner}/{repo}/commits/{sha}/check-runs?per_page=100&page={page}"),
        )
        .await?;
        let batch = value
            .get("check_runs")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("check_runs_unreadable"))?
            .clone();
        let last = batch.len() < 100;
        runs.extend(batch);
        if runs.len() > MAX_CHECKS {
            anyhow::bail!("too_many_checks")
        }
        if last {
            break;
        }
    }
    let combined = github_get(
        token,
        &format!("/repos/{owner}/{repo}/commits/{sha}/status?per_page=100"),
    )
    .await?;
    if combined.get("total_count").and_then(|v| v.as_u64()) > Some(100) {
        anyhow::bail!("too_many_checks")
    }
    runs.extend(
        super::merge_gate::statuses_as_runs(&combined).map_err(|reason| anyhow::anyhow!(reason))?,
    );
    Ok(runs)
}

/// Best-effort: ensure a label exists (needs push/triage). Ignored on failure.
/// Color for a label following the QA taxonomy (mirrors the kasymir-create-issues
/// skill so autonomously-filed issues look identical to hand-filed ones).
fn label_color(label: &str) -> &'static str {
    match label {
        "severity:critical" | "security" => "b60205",
        "severity:high" => "d93f0b",
        "severity:medium" => "fbca04",
        "severity:low" => "0e8a16",
        "bug" => "d73a4a",
        "qa" => "1d76db",
        "i18n" => "c5def5",
        "a11y" => "bfdadc",
        "design" => "d4c5f9",
        "ux" => "fef2c0",
        _ if label.starts_with("module:") => "5319e7",
        _ => "ededed",
    }
}

async fn ensure_github_label(token: &str, owner: &str, repo: &str, label: &str) {
    let _ = github_post(
        token,
        &format!("/repos/{owner}/{repo}/labels"),
        json!({"name": label, "color": label_color(label), "description": "NexusMind autonomous QA"}),
    )
    .await;
}

pub async fn create_github_issue(
    token: &str,
    repository: &str,
    title: &str,
    body: &str,
    labels: &[String],
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    // Applying labels needs push/triage access. Ensure each exists (free/idempotent)
    // then create labelled; if the token lacks the permission the labelled create
    // 403s, so fall back to an unlabelled create rather than failing the delivery.
    for label in labels {
        ensure_github_label(token, owner, repo, label).await;
    }
    let labeled = github_post(
        token,
        &format!("/repos/{owner}/{repo}/issues"),
        json!({"title":title,"body":body,"labels":labels}),
    )
    .await;
    match labeled {
        Ok(value) => Ok(value),
        Err(_) => {
            github_post(
                token,
                &format!("/repos/{owner}/{repo}/issues"),
                json!({"title":title,"body":body}),
            )
            .await
        }
    }
}

pub async fn update_github_issue(
    token: &str,
    repository: &str,
    number: i64,
    title: &str,
    body: &str,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_patch(
        token,
        &format!("/repos/{owner}/{repo}/issues/{number}"),
        json!({"title":title,"body":body,"state":"open"}),
    )
    .await
}

/// Close an issue (used by the Judge when it verifies the claim held and no
/// problem remains). Idempotent — closing an already-closed issue is a no-op.
pub async fn close_github_issue(token: &str, repository: &str, number: i64) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_patch(
        token,
        &format!("/repos/{owner}/{repo}/issues/{number}"),
        json!({ "state": "closed" }),
    )
    .await
}

/// Close an issue with an explicit `state_reason` (`completed` | `not_planned`).
/// Used by the resolver when it determines a change is not required, so the
/// closed issue reads as "not planned" rather than delivered work.
pub async fn close_github_issue_with_reason(
    token: &str,
    repository: &str,
    number: i64,
    state_reason: &str,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_patch(
        token,
        &format!("/repos/{owner}/{repo}/issues/{number}"),
        json!({ "state": "closed", "state_reason": state_reason }),
    )
    .await
}

/// Close a pull request (revert action for a draft PR an agent opened).
pub async fn close_github_pull(token: &str, repository: &str, number: i64) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_patch(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}"),
        json!({ "state": "closed" }),
    )
    .await
}

pub(crate) async fn github_delete(token: &str, path: &str) -> Result<()> {
    let response = reqwest::Client::new()
        .delete(format!("https://api.github.com{path}"))
        .bearer_auth(token)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .header("User-Agent", "nexusmind-autonomous-agents")
        .send()
        .await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "github_api_{}: {}",
            status.as_u16(),
            text.chars().take(200).collect::<String>()
        );
    }
    Ok(())
}

/// Delete an issue/PR comment by its id (revert action for a comment an agent
/// posted).
pub async fn delete_issue_comment(token: &str, repository: &str, comment_id: i64) -> Result<()> {
    let (owner, repo) = repository_parts(repository)?;
    github_delete(
        token,
        &format!("/repos/{owner}/{repo}/issues/comments/{comment_id}"),
    )
    .await
}

/// Link an existing issue as a sub-issue of `parent_number` via GitHub's
/// sub-issues REST API. `sub_issue_id` is the child issue's database `id`
/// (NOT its number — that field comes back on the create response).
pub async fn add_sub_issue(
    token: &str,
    repository: &str,
    parent_number: i64,
    sub_issue_id: i64,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/issues/{parent_number}/sub_issues"),
        json!({ "sub_issue_id": sub_issue_id }),
    )
    .await
}

pub async fn create_issue_comment(
    token: &str,
    repository: &str,
    number: i64,
    body: &str,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/issues/{number}/comments"),
        json!({ "body": body }),
    )
    .await
}

/// Assign an issue to the given accounts (added to any existing assignees).
/// Used by "Resolve with agent" to claim the issue for the logged-in bot before
/// work starts, so it reflects that the issue is being resolved.
pub async fn add_issue_assignees(
    token: &str,
    repository: &str,
    number: i64,
    assignees: &[String],
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/issues/{number}/assignees"),
        json!({ "assignees": assignees }),
    )
    .await
}

pub async fn create_draft_pr(
    token: &str,
    repository: &str,
    title: &str,
    head: &str,
    base: &str,
    body: &str,
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/pulls"),
        json!({"title":title,"head":head,"base":base,"body":body,"draft":true}),
    )
    .await
}

pub async fn publish_pr_review(
    token: &str,
    repository: &str,
    number: i64,
    body: &str,
    request_changes: bool,
    commit_id: Option<&str>,
    comments: &[Value],
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    let mut payload =
        json!({"body":body,"event":if request_changes {"REQUEST_CHANGES"} else {"COMMENT"}});
    // Pinning the reviewed head keeps inline line numbers tied to the code the
    // agent actually read, even if the PR moved since.
    if let Some(commit) = commit_id.filter(|c| c.len() == 40 && c.bytes().all(|b| b.is_ascii_hexdigit())) {
        payload["commit_id"] = json!(commit);
    }
    if !comments.is_empty() {
        payload["comments"] = json!(comments);
    }
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}/reviews"),
        payload,
    )
    .await
}

pub async fn send_slack(webhook_url: &str, summary: &str, nexusmind_url: &str) -> Result<()> {
    validate_slack_webhook(webhook_url)?;
    // The webhook URL is the secret: errors never carry it.
    reqwest::Client::new().post(webhook_url).timeout(std::time::Duration::from_secs(15)).json(&json!({
        "text": summary,
        "blocks":[
            {"type":"section","text":{"type":"mrkdwn","text":summary.chars().take(2500).collect::<String>()}},
            {"type":"actions","elements":[{"type":"button","text":{"type":"plain_text","text":"Open in NexusMind"},"url":nexusmind_url}]}
        ]
    })).send().await.map_err(|e| anyhow::anyhow!("slack_send_failed: {}", e.without_url()))?
    .error_for_status().map_err(|e| anyhow::anyhow!("slack_send_failed: {}", e.without_url()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repository_parser_rejects_urls_and_path_widening() {
        assert!(repository_parts("acme/api").is_ok());
        assert!(repository_parts("https://github.com/acme/api").is_err());
        assert!(repository_parts("acme/api/extra").is_err());
    }

    #[test]
    fn slack_destination_requires_the_exact_https_host_and_services_path() {
        assert!(validate_slack_webhook("https://hooks.slack.com/services/T/B/X").is_ok());
        assert!(validate_slack_webhook("https://hooks.slack.com.evil/services/T/B/X").is_err());
        assert!(validate_slack_webhook("http://hooks.slack.com/services/T/B/X").is_err());
        assert!(validate_slack_webhook("https://user@hooks.slack.com/services/T/B/X").is_err());
        assert!(validate_slack_webhook("https://hooks.slack.com/not-services/X").is_err());
    }

    #[tokio::test]
    async fn github_issue_recovery_scans_http_pages_and_finds_closed_marker_once() {
        use axum::{extract::Query,http::HeaderMap,routing::get,Json,Router};
        use std::{collections::HashMap,sync::{Arc,Mutex}};
        let pages=Arc::new(Mutex::new(Vec::new()));
        let observed=pages.clone();
        let marker="<!-- nexusmind-fingerprint:checkout-total -->".to_string();
        let response_marker=marker.clone();
        let app=Router::new().route("/repos/acme/api/issues",get(move |headers:HeaderMap,Query(query):Query<HashMap<String,String>>|{
            let observed=observed.clone();let response_marker=response_marker.clone();
            async move {
                assert_eq!(headers.get("authorization").and_then(|value|value.to_str().ok()),Some("Bearer test-token"));
                assert_eq!(query.get("state").map(String::as_str),Some("all"));
                assert_eq!(query.get("per_page").map(String::as_str),Some("100"));
                let page=query.get("page").cloned().unwrap_or_default();
                observed.lock().unwrap().push(page.clone());
                if page=="1" {Json(json!((0..100).map(|number|json!({"number":number,"title":"unrelated","body":"no marker","state":"open"})).collect::<Vec<_>>()))}
                else if page=="2" {Json(json!([{"number":101,"title":"old issue","body":response_marker,"state":"closed"}]))}
                else {Json(json!([]))}
            }
        }));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
        let found=find_github_issue_by_marker_at(&base,"test-token","acme/api",&marker).await.unwrap().unwrap();
        assert_eq!(found["number"],101);
        assert_eq!(found["state"],"closed");
        assert_eq!(*pages.lock().unwrap(),vec!["1".to_string(),"2".to_string()]);
        pages.lock().unwrap().clear();
        let missing=find_github_issue_by_marker_at(&base,"test-token","acme/api","missing-marker").await.unwrap();
        assert!(missing.is_none(),"unexpected issue for missing marker: {missing:?}");
        assert_eq!(*pages.lock().unwrap(),vec!["1".to_string(),"2".to_string()]);
        server.abort();
    }

    #[tokio::test]
    async fn marker_search_covers_all_authorized_repository_candidates() {
        use axum::{extract::{Path,Query},routing::get,Json,Router};
        use std::collections::HashMap;
        let marker="<!-- nexusmind-fingerprint:shared-responsibility -->";
        let app=Router::new().route("/repos/:owner/:repo/issues",get(move |Path((owner,repo)):Path<(String,String)>,Query(_query):Query<HashMap<String,String>>|async move{
            if owner=="acme"&&repo=="web" {Json(json!([{"number":8,"html_url":"https://github.com/acme/web/issues/8","body":marker,"state":"closed"}]))}
            else {Json(json!([]))}
        }));
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let server=tokio::spawn(async move{axum::serve(listener,app).await.unwrap()});
        let candidates=vec!["acme/api".to_string(),"acme/web".to_string()];
        let found=find_github_issues_by_marker_at(&base,"test-token",&candidates,marker).await.unwrap();
        assert_eq!(found.len(),1);
        assert_eq!(found[0].0,"acme/web");
        assert_eq!(found[0].1["state"],"closed");
        server.abort();
    }

    #[tokio::test]
    async fn lost_github_issue_response_is_recovered_without_a_second_post() {
        use axum::{extract::State,http::{HeaderMap,StatusCode},routing::get,Json,Router};
        use std::sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}};
        #[derive(Clone)] struct Mock {issues:Arc<Mutex<Vec<Value>>>,posts:Arc<AtomicUsize>}
        let state=Mock{issues:Arc::new(Mutex::new(Vec::new())),posts:Arc::new(AtomicUsize::new(0))};
        let app=Router::new().route("/repos/acme/api/issues",get(|State(state):State<Mock>|async move{Json(state.issues.lock().unwrap().clone())}).post(|State(state):State<Mock>,headers:HeaderMap,Json(body):Json<Value>|async move{
            assert_eq!(headers.get("authorization").and_then(|value|value.to_str().ok()),Some("Bearer test-token"));
            state.posts.fetch_add(1,Ordering::SeqCst);
            let issue=json!({"number":5,"state":"closed","body":body["body"]});
            state.issues.lock().unwrap().push(issue);
            StatusCode::INTERNAL_SERVER_ERROR
        })).with_state(state.clone());
        let listener=tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base=format!("http://{}",listener.local_addr().unwrap());
        let server=tokio::spawn(async move {axum::serve(listener,app).await.unwrap()});
        let marker="<!-- nexusmind-fingerprint:lost-response -->";
        assert!(create_github_issue_once_at(&base,"test-token","acme/api","same title",marker,&[]).await.is_err());
        let recovered=find_github_issue_by_marker_at(&base,"test-token","acme/api",marker).await.unwrap().unwrap();
        assert_eq!(recovered["number"],5);
        assert_eq!(recovered["state"],"closed");
        assert_eq!(state.posts.load(Ordering::SeqCst),1);
        server.abort();
    }

    #[test]
    fn ambiguous_github_writes_reconcile_only_the_exact_marker() {
        let marker = "<!-- nexusmind-fingerprint:abc -->";
        let issues = json!([
            {"number":1,"body":"unrelated"},
            {"number":2,"body":marker},
            {"number":3,"body":marker,"pull_request":{}}
        ]);
        assert_eq!(
            find_issue_marker(&issues, marker).and_then(|value| value.get("number").cloned()),
            Some(json!(2))
        );
        assert!(find_issue_marker(&issues, "missing").is_none());
        assert!(find_issue_marker(&json!([{"number":7,"title":"[NexusMind QA] POS toast"}]),"missing").is_none());
        assert_eq!(
            find_body_marker(&json!([{"id":9,"body":"NexusMind run: `run-1`"}]), "run-1")
                .and_then(|value| value.get("id").cloned()),
            Some(json!(9))
        );
    }

    #[test]
    fn find_issue_marker_requires_a_normalized_fingerprint_match() {
        // Fingerprint drifted only by case/separator → still the same defect.
        let issues = json!([
            {"number":11,"title":"[POS] toast","body":"x\n<!-- nexusmind-fingerprint:POS_Toast_UUID -->"}
        ]);
        assert_eq!(
            find_issue_marker(
                &issues,
                "<!-- nexusmind-fingerprint:pos-toast-uuid -->",
            )
            .and_then(|value| value.get("number").cloned()),
            Some(json!(11)),
        );

        // Similar titles without the stable marker are different findings.
        let issues = json!([
            {"number":12,"title":"[Users] Archive user is a no-op","body":"no marker here"}
        ]);
        assert!(find_issue_marker(&issues,"missing").is_none());

        // Similar titles in any module are not enough evidence to merge.
        let issues = json!([
            {"number":13,"title":"[Billing] Total wrong for 2 items","body":""},
            {"number":14,"title":"[Cart] Archive user is a no-op","body":""}
        ]);
        assert!(find_issue_marker(&issues, "missing").is_none());
        assert!(find_issue_marker(&issues, "missing").is_none());
    }
}
