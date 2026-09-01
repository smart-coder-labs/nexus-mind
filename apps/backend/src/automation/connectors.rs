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

async fn github_post(token: &str, path: &str, body: Value) -> Result<Value> {
    let response = reqwest::Client::new()
        .post(format!("https://api.github.com{path}"))
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

async fn github_patch(token: &str, path: &str, body: Value) -> Result<Value> {
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

async fn github_get(token: &str, path: &str) -> Result<Value> {
    Ok(reqwest::Client::new()
        .get(format!("https://api.github.com{path}"))
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
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_put(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}/merge"),
        json!({ "merge_method": method }),
    )
    .await
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

pub async fn find_github_issue_by_marker(
    token: &str,
    repository: &str,
    marker: &str,
    title: &str,
) -> Result<Option<Value>> {
    let (owner, repo) = repository_parts(repository)?;
    // Do NOT require the `nexusmind-qa` label: issues created while the bot lacked
    // label permission are unlabelled and would be invisible, causing duplicates.
    // Scan recent OPEN issues and match on the fingerprint marker OR an identical
    // title so a same-bug finding whose fingerprint drifted still dedupes.
    let value = github_get(
        token,
        &format!("/repos/{owner}/{repo}/issues?state=open&sort=created&direction=desc&per_page=100"),
    )
    .await?;
    Ok(find_issue_marker(&value, marker, title))
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

/// Lowercase alphanumeric title with runs of punctuation/space collapsed to a
/// single space — the comparison form for near-duplicate title detection.
fn normalize_title(text: &str) -> String {
    let mut out = String::new();
    let mut prev_space = true;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            out.extend(ch.to_lowercase());
            prev_space = false;
        } else if !prev_space {
            out.push(' ');
            prev_space = true;
        }
    }
    out.trim().to_string()
}

/// Leading `[Module]` tag (lowercased) if present, else empty. Two titles only
/// dedupe by similarity when their module tag matches, so a lookalike symptom
/// in a different module is never merged.
fn module_prefix(text: &str) -> String {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        if let Some(end) = trimmed.find(']') {
            return trimmed[..=end].to_lowercase();
        }
    }
    String::new()
}

/// Digit runs in order — distinct bugs often differ only by a number
/// ("2 items" vs "3 items"), so these must match for a title-similarity merge.
fn digit_tokens(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_digit())
        .filter(|token| !token.is_empty())
        .map(str::to_string)
        .collect()
}

/// Token-coverage similarity: the fraction of the SHORTER title's words that
/// also appear in the other title (multiset intersection over the smaller word
/// count). Catches "same title + a few extra words" drift that character-level
/// metrics miss, while the module + number guards upstream keep distinct
/// defects apart. 1.0 == one title's words are all contained in the other.
fn title_similarity(left: &str, right: &str) -> f64 {
    let tokens = |value: &str| value.split_whitespace().map(str::to_string).collect::<Vec<_>>();
    let left_tokens = tokens(left);
    let mut right_tokens = tokens(right);
    let denom = left_tokens.len().min(right_tokens.len());
    if denom == 0 {
        return 0.0;
    }
    let mut overlap = 0usize;
    for token in &left_tokens {
        if let Some(pos) = right_tokens.iter().position(|other| other == token) {
            right_tokens.remove(pos);
            overlap += 1;
        }
    }
    overlap as f64 / denom as f64
}

/// Minimum title coverage to treat two issues as the same defect when the
/// fingerprint drifted. High on purpose — this is a last-resort dedup.
const TITLE_DEDUP_THRESHOLD: f64 = 0.9;

/// Titles shorter than this (in words) are too generic to dedupe by coverage
/// alone, so a title match requires at least this many words on the shorter side.
const TITLE_DEDUP_MIN_WORDS: usize = 3;

fn find_issue_marker(value: &Value, marker: &str, title: &str) -> Option<Value> {
    // Dedup an incoming finding against recent OPEN issues through three ladders,
    // most precise first: (1) the exact fingerprint marker in the body; (2) the
    // same fingerprint after case/separator normalization; (3) a near-identical
    // title (same [Module], same numbers) when the fingerprint drifted entirely.
    let want_fingerprint = normalize_fingerprint(&extract_fingerprint(marker));
    let want_title = normalize_title(title);
    let want_module = module_prefix(title);
    let want_digits = digit_tokens(&want_title);
    value.as_array().and_then(|items| {
        items
            .iter()
            .find(|item| {
                if item.get("pull_request").is_some() {
                    return false;
                }
                let body = item.get("body").and_then(Value::as_str).unwrap_or_default();
                if !marker.is_empty() && body.contains(marker) {
                    return true;
                }
                if !want_fingerprint.is_empty() {
                    let their_fingerprint =
                        normalize_fingerprint(&extract_fingerprint(body));
                    if !their_fingerprint.is_empty() && their_fingerprint == want_fingerprint {
                        return true;
                    }
                }
                if !want_title.is_empty() {
                    let raw_title = item.get("title").and_then(Value::as_str).unwrap_or_default();
                    let their_title = normalize_title(raw_title);
                    let shorter_words = want_title
                        .split_whitespace()
                        .count()
                        .min(their_title.split_whitespace().count());
                    if !their_title.is_empty()
                        && shorter_words >= TITLE_DEDUP_MIN_WORDS
                        && module_prefix(raw_title) == want_module
                        && digit_tokens(&their_title) == want_digits
                        && title_similarity(&want_title, &their_title) >= TITLE_DEDUP_THRESHOLD
                    {
                        return true;
                    }
                }
                false
            })
            .cloned()
    })
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

async fn github_delete(token: &str, path: &str) -> Result<()> {
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
) -> Result<Value> {
    let (owner, repo) = repository_parts(repository)?;
    github_post(
        token,
        &format!("/repos/{owner}/{repo}/pulls/{number}/reviews"),
        json!({"body":body,"event":if request_changes {"REQUEST_CHANGES"} else {"COMMENT"}}),
    )
    .await
}

pub async fn send_slack(webhook_url: &str, summary: &str, nexusmind_url: &str) -> Result<()> {
    validate_slack_webhook(webhook_url)?;
    reqwest::Client::new().post(webhook_url).json(&json!({
        "text": summary,
        "blocks":[
            {"type":"section","text":{"type":"mrkdwn","text":summary.chars().take(2500).collect::<String>()}},
            {"type":"actions","elements":[{"type":"button","text":{"type":"plain_text","text":"Open in NexusMind"},"url":nexusmind_url}]}
        ]
    })).send().await?.error_for_status()?;
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

    #[test]
    fn ambiguous_github_writes_reconcile_only_the_exact_marker() {
        let marker = "<!-- nexusmind-fingerprint:abc -->";
        let issues = json!([
            {"number":1,"body":"unrelated"},
            {"number":2,"body":marker},
            {"number":3,"body":marker,"pull_request":{}}
        ]);
        assert_eq!(
            find_issue_marker(&issues, marker, "").and_then(|value| value.get("number").cloned()),
            Some(json!(2))
        );
        assert!(find_issue_marker(&issues, "missing", "").is_none());
        // Title match dedupes even when the fingerprint marker drifted.
        assert_eq!(
            find_issue_marker(
                &json!([{"number":7,"title":"[NexusMind QA] POS toast"}]),
                "missing",
                "[NexusMind QA] POS toast",
            )
            .and_then(|value| value.get("number").cloned()),
            Some(json!(7))
        );
        assert_eq!(
            find_body_marker(&json!([{"id":9,"body":"NexusMind run: `run-1`"}]), "run-1")
                .and_then(|value| value.get("id").cloned()),
            Some(json!(9))
        );
    }

    #[test]
    fn find_issue_marker_dedupes_on_normalized_fingerprint_and_near_title() {
        // Fingerprint drifted only by case/separator → still the same defect.
        let issues = json!([
            {"number":11,"title":"[POS] toast","body":"x\n<!-- nexusmind-fingerprint:POS_Toast_UUID -->"}
        ]);
        assert_eq!(
            find_issue_marker(
                &issues,
                "<!-- nexusmind-fingerprint:pos-toast-uuid -->",
                "[POS] something else entirely",
            )
            .and_then(|value| value.get("number").cloned()),
            Some(json!(11)),
        );

        // Same [Module], title reworded a little, fingerprint gone → title match.
        let issues = json!([
            {"number":12,"title":"[Users] Archive user is a no-op","body":"no marker here"}
        ]);
        assert_eq!(
            find_issue_marker(
                &issues,
                "missing",
                "[Users] Archive user is a no-op — no status sent",
            )
            .and_then(|value| value.get("number").cloned()),
            Some(json!(12)),
        );

        // Guardrails: different module, or a differing number, must NOT merge.
        let issues = json!([
            {"number":13,"title":"[Billing] Total wrong for 2 items","body":""},
            {"number":14,"title":"[Cart] Archive user is a no-op","body":""}
        ]);
        assert!(find_issue_marker(&issues, "missing", "[Billing] Total wrong for 3 items").is_none());
        assert!(find_issue_marker(&issues, "missing", "[Orders] Total wrong for 2 items").is_none());
    }
}
