//! Intake: turns external work items into validated `TaskSpec`s (plan D13).
//!
//! Each source is a thin async fetcher over a pure normalizer. Normalizers decide
//! everything that matters — opt-in, trust, identity, task class — so they are
//! what the tests pin down. Intake does not persist or route; the router (F3)
//! consumes the specs. The target repository always comes from the control-plane
//! configuration of the source, never from the item itself.

use axum::async_trait;
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use crate::factory::contracts::{
    Contract, OriginTrust, PrivacyClass, SchemaV1, SourceKind, TaskClass, TaskRepository,
    TaskSource, TaskSpec,
};
use crate::factory::redact::Redactor;
use crate::models::types::Task;

/// Only items carrying this label enter the factory.
pub const INTAKE_LABEL: &str = "factory";
/// Label on factory tasks whose text came from an untrusted source (Slack,
/// Sentry). Such a task stays untrusted wherever it is picked up again.
pub const UNTRUSTED_LABEL: &str = "untrusted";
/// Marker in the body of a GitHub issue the factory opened from untrusted intake.
pub const INTAKE_ISSUE_MARKER: &str = "nexusmind-factory-intake:";

/// Contract limit on `TaskSpec.description`.
const MAX_DESCRIPTION_CHARS: usize = 65_536;

/// Where a source's items apply. Configured in the control plane.
#[derive(Clone, Debug, PartialEq)]
pub struct IntakeTarget {
    pub repository: String,
    pub base_ref: String,
    pub privacy_class: PrivacyClass,
}

#[async_trait]
pub trait IntakeSource: Send + Sync {
    fn kind(&self) -> SourceKind;
    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>>;
}

/// A stable, name-based id: the same source item always yields the same task id,
/// so the router can deduplicate re-fetches without storage.
pub fn stable_task_id(source_ref: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(source_ref.as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    // Name-based UUID layout (version/variant bits set), lowercase hyphenated.
    uuid::Builder::from_sha1_bytes(bytes)
        .into_uuid()
        .hyphenated()
        .to_string()
}

/// Task class from labels. When labels conflict the most sensitive class wins, so
/// a `security` + `docs` item is never treated as docs.
pub fn classify_task(labels: &[String]) -> TaskClass {
    const BY_SENSITIVITY: [(TaskClass, &[&str]); 9] = [
        (TaskClass::Security, &["security", "vulnerability", "auth"]),
        (TaskClass::Migration, &["migration", "database", "db"]),
        (
            TaskClass::Infra,
            &["infra", "infrastructure", "ci", "devops", "deploy"],
        ),
        (TaskClass::Bugfix, &["bug", "bugfix", "defect"]),
        (TaskClass::Backend, &["backend", "api", "server"]),
        (TaskClass::Ui, &["ui", "frontend", "design"]),
        (TaskClass::Refactor, &["refactor", "refactoring"]),
        (TaskClass::Tests, &["tests", "test", "testing", "qa"]),
        (TaskClass::Docs, &["docs", "documentation", "doc"]),
    ];
    let normalized: Vec<String> = labels.iter().map(|l| l.trim().to_lowercase()).collect();
    BY_SENSITIVITY
        .iter()
        .find(|(_, names)| {
            normalized
                .iter()
                .any(|label| names.contains(&label.as_str()))
        })
        .map_or(TaskClass::Unknown, |(class, _)| *class)
}

/// Markdown checklist items (`- [ ] …` / `- [x] …`) of a description.
pub fn acceptance_criteria(description: &str) -> Vec<String> {
    description
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            ["- [ ] ", "- [x] ", "- [X] "]
                .iter()
                .find_map(|marker| line.strip_prefix(marker))
        })
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
        .collect()
}

fn truncate_description(text: &str) -> String {
    text.chars().take(MAX_DESCRIPTION_CHARS).collect()
}

fn has_intake_label(labels: &[String]) -> bool {
    labels
        .iter()
        .any(|label| label.eq_ignore_ascii_case(INTAKE_LABEL))
}

#[allow(clippy::too_many_arguments)]
fn build_spec(
    source: TaskSource,
    source_key: &str,
    origin_trust: OriginTrust,
    target: &IntakeTarget,
    title: &str,
    description: &str,
    labels: &[String],
    now: DateTime<Utc>,
) -> Result<TaskSpec, String> {
    let spec = TaskSpec {
        schema_version: SchemaV1,
        task_id: stable_task_id(source_key),
        source,
        origin_trust,
        privacy_class: target.privacy_class,
        repository: TaskRepository {
            remote: target.repository.clone(),
            base_ref: target.base_ref.clone(),
        },
        title: title.trim().to_string(),
        description: truncate_description(description),
        task_class: classify_task(labels),
        requirements: Vec::new(),
        acceptance_criteria: acceptance_criteria(description),
        created_at: now,
    };
    spec.validate()?;
    Ok(spec)
}

/// `Ok(None)`: not for the factory (no label, or a pull request). `Err`: malformed.
pub fn github_issue_to_task_spec(
    issue: &Value,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    if issue.get("pull_request").is_some() {
        return Ok(None);
    }
    let labels: Vec<String> = issue
        .get("labels")
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.get("name")
                        .and_then(|n| n.as_str())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    if !has_intake_label(&labels) {
        return Ok(None);
    }
    let number = issue
        .get("number")
        .and_then(|v| v.as_i64())
        .ok_or("issue_number_missing")?;
    let title = issue
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let body = issue
        .get("body")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    // Anyone can open an issue on a public repository: only people GitHub itself
    // associates with the repository are trusted. A missing field is untrusted.
    // An issue the factory opened from Slack or Sentry carries untrusted text
    // even though the bot (a member) authored it.
    let trust = match issue.get("author_association").and_then(|v| v.as_str()) {
        Some("OWNER" | "MEMBER" | "COLLABORATOR") if !body.contains(INTAKE_ISSUE_MARKER) => OriginTrust::Trusted,
        _ => OriginTrust::Untrusted,
    };
    let reference = format!("{}#{number}", target.repository);
    let source = TaskSource {
        kind: SourceKind::GithubIssue,
        reference: reference.clone(),
        url: issue
            .get("html_url")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    };
    build_spec(
        source,
        &format!("github_issue:{reference}"),
        trust,
        target,
        title,
        body,
        &labels,
        now,
    )
    .map(Some)
}

/// NexusMind tasks are authored by authenticated org members, so they are trusted,
/// unless the factory created them from untrusted intake (`untrusted` label).
/// Only unstarted tasks (`backlog`, `todo`) enter.
pub fn nexusmind_task_to_task_spec(
    task: &Task,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    // Only unstarted work: a task someone is already working on (or that is
    // closed) is never picked up, whatever its labels.
    if !matches!(task.status.as_str(), "backlog" | "todo") || !has_intake_label(&task.labels) {
        return Ok(None);
    }
    let source = TaskSource {
        kind: SourceKind::NexusmindTask,
        reference: task.id.clone(),
        url: None,
    };
    build_spec(
        source,
        &format!("nexusmind_task:{}", task.id),
        if task.labels.iter().any(|l| l.eq_ignore_ascii_case(UNTRUSTED_LABEL)) {
            OriginTrust::Untrusted
        } else {
            OriginTrust::Trusted
        },
        target,
        &task.title,
        task.description.as_deref().unwrap_or_default(),
        &task.labels,
        now,
    )
    .map(Some)
}

/// GitHub issues labeled `factory` in the target repository.
pub struct GithubIssueIntake {
    pub token: String,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for GithubIssueIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::GithubIssue
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        let issues = crate::automation::connectors::list_labeled_github_issues(
            &self.token,
            &self.target.repository,
            INTAKE_LABEL,
        )
        .await?;
        let now = Utc::now();
        let mut specs = Vec::new();
        for issue in &issues {
            match github_issue_to_task_spec(issue, &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(
                    repository = %self.target.repository,
                    "Skipping malformed issue in intake: {reason}"
                ),
            }
        }
        Ok(specs)
    }
}

/// Unstarted (`backlog`/`todo`) NexusMind tasks of one project labeled `factory`.
pub struct NexusmindTaskIntake {
    pub store: crate::store::sqlite::SqliteStore,
    pub org_id: String,
    pub project: String,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for NexusmindTaskIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::NexusmindTask
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        // Paginate the project's unarchived tasks and filter in code: the SQL label
        // filter is case-sensitive while the normalizer (like GitHub) is not, and a
        // capped single page could let closed tasks crowd out open ones.
        const PAGE: i64 = 200;
        let mut tasks = Vec::new();
        for page in 0.. {
            let batch = {
                let db = self.store.conn();
                let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
                crate::db::queries::list_tasks(
                    &conn,
                    &self.org_id,
                    None,
                    &crate::db::queries::TaskListFilters {
                        project: Some(self.project.clone()),
                        ..Default::default()
                    },
                    PAGE,
                    page * PAGE,
                )?
            };
            let last = (batch.len() as i64) < PAGE;
            tasks.extend(batch);
            if last {
                break;
            }
        }
        let now = Utc::now();
        let mut specs = Vec::new();
        for task in &tasks {
            match nexusmind_task_to_task_spec(task, &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(task = %task.id, "Skipping task in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Slack (F3)

/// The reaction that hands a Slack message to the factory, when an allowed
/// person adds it.
pub const SLACK_INTAKE_REACTION: &str = "factory";

/// Slack escapes `&`, `<` and `>` in message text.
fn slack_unescape(text: &str) -> String {
    text.replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// A top-level, human Slack message that an allowed person flagged with the
/// `factory` reaction. Anyone in a channel can write a message, so the content
/// is untrusted (plan D13) and never picks its own task class: it stays
/// `Unknown`, which no auto-merge allowlist includes. An empty `allowed_reactors`
/// flags nothing. `Ok(None)`: not for the factory. `Err`: malformed.
pub fn slack_message_to_task_spec(
    message: &Value,
    channel_id: &str,
    allowed_reactors: &[String],
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    let ts = message.get("ts").and_then(|v| v.as_str()).ok_or("slack_ts_missing")?;
    // A plain message or one with files; every other subtype is a bot or the system.
    let human = message.get("bot_id").is_none()
        && matches!(
            message.get("subtype").and_then(|v| v.as_str()),
            None | Some("file_share")
        );
    let is_reply = message
        .get("thread_ts")
        .and_then(|v| v.as_str())
        .is_some_and(|thread| thread != ts);
    let flagged = message
        .get("reactions")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|r| r.get("name").and_then(|n| n.as_str()) == Some(SLACK_INTAKE_REACTION))
        .flat_map(|r| r.get("users").and_then(|u| u.as_array()).into_iter().flatten())
        .filter_map(|user| user.as_str())
        .any(|user| allowed_reactors.iter().any(|allowed| allowed == user));
    if !human || is_reply || !flagged {
        return Ok(None);
    }
    let text = slack_unescape(
        message.get("text").and_then(|v| v.as_str()).unwrap_or_default().trim(),
    );
    let title: String = text.lines().next().unwrap_or_default().chars().take(200).collect();
    if title.trim().is_empty() {
        return Err("slack_text_empty".into());
    }
    let reference = format!("{channel_id}:{ts}");
    let source = TaskSource { kind: SourceKind::Slack, reference: reference.clone(), url: None };
    build_spec(
        source,
        &format!("slack:{reference}->{}", target.repository),
        OriginTrust::Untrusted,
        target,
        &title,
        &text,
        &[],
        now,
    )
    .map(Some)
}

/// Slack messages of one channel flagged `:factory:` by an allowed person.
/// Reads the most recent page of history (up to 200 messages).
pub struct SlackIntake {
    pub token: String,
    pub channel_id: String,
    /// Slack user ids whose `:factory:` reaction counts.
    pub allowed_reactors: Vec<String>,
    pub target: IntakeTarget,
}

/// Slack channel ids are uppercase alphanumerics (`C0123ABC`).
pub fn valid_slack_channel(channel_id: &str) -> bool {
    (8..=20).contains(&channel_id.len())
        && channel_id.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

#[async_trait]
impl IntakeSource for SlackIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::Slack
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        if !valid_slack_channel(&self.channel_id) {
            anyhow::bail!("invalid_slack_channel");
        }
        let body: Value = reqwest::Client::new()
            .get("https://slack.com/api/conversations.history")
            .bearer_auth(&self.token)
            .query(&[("channel", self.channel_id.as_str()), ("limit", "200")])
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
            let error = body.get("error").and_then(|v| v.as_str()).unwrap_or("unknown");
            anyhow::bail!("slack_error:{error}");
        }
        let messages = body
            .get("messages")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("slack_messages_unreadable"))?;
        let now = Utc::now();
        let mut specs = Vec::new();
        for message in messages {
            match slack_message_to_task_spec(
                message,
                &self.channel_id,
                &self.allowed_reactors,
                &self.target,
                now,
            ) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(channel = %self.channel_id, "Skipping Slack message in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Sentry (F3)

/// A Sentry field that is a string or a number, as text.
fn sentry_number(issue: &Value, name: &str) -> String {
    match issue.get(name) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => "?".into(),
    }
}

/// An unresolved Sentry issue at error or fatal level becomes a bugfix task.
/// Its text comes from runtime errors (possibly user input), so it is always
/// untrusted. `instance` names the Sentry host and org (`sentry.io/acme`):
/// issue ids are only unique within one. `Ok(None)`: not for the factory.
/// `Err`: malformed.
pub fn sentry_issue_to_task_spec(
    issue: &Value,
    instance: &str,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    let id = issue.get("id").and_then(|v| v.as_str()).ok_or("sentry_id_missing")?;
    let unresolved = issue.get("status").and_then(|v| v.as_str()) == Some("unresolved");
    let severe = matches!(issue.get("level").and_then(|v| v.as_str()), Some("error" | "fatal"));
    if !unresolved || !severe {
        return Ok(None);
    }
    let field = |name: &str| issue.get(name).and_then(|v| v.as_str()).unwrap_or_default();
    let title: String = field("title").chars().take(200).collect();
    let description = format!(
        "Sentry issue {}\n\nCulprit: {}\nEvents: {} · users affected: {}\nFirst seen: {} · last seen: {}\n{}",
        field("shortId"),
        field("culprit"),
        sentry_number(issue, "count"),
        sentry_number(issue, "userCount"),
        field("firstSeen"),
        field("lastSeen"),
        field("permalink"),
    );
    let reference = format!("{instance}:{id}");
    let source = TaskSource {
        kind: SourceKind::Sentry,
        reference: reference.clone(),
        url: Some(field("permalink").to_string()).filter(|u| u.starts_with("https://")),
    };
    build_spec(
        source,
        &format!("sentry:{reference}->{}", target.repository),
        OriginTrust::Untrusted,
        target,
        &title,
        &description,
        &["bug".to_string()],
        now,
    )
    .map(Some)
}

/// Sentry org and project slugs: lowercase alphanumerics, `-` and `_`.
pub fn valid_sentry_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 100
        && slug.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The host of a Sentry base URL the token may be sent to: https, a DNS name
/// (no IP literal, no localhost), no credentials, path, query or fragment.
pub fn sentry_base_host(base_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(base_url).ok()?;
    // `host_str` of an IP literal parses back as an IP; a domain does not.
    let host = url.host_str()?.to_ascii_lowercase();
    if host.starts_with('[') || host.parse::<std::net::IpAddr>().is_ok() {
        return None;
    }
    let clean = url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && matches!(url.path(), "" | "/")
        && url.query().is_none()
        && url.fragment().is_none()
        && url.port().is_none()
        && host != "localhost"
        && !host.ends_with(".localhost")
        && host.contains('.');
    clean.then_some(host)
}

/// Issues of one Sentry project matching an explicit query. There is no default
/// query: the query is the opt-in (e.g. `is:unresolved level:[error,fatal]
/// tag:factory:yes`), so a noisy project does not flood the factory. Reads the
/// first page of results.
pub struct SentryIntake {
    pub token: String,
    /// `https://sentry.io` or a self-hosted base URL.
    pub base_url: String,
    pub org_slug: String,
    pub project_slug: String,
    pub query: String,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for SentryIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::Sentry
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        if !valid_sentry_slug(&self.org_slug) || !valid_sentry_slug(&self.project_slug) {
            anyhow::bail!("invalid_sentry_slug");
        }
        let host = sentry_base_host(&self.base_url)
            .ok_or_else(|| anyhow::anyhow!("invalid_sentry_base_url"))?;
        if self.query.trim().is_empty() {
            anyhow::bail!("sentry_query_required");
        }
        let url = format!(
            "https://{host}/api/0/projects/{}/{}/issues/",
            self.org_slug, self.project_slug
        );
        let body: Value = reqwest::Client::new()
            .get(url)
            .bearer_auth(&self.token)
            .query(&[("query", self.query.as_str())])
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let issues = body
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("sentry_issues_unreadable"))?;
        let instance = format!("{host}/{}", self.org_slug);
        let now = Utc::now();
        let mut specs = Vec::new();
        for issue in issues {
            match sentry_issue_to_task_spec(issue, &instance, &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(project = %self.project_slug, "Skipping Sentry issue in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Gmail (F4)

const GMAIL_API: &str = "https://gmail.googleapis.com/gmail/v1/users/me";
/// Messages fetched per poll; the rest wait for the next one.
const GMAIL_PAGE: usize = 50;

fn gmail_decode_base64url(data: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(data)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .unwrap_or_default()
}

/// The `text/plain` body of a Gmail message payload, walking `parts` for a
/// multipart message. Other MIME types (`text/html`, attachments) are ignored:
/// only plain text ever reaches a model.
fn gmail_plain_text(payload: &Value) -> String {
    if payload.get("mimeType").and_then(|v| v.as_str()) == Some("text/plain") {
        if let Some(data) = payload.pointer("/body/data").and_then(|v| v.as_str()) {
            return gmail_decode_base64url(data);
        }
    }
    payload
        .get("parts")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .map(gmail_plain_text)
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn gmail_header(payload: &Value, name: &str) -> String {
    payload
        .get("headers")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .find(|header| {
            header
                .get("name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(name))
        })
        .and_then(|header| header.get("value"))
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

/// A Gmail message resource (`messages.get`, `format=full`) carrying the
/// `factory` label, resolved to its id by the fetcher since messages name
/// labels by id, not by name (plan D13). Its text is always untrusted, and the
/// task it becomes is always `fix: manual` regardless of the decision model
/// (`floor_source_kind` in `automation::factory_intake`). `Ok(None)`: not
/// labelled. `Err`: malformed.
pub fn gmail_message_to_task_spec(
    message: &Value,
    factory_label_id: &str,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    let labelled = message
        .get("labelIds")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str())
        .any(|id| id == factory_label_id);
    if !labelled {
        return Ok(None);
    }
    let id = message.get("id").and_then(|v| v.as_str()).ok_or("gmail_id_missing")?;
    let payload = message.get("payload").ok_or("gmail_payload_missing")?;
    let mut redactor = Redactor::new();
    let title = redactor.apply(&gmail_header(payload, "Subject"));
    let description = redactor.apply(&gmail_plain_text(payload));
    if title.trim().is_empty() {
        return Err("gmail_subject_missing".into());
    }
    let reference = id.to_string();
    let source = TaskSource { kind: SourceKind::Gmail, reference: reference.clone(), url: None };
    build_spec(
        source,
        &format!("gmail:{reference}->{}", target.repository),
        OriginTrust::Untrusted,
        target,
        &title,
        &description,
        &[],
        now,
    )
    .map(Some)
}

/// Gmail messages labelled `factory` (plan D13). Reads one bounded page of
/// matching messages per poll, fetching each in full to extract its
/// `text/plain` body.
pub struct GmailIntake {
    pub token: String,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for GmailIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::Gmail
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        let client = reqwest::Client::new();
        let labels: Value = client
            .get(format!("{GMAIL_API}/labels"))
            .bearer_auth(&self.token)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let factory_label_id = labels
            .get("labels")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .find(|label| {
                label
                    .get("name")
                    .and_then(|v| v.as_str())
                    .is_some_and(|n| n.eq_ignore_ascii_case(INTAKE_LABEL))
            })
            .and_then(|label| label.get("id"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("gmail_label_missing"))?
            .to_string();
        let query = format!("label:{INTAKE_LABEL}");
        let max_results = GMAIL_PAGE.to_string();
        let list: Value = client
            .get(format!("{GMAIL_API}/messages"))
            .bearer_auth(&self.token)
            .query(&[("q", query.as_str()), ("maxResults", max_results.as_str())])
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let ids: Vec<String> = list
            .get("messages")
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|m| m.get("id").and_then(|v| v.as_str()).map(str::to_string))
            .collect();
        let now = Utc::now();
        let mut specs = Vec::new();
        for id in ids {
            let message: Value = client
                .get(format!("{GMAIL_API}/messages/{id}"))
                .bearer_auth(&self.token)
                .query(&[("format", "full")])
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            match gmail_message_to_task_spec(&message, &factory_label_id, &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(message = %id, "Skipping Gmail message in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Notion (F4)

const NOTION_API: &str = "https://api.notion.com/v1";
const NOTION_VERSION: &str = "2022-06-28";
const NOTION_PAGE: i64 = 50;

/// Notion ids are UUIDs, written hyphenated or compact (32 hex characters
/// either way).
pub fn valid_notion_id(id: &str) -> bool {
    let compact: String = id.chars().filter(|c| *c != '-').collect();
    compact.len() == 32 && compact.bytes().all(|b| b.is_ascii_hexdigit())
}

fn normalize_notion_id(id: &str) -> String {
    id.chars().filter(|c| *c != '-').collect::<String>().to_ascii_lowercase()
}

/// Whether a Notion page opts into the factory: it lives in the configured
/// database, or one of its properties tags it `factory` (a `multi_select` /
/// `select` / `status` option named `factory`, or a checkbox property named
/// `factory` that is checked).
pub fn notion_page_is_factory(page: &Value, configured_database_id: Option<&str>) -> bool {
    if let Some(configured) = configured_database_id {
        let parent_db = page.pointer("/parent/database_id").and_then(|v| v.as_str());
        if parent_db.is_some_and(|id| normalize_notion_id(id) == normalize_notion_id(configured)) {
            return true;
        }
    }
    let Some(properties) = page.get("properties").and_then(|v| v.as_object()) else {
        return false;
    };
    properties.iter().any(|(name, property)| {
        let kind = property.get("type").and_then(|v| v.as_str()).unwrap_or_default();
        match kind {
            "multi_select" => property
                .get("multi_select")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|o| o.get("name").and_then(|v| v.as_str()))
                .any(|n| n.eq_ignore_ascii_case(INTAKE_LABEL)),
            "select" => property
                .pointer("/select/name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(INTAKE_LABEL)),
            "status" => property
                .pointer("/status/name")
                .and_then(|v| v.as_str())
                .is_some_and(|n| n.eq_ignore_ascii_case(INTAKE_LABEL)),
            "checkbox" => {
                name.eq_ignore_ascii_case(INTAKE_LABEL)
                    && property.get("checkbox").and_then(|v| v.as_bool()) == Some(true)
            }
            _ => false,
        }
    })
}

fn notion_rich_text_plain(value: &Value) -> String {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|rt| rt.get("plain_text").and_then(|v| v.as_str()))
        .collect::<Vec<_>>()
        .join("")
}

fn notion_title(page: &Value) -> String {
    page.get("properties")
        .and_then(|v| v.as_object())
        .into_iter()
        .flatten()
        .find(|(_, property)| property.get("type").and_then(|v| v.as_str()) == Some("title"))
        .map(|(_, property)| notion_rich_text_plain(property.get("title").unwrap_or(&Value::Null)))
        .unwrap_or_default()
}

fn notion_block_line(block: &Value) -> Option<String> {
    let kind = block.get("type").and_then(|v| v.as_str())?;
    let body = block.get(kind)?;
    let text = notion_rich_text_plain(body.get("rich_text").unwrap_or(&Value::Null));
    match kind {
        "paragraph" | "heading_1" | "heading_2" | "heading_3" | "quote" | "code" => {
            (!text.is_empty()).then_some(text)
        }
        "bulleted_list_item" | "numbered_list_item" => Some(format!("- {text}")),
        "to_do" => {
            let checked = body.get("checked").and_then(|v| v.as_bool()) == Some(true);
            Some(format!("- [{}] {text}", if checked { "x" } else { " " }))
        }
        _ => None,
    }
}

/// Flattens one page of Notion block children (`blocks.children`, no
/// recursion into nested children — best effort) into plain text. A checkbox
/// to-do becomes a markdown checklist line, so `acceptance_criteria` picks it
/// up like any other source.
pub fn notion_blocks_to_text(blocks: &Value) -> String {
    blocks
        .get("results")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(notion_block_line)
        .collect::<Vec<_>>()
        .join("\n")
}

/// A Notion page (`pages.get` shape) that opts into the factory, with its
/// block children already flattened to text by the fetcher. `Ok(None)`: not
/// opted in. `Err`: malformed. Its task is always `fix: manual`, like Gmail.
pub fn notion_page_to_task_spec(
    page: &Value,
    body_text: &str,
    configured_database_id: Option<&str>,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    if !notion_page_is_factory(page, configured_database_id) {
        return Ok(None);
    }
    let id = page.get("id").and_then(|v| v.as_str()).ok_or("notion_id_missing")?;
    let mut redactor = Redactor::new();
    let title = redactor.apply(&notion_title(page));
    let description = redactor.apply(body_text);
    if title.trim().is_empty() {
        return Err("notion_title_missing".into());
    }
    let reference = id.to_string();
    let url = page
        .get("url")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|u| u.starts_with("https://"));
    let source = TaskSource { kind: SourceKind::Transcript, reference: reference.clone(), url };
    build_spec(
        source,
        &format!("notion:{reference}->{}", target.repository),
        OriginTrust::Untrusted,
        target,
        &title,
        &description,
        &[],
        now,
    )
    .map(Some)
}

/// Notion pages tagged `factory`, or inside a configured database (plan D13).
/// Reads one bounded page of matching pages per poll, fetching each page's
/// block children (one bounded page each, no nested recursion) for its text.
pub struct NotionIntake {
    pub token: String,
    pub database_id: Option<String>,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for NotionIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::Transcript
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        if let Some(id) = &self.database_id {
            if !valid_notion_id(id) {
                anyhow::bail!("invalid_notion_database_id");
            }
        }
        let client = reqwest::Client::new();
        let body: Value = match &self.database_id {
            Some(id) => client
                .post(format!("{NOTION_API}/databases/{id}/query"))
                .bearer_auth(&self.token)
                .header("Notion-Version", NOTION_VERSION)
                .json(&json!({ "page_size": NOTION_PAGE }))
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?,
            None => client
                .post(format!("{NOTION_API}/search"))
                .bearer_auth(&self.token)
                .header("Notion-Version", NOTION_VERSION)
                .json(&json!({"filter": {"value": "page", "property": "object"}, "page_size": NOTION_PAGE}))
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?,
        };
        let pages = body
            .get("results")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("notion_pages_unreadable"))?;
        let now = Utc::now();
        let mut specs = Vec::new();
        for page in pages {
            if !notion_page_is_factory(page, self.database_id.as_deref()) {
                continue;
            }
            let Some(id) = page.get("id").and_then(|v| v.as_str()) else { continue };
            let blocks: Value = client
                .get(format!("{NOTION_API}/blocks/{id}/children"))
                .bearer_auth(&self.token)
                .header("Notion-Version", NOTION_VERSION)
                .query(&[("page_size", "100")])
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            let text = notion_blocks_to_text(&blocks);
            match notion_page_to_task_spec(page, &text, self.database_id.as_deref(), &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(page = %id, "Skipping Notion page in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Drive / Meet transcripts (F4)

const DRIVE_API: &str = "https://www.googleapis.com/drive/v3";
const DRIVE_PAGE: i64 = 50;

/// Google Drive file and folder ids: URL-safe-ish alphanumerics, `-` and `_`.
/// Also blocks a folder id crafted to break out of the Drive `q` query string.
pub fn valid_drive_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

/// A Google Drive file (`files.list` shape) in the configured folder, with its
/// text already exported (`files.export?mimeType=text/plain`) by the fetcher.
/// Always untrusted, always `fix: manual`, like every other transcript.
/// `Ok(None)`: malformed enough to have no name. `Err` is never returned here;
/// Drive's own query already is the opt-in, so there is nothing left to reject.
pub fn drive_transcript_to_task_spec(
    file: &Value,
    text: &str,
    target: &IntakeTarget,
    now: DateTime<Utc>,
) -> Result<Option<TaskSpec>, String> {
    let id = file.get("id").and_then(|v| v.as_str()).ok_or("drive_id_missing")?;
    let mut redactor = Redactor::new();
    let title = redactor.apply(file.get("name").and_then(|v| v.as_str()).unwrap_or_default());
    let description = redactor.apply(text);
    if title.trim().is_empty() {
        return Err("drive_file_name_missing".into());
    }
    let reference = id.to_string();
    let url = file
        .get("webViewLink")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .filter(|u| u.starts_with("https://"));
    let source = TaskSource { kind: SourceKind::Transcript, reference: reference.clone(), url };
    build_spec(
        source,
        &format!("drive:{reference}->{}", target.repository),
        OriginTrust::Untrusted,
        target,
        &title,
        &description,
        &[],
        now,
    )
    .map(Some)
}

/// Google Drive "Meet transcript" docs in a configured folder (plan D13).
/// Reads one bounded page of matching files per poll, exporting each as plain
/// text.
pub struct DriveIntake {
    pub token: String,
    pub folder_id: String,
    pub target: IntakeTarget,
}

#[async_trait]
impl IntakeSource for DriveIntake {
    fn kind(&self) -> SourceKind {
        SourceKind::Transcript
    }

    async fn fetch(&self) -> anyhow::Result<Vec<TaskSpec>> {
        if !valid_drive_id(&self.folder_id) {
            anyhow::bail!("invalid_drive_folder_id");
        }
        let client = reqwest::Client::new();
        let query = format!(
            "'{}' in parents and mimeType='application/vnd.google-apps.document' and trashed=false",
            self.folder_id
        );
        let page_size = DRIVE_PAGE.to_string();
        let list: Value = client
            .get(format!("{DRIVE_API}/files"))
            .bearer_auth(&self.token)
            .query(&[
                ("q", query.as_str()),
                ("pageSize", page_size.as_str()),
                ("fields", "files(id,name,webViewLink)"),
            ])
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let files = list
            .get("files")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("drive_files_unreadable"))?;
        let now = Utc::now();
        let mut specs = Vec::new();
        for file in files {
            let Some(id) = file.get("id").and_then(|v| v.as_str()) else { continue };
            let text = client
                .get(format!("{DRIVE_API}/files/{id}/export"))
                .bearer_auth(&self.token)
                .query(&[("mimeType", "text/plain")])
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await?
                .error_for_status()?
                .text()
                .await?;
            match drive_transcript_to_task_spec(file, &text, &self.target, now) {
                Ok(Some(spec)) => specs.push(spec),
                Ok(None) => {}
                Err(reason) => tracing::warn!(file = %id, "Skipping Drive file in intake: {reason}"),
            }
        }
        Ok(specs)
    }
}

// ---------------------------------------------------------------- Uploaded / local transcripts (F4)

/// A transcript with no attached resolver has no code destination; this
/// placeholder satisfies `TaskRepository`'s `owner/repo` shape without naming
/// a real repository. Chosen so a repository-keyed lookup (`untrusted_intake_
/// issue`/`_pull`) simply never matches these items — there is no PR to
/// protect because there is no repository to open one in.
pub const NO_RESOLVER_REPOSITORY: &str = "none/transcript-intake";

/// A local `.txt` file or an admin-uploaded transcript (plan D13): raw text
/// with no provider id of its own. Always untrusted, always `fix: manual`.
/// `repository`: the attached resolver's target repository, or `None` when
/// the transcript has no resolver. There is no provider id to dedupe on, so
/// the task id is derived from the redacted text itself: uploading the exact
/// same transcript twice yields the same `task_id`, which the database's
/// primary key then refuses as a duplicate rather than creating a second task.
pub fn transcript_text_to_task_spec(
    title: &str,
    text: &str,
    repository: Option<&str>,
    now: DateTime<Utc>,
) -> Result<TaskSpec, String> {
    let target = IntakeTarget {
        repository: repository.unwrap_or(NO_RESOLVER_REPOSITORY).to_string(),
        base_ref: "main".into(),
        privacy_class: PrivacyClass::Internal,
    };
    let mut redactor = Redactor::new();
    let title = redactor.apply(title);
    let description = redactor.apply(text);
    let digest = {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(title.as_bytes());
        hasher.update([0u8]);
        hasher.update(description.as_bytes());
        hasher.update([0u8]);
        hasher.update(target.repository.as_bytes());
        hasher.finalize().iter().map(|b| format!("{b:02x}")).collect::<String>()
    };
    let reference = format!("sha256:{digest}");
    let source = TaskSource { kind: SourceKind::Transcript, reference: reference.clone(), url: None };
    build_spec(
        source,
        &format!("transcript:{reference}"),
        OriginTrust::Untrusted,
        &target,
        &title,
        &description,
        &[],
        now,
    )
}

// ---------------------------------------------------------------- Source config (F3)

/// A configured intake source, as stored in `factory_intake_sources`.
#[derive(Clone, Debug, PartialEq)]
pub enum SourceConfig {
    Slack { channel_id: String, allowed_reactors: Vec<String> },
    Sentry { base_url: String, org_slug: String, project_slug: String, query: String },
    /// No fields: the label (`factory`) is fixed, not admin-configurable.
    Gmail,
    /// `database_id`: restrict to one database; `None` searches every page
    /// for the `factory` tag instead.
    Notion { database_id: Option<String> },
    Drive { folder_id: String },
}

/// Slack user ids are uppercase alphanumerics (`U0123ABC`, `W0123ABC`).
fn valid_slack_user(user: &str) -> bool {
    (8..=20).contains(&user.len()) && user.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

/// Validates a stored source config for its kind (`slack`, `sentry`, `gmail`,
/// `notion` or `drive`).
pub fn parse_source_config(kind: &str, config: &Value) -> Result<SourceConfig, String> {
    let text = |name: &str| config.get(name).and_then(|v| v.as_str()).map(str::trim).unwrap_or_default().to_string();
    match kind {
        "slack" => {
            let channel_id = text("channel_id");
            if !valid_slack_channel(&channel_id) {
                return Err("invalid_slack_channel".into());
            }
            let reactors = config.get("allowed_reactors").and_then(|v| v.as_array()).ok_or("allowed_reactors_required")?;
            let allowed_reactors: Vec<String> = reactors
                .iter()
                .map(|v| v.as_str().map(str::trim).unwrap_or_default().to_string())
                .collect();
            if allowed_reactors.is_empty() || allowed_reactors.len() > 50 || !allowed_reactors.iter().all(|u| valid_slack_user(u)) {
                return Err("invalid_allowed_reactors".into());
            }
            Ok(SourceConfig::Slack { channel_id, allowed_reactors })
        }
        "sentry" => {
            let base_url = text("base_url");
            let (org_slug, project_slug, query) = (text("org_slug"), text("project_slug"), text("query"));
            if sentry_base_host(&base_url).is_none() {
                return Err("invalid_sentry_base_url".into());
            }
            if !valid_sentry_slug(&org_slug) || !valid_sentry_slug(&project_slug) {
                return Err("invalid_sentry_slug".into());
            }
            if query.is_empty() || query.chars().count() > 500 {
                return Err("sentry_query_required".into());
            }
            Ok(SourceConfig::Sentry { base_url, org_slug, project_slug, query })
        }
        "gmail" => Ok(SourceConfig::Gmail),
        "notion" => {
            let database_id = text("database_id");
            if database_id.is_empty() {
                Ok(SourceConfig::Notion { database_id: None })
            } else if valid_notion_id(&database_id) {
                Ok(SourceConfig::Notion { database_id: Some(database_id) })
            } else {
                Err("invalid_notion_database_id".into())
            }
        }
        "drive" => {
            let folder_id = text("folder_id");
            if valid_drive_id(&folder_id) {
                Ok(SourceConfig::Drive { folder_id })
            } else {
                Err("invalid_drive_folder_id".into())
            }
        }
        _ => Err("unknown_intake_kind".into()),
    }
}

/// The fetcher for a validated source config.
pub fn intake_for(config: SourceConfig, token: String, target: IntakeTarget) -> Box<dyn IntakeSource> {
    match config {
        SourceConfig::Slack { channel_id, allowed_reactors } => {
            Box::new(SlackIntake { token, channel_id, allowed_reactors, target })
        }
        SourceConfig::Sentry { base_url, org_slug, project_slug, query } => {
            Box::new(SentryIntake { token, base_url, org_slug, project_slug, query, target })
        }
        SourceConfig::Gmail => Box::new(GmailIntake { token, target }),
        SourceConfig::Notion { database_id } => Box::new(NotionIntake { token, database_id, target }),
        SourceConfig::Drive { folder_id } => Box::new(DriveIntake { token, folder_id, target }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn target() -> IntakeTarget {
        IntakeTarget {
            repository: "acme/web".into(),
            base_ref: "main".into(),
            privacy_class: PrivacyClass::Internal,
        }
    }

    fn now() -> DateTime<Utc> {
        "2026-09-29T20:00:00Z".parse().unwrap()
    }

    fn issue(association: &str, labels: &[&str]) -> Value {
        json!({
            "number": 42,
            "title": "Document MetricCard props",
            "body": "Add JSDoc.\n\n- [ ] every prop documented\n- [x] lint passes\n* [ ] not a dash item",
            "html_url": "https://github.com/acme/web/issues/42",
            "author_association": association,
            "labels": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>(),
        })
    }

    #[test]
    fn only_labeled_issues_enter_and_pull_requests_are_skipped() {
        assert_eq!(
            github_issue_to_task_spec(&issue("MEMBER", &["docs"]), &target(), now()),
            Ok(None)
        );
        let mut pr = issue("MEMBER", &["factory"]);
        pr["pull_request"] = json!({"url": "https://api.github.com/..."});
        assert_eq!(github_issue_to_task_spec(&pr, &target(), now()), Ok(None));
    }

    #[test]
    fn github_trust_comes_from_the_author_association() {
        for (association, trust) in [
            ("OWNER", OriginTrust::Trusted),
            ("MEMBER", OriginTrust::Trusted),
            ("COLLABORATOR", OriginTrust::Trusted),
            ("CONTRIBUTOR", OriginTrust::Untrusted),
            ("NONE", OriginTrust::Untrusted),
            ("FIRST_TIMER", OriginTrust::Untrusted),
        ] {
            let spec =
                github_issue_to_task_spec(&issue(association, &["factory"]), &target(), now())
                    .unwrap()
                    .unwrap();
            assert_eq!(spec.origin_trust, trust, "{association}");
        }
        let mut anonymous = issue("MEMBER", &["factory"]);
        anonymous
            .as_object_mut()
            .unwrap()
            .remove("author_association");
        let spec = github_issue_to_task_spec(&anonymous, &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
    }

    #[test]
    fn a_github_issue_becomes_a_valid_task_spec() {
        let spec =
            github_issue_to_task_spec(&issue("MEMBER", &["factory", "docs"]), &target(), now())
                .unwrap()
                .unwrap();
        spec.validate().unwrap();
        assert_eq!(spec.source.kind, SourceKind::GithubIssue);
        assert_eq!(spec.source.reference, "acme/web#42");
        assert_eq!(
            spec.source.url.as_deref(),
            Some("https://github.com/acme/web/issues/42")
        );
        assert_eq!(spec.task_class, TaskClass::Docs);
        assert_eq!(spec.repository.remote, "acme/web");
        assert_eq!(
            spec.acceptance_criteria,
            vec!["every prop documented", "lint passes"]
        );
        assert_eq!(spec.task_id, stable_task_id("github_issue:acme/web#42"));
    }

    #[test]
    fn malformed_issues_are_errors_not_silent_skips() {
        let mut no_number = issue("MEMBER", &["factory"]);
        no_number.as_object_mut().unwrap().remove("number");
        assert!(github_issue_to_task_spec(&no_number, &target(), now()).is_err());
        let mut empty_title = issue("MEMBER", &["factory"]);
        empty_title["title"] = json!("");
        assert!(github_issue_to_task_spec(&empty_title, &target(), now()).is_err());
    }

    #[test]
    fn long_descriptions_are_truncated_to_the_contract_limit() {
        let mut long = issue("MEMBER", &["factory"]);
        long["body"] = json!("é".repeat(70_000));
        let spec = github_issue_to_task_spec(&long, &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(spec.description.chars().count(), MAX_DESCRIPTION_CHARS);
        spec.validate().unwrap();
    }

    #[test]
    fn task_ids_are_stable_and_distinct() {
        let a = stable_task_id("github_issue:acme/web#42");
        assert_eq!(a, stable_task_id("github_issue:acme/web#42"));
        assert_ne!(a, stable_task_id("github_issue:acme/web#43"));
        assert!(uuid::Uuid::parse_str(&a).is_ok());
        assert_eq!(a, a.to_lowercase());
    }

    #[test]
    fn the_most_sensitive_label_wins() {
        let labels = |names: &[&str]| names.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(
            classify_task(&labels(&["docs", "security"])),
            TaskClass::Security
        );
        assert_eq!(
            classify_task(&labels(&["tests", "database"])),
            TaskClass::Migration
        );
        assert_eq!(classify_task(&labels(&["ui", "bug"])), TaskClass::Bugfix);
        assert_eq!(classify_task(&labels(&["Documentation"])), TaskClass::Docs);
        assert_eq!(
            classify_task(&labels(&["factory", "good first issue"])),
            TaskClass::Unknown
        );
    }

    fn task(labels: &[&str]) -> Task {
        Task {
            id: "t-1".into(),
            org_id: "o1".into(),
            project: "web".into(),
            title: "Add tests for MetricCard".into(),
            description: Some("- [ ] covers empty state".into()),
            status: "todo".into(),
            priority: "medium".into(),
            due_date: None,
            parent_id: None,
            sprint_id: None,
            created_by: "u1".into(),
            created_at: "2026-09-29T10:00:00Z".into(),
            updated_at: "2026-09-29T10:00:00Z".into(),
            archived_at: None,
            assignees: vec![],
            labels: labels.iter().map(|l| l.to_string()).collect(),
            comment_count: 0,
            spec_links: vec![],
            subtask_count: 0,
        }
    }

    #[test]
    fn only_unstarted_nexusmind_tasks_enter() {
        for status in ["in_progress", "in_review", "done", "cancelled", "mystery"] {
            let mut t = task(&["factory"]);
            t.status = status.into();
            assert_eq!(
                nexusmind_task_to_task_spec(&t, &target(), now()),
                Ok(None),
                "{status}"
            );
        }
        for status in ["backlog", "todo"] {
            let mut t = task(&["factory"]);
            t.status = status.into();
            assert!(
                nexusmind_task_to_task_spec(&t, &target(), now())
                    .unwrap()
                    .is_some(),
                "{status}"
            );
        }
    }

    #[test]
    fn the_intake_label_matches_regardless_of_case_in_both_sources() {
        assert!(
            nexusmind_task_to_task_spec(&task(&["Factory"]), &target(), now())
                .unwrap()
                .is_some()
        );
        assert!(
            github_issue_to_task_spec(&issue("MEMBER", &["FACTORY"]), &target(), now())
                .unwrap()
                .is_some()
        );
    }

    #[test]
    fn nexusmind_tasks_are_trusted_and_opt_in() {
        assert_eq!(
            nexusmind_task_to_task_spec(&task(&["tests"]), &target(), now()),
            Ok(None)
        );
        let spec = nexusmind_task_to_task_spec(&task(&["factory", "tests"]), &target(), now())
            .unwrap()
            .unwrap();
        spec.validate().unwrap();
        assert_eq!(spec.origin_trust, OriginTrust::Trusted);
        assert_eq!(spec.source.kind, SourceKind::NexusmindTask);
        assert_eq!(spec.source.reference, "t-1");
        assert_eq!(spec.task_class, TaskClass::Tests);
        assert_eq!(spec.acceptance_criteria, vec!["covers empty state"]);
    }

    fn slack_message(reactors: &[&str]) -> Value {
        json!({
            "ts": "1730000000.000100",
            "user": "U123",
            "text": "Refund button missing &amp; broken #docs\nSteps: open a sale…",
            "reactions": [{"name": "eyes", "users": ["U9"]}, {"name": "factory", "users": reactors}]
        })
    }

    fn reactors() -> Vec<String> {
        vec!["ULEAD".to_string()]
    }

    #[test]
    fn a_slack_message_flagged_by_an_allowed_person_is_an_untrusted_task() {
        let message = slack_message(&["U123", "ULEAD"]);
        let spec = slack_message_to_task_spec(&message, "C0123ABCD", &reactors(), &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(spec.title, "Refund button missing & broken #docs");
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        // The author's `#docs` does not pick the class.
        assert_eq!(spec.task_class, TaskClass::Unknown);
        assert_eq!(spec.source.reference, "C0123ABCD:1730000000.000100");
        let again = slack_message_to_task_spec(&message, "C0123ABCD", &reactors(), &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(spec.task_id, again.task_id);
        let mut other = target();
        other.repository = "acme/api".into();
        let elsewhere = slack_message_to_task_spec(&message, "C0123ABCD", &reactors(), &other, now())
            .unwrap()
            .unwrap();
        assert_ne!(spec.task_id, elsewhere.task_id);
        // A thread parent counts; a file share is a human message.
        let mut parent = message.clone();
        parent["thread_ts"] = json!("1730000000.000100");
        parent["subtype"] = json!("file_share");
        assert!(slack_message_to_task_spec(&parent, "C0123ABCD", &reactors(), &target(), now())
            .unwrap()
            .is_some());
    }

    #[test]
    fn only_allowed_reactors_and_human_top_level_messages_count() {
        let flagged = |message: &Value| {
            slack_message_to_task_spec(message, "C0123ABCD", &reactors(), &target(), now()).unwrap()
        };
        // The author or anyone else adding `:factory:` is not enough.
        assert_eq!(flagged(&slack_message(&["U123", "UGUEST"])), None);
        assert_eq!(
            slack_message_to_task_spec(&slack_message(&["ULEAD"]), "C0123ABCD", &[], &target(), now())
                .unwrap(),
            None
        );
        let mut bot = slack_message(&["ULEAD"]);
        bot["bot_id"] = json!("B1");
        let mut joined = slack_message(&["ULEAD"]);
        joined["subtype"] = json!("channel_join");
        let mut reply = slack_message(&["ULEAD"]);
        reply["thread_ts"] = json!("1729999999.000001");
        for message in [bot, joined, reply] {
            assert_eq!(flagged(&message), None);
        }
        let mut empty = slack_message(&["ULEAD"]);
        empty["text"] = json!("   ");
        assert!(slack_message_to_task_spec(&empty, "C0123ABCD", &reactors(), &target(), now()).is_err());
        let mut no_ts = slack_message(&["ULEAD"]);
        no_ts.as_object_mut().unwrap().remove("ts");
        assert!(slack_message_to_task_spec(&no_ts, "C0123ABCD", &reactors(), &target(), now()).is_err());
        assert!(valid_slack_channel("C0123ABCD"));
        assert!(!valid_slack_channel("c0123abcd"));
        assert!(!valid_slack_channel("C01&x=1"));
    }

    fn sentry_issue() -> Value {
        json!({
            "id": "4501", "shortId": "APP-12", "title": "TypeError: refund is undefined",
            "culprit": "SaleDetail in render", "status": "unresolved", "level": "error",
            "count": "37", "userCount": 9, "permalink": "https://acme.sentry.io/issues/4501/",
            "firstSeen": "2026-10-01T00:00:00Z", "lastSeen": "2026-10-03T00:00:00Z"
        })
    }

    #[test]
    fn an_unresolved_sentry_error_or_fatal_is_an_untrusted_bugfix() {
        let spec = sentry_issue_to_task_spec(&sentry_issue(), "sentry.io/acme", &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(spec.task_class, TaskClass::Bugfix);
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        assert_eq!(spec.source.reference, "sentry.io/acme:4501");
        assert_eq!(spec.source.url.as_deref(), Some("https://acme.sentry.io/issues/4501/"));
        assert!(spec.description.contains("Events: 37 · users affected: 9"), "{}", spec.description);
        // Issue ids are per instance.
        let other = sentry_issue_to_task_spec(&sentry_issue(), "sentry.acme.dev/acme", &target(), now())
            .unwrap()
            .unwrap();
        assert_ne!(spec.task_id, other.task_id);

        let mut fatal = sentry_issue();
        fatal["level"] = json!("fatal");
        fatal["title"] = json!("x".repeat(2000));
        fatal["permalink"] = json!("http://insecure/1");
        fatal["userCount"] = json!(null);
        let fatal = sentry_issue_to_task_spec(&fatal, "sentry.io/acme", &target(), now()).unwrap().unwrap();
        assert_eq!(fatal.title.chars().count(), 200);
        assert_eq!(fatal.source.url, None);
        assert!(fatal.description.contains("users affected: ?"));

        let mut resolved = sentry_issue();
        resolved["status"] = json!("resolved");
        let mut warning = sentry_issue();
        warning["level"] = json!("warning");
        for skipped in [resolved, warning] {
            assert_eq!(sentry_issue_to_task_spec(&skipped, "sentry.io/acme", &target(), now()).unwrap(), None);
        }
        let mut numeric_id = sentry_issue();
        numeric_id["id"] = json!(4501);
        assert!(sentry_issue_to_task_spec(&numeric_id, "sentry.io/acme", &target(), now()).is_err());
    }

    #[test]
    fn the_sentry_token_only_goes_to_a_clean_https_host() {
        assert_eq!(sentry_base_host("https://sentry.io").as_deref(), Some("sentry.io"));
        assert_eq!(sentry_base_host("https://Sentry.Acme.dev/").as_deref(), Some("sentry.acme.dev"));
        for bad in [
            "http://sentry.io",
            "https://sentry.io@evil.com",
            "https://user:pw@sentry.io",
            "https://10.0.0.5",
            "https://[::1]",
            "https://localhost",
            "https://sentry.io/x?a=",
            "https://sentry.io/#f",
            "https://sentry.io:8443",
            "https://intranet",
            "not a url",
        ] {
            assert_eq!(sentry_base_host(bad), None, "{bad}");
        }
        assert!(valid_sentry_slug("acme-prod"));
        assert!(!valid_sentry_slug(""));
        assert!(!valid_sentry_slug("Acme/../x"));
    }

    #[test]
    fn source_configs_are_validated_per_kind() {
        let slack = parse_source_config(
            "slack",
            &json!({"channel_id": "C0123ABCD", "allowed_reactors": ["ULEAD0001"]}),
        )
        .unwrap();
        assert_eq!(
            slack,
            SourceConfig::Slack { channel_id: "C0123ABCD".into(), allowed_reactors: vec!["ULEAD0001".into()] }
        );
        for bad in [
            json!({"channel_id": "C0123ABCD", "allowed_reactors": []}),
            json!({"channel_id": "C0123ABCD"}),
            json!({"channel_id": "C0123ABCD", "allowed_reactors": ["lowercase1"]}),
            json!({"channel_id": "x", "allowed_reactors": ["ULEAD0001"]}),
        ] {
            assert!(parse_source_config("slack", &bad).is_err(), "{bad}");
        }
        let sentry = json!({"base_url": "https://sentry.io", "org_slug": "acme", "project_slug": "web", "query": "is:unresolved"});
        assert!(parse_source_config("sentry", &sentry).is_ok());
        let mut no_query = sentry.clone();
        no_query["query"] = json!("  ");
        let mut evil = sentry.clone();
        evil["base_url"] = json!("https://sentry.io@evil.com");
        for bad in [no_query, evil] {
            assert!(parse_source_config("sentry", &bad).is_err());
        }
        assert!(parse_source_config("github", &json!({})).is_err());
    }

    #[test]
    fn factory_issues_and_tasks_from_untrusted_intake_stay_untrusted() {
        let mut opened = issue("MEMBER", &["factory"]);
        opened["body"] = json!(format!("Text from Slack\n<!-- {INTAKE_ISSUE_MARKER}abc -->"));
        let spec = github_issue_to_task_spec(&opened, &target(), now()).unwrap().unwrap();
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        let member = github_issue_to_task_spec(&issue("MEMBER", &["factory"]), &target(), now()).unwrap().unwrap();
        assert_eq!(member.origin_trust, OriginTrust::Trusted);
    }

    // ------------------------------------------------------------ Gmail (F4)

    fn gmail_message(label_ids: &[&str], body: &str) -> Value {
        use base64::Engine;
        let data = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(body);
        json!({
            "id": "18c7f1",
            "labelIds": label_ids,
            "payload": {
                "headers": [{"name": "Subject", "value": "Refund button broken, contact ana@acme.test"}],
                "mimeType": "multipart/mixed",
                "parts": [{"mimeType": "text/plain", "body": {"data": data}}]
            }
        })
    }

    #[test]
    fn gmail_messages_need_the_factory_label_and_are_always_untrusted() {
        let message = gmail_message(&["Label_1", "INBOX"], "Steps to reproduce.\nCall 300 123 4567.");
        let spec = gmail_message_to_task_spec(&message, "Label_1", &target(), now()).unwrap().unwrap();
        assert_eq!(spec.source.kind, SourceKind::Gmail);
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        assert_eq!(spec.source.reference, "18c7f1");
        assert_eq!(spec.title, "Refund button broken, contact [EMAIL_1]");
        assert!(spec.description.contains("[PHONE_1]"), "{}", spec.description);
        spec.validate().unwrap();

        // Without the (resolved) label id, it is not for the factory.
        assert_eq!(gmail_message_to_task_spec(&message, "Label_other", &target(), now()).unwrap(), None);

        // No subject is malformed, not a silent skip.
        let mut no_subject = message.clone();
        no_subject["payload"]["headers"] = json!([]);
        assert!(gmail_message_to_task_spec(&no_subject, "Label_1", &target(), now()).is_err());
    }

    // ------------------------------------------------------------ Notion (F4)

    fn notion_page_tagged() -> Value {
        json!({
            "id": "page-1",
            "url": "https://www.notion.so/page-1",
            "parent": {"type": "workspace"},
            "properties": {
                "Name": {"type": "title", "title": [{"plain_text": "Fix refund flow"}]},
                "Tags": {"type": "multi_select", "multi_select": [{"name": "factory"}]}
            }
        })
    }

    fn notion_page_untagged(database_id: &str) -> Value {
        json!({
            "id": "page-2",
            "url": "https://www.notion.so/page-2",
            "parent": {"type": "database_id", "database_id": database_id},
            "properties": {"Name": {"type": "title", "title": [{"plain_text": "Document API"}]}}
        })
    }

    fn notion_blocks() -> Value {
        json!({"results": [
            {"type": "paragraph", "paragraph": {"rich_text": [{"plain_text": "Contact ana@acme.test"}]}},
            {"type": "to_do", "to_do": {"rich_text": [{"plain_text": "Ship the fix"}], "checked": false}}
        ]})
    }

    #[test]
    fn notion_pages_opt_in_by_tag_or_by_database_and_are_untrusted() {
        let text = notion_blocks_to_text(&notion_blocks());
        assert!(text.contains("ana@acme.test"));
        assert!(text.contains("- [ ] Ship the fix"));

        let tagged = notion_page_tagged();
        let spec = notion_page_to_task_spec(&tagged, &text, None, &target(), now()).unwrap().unwrap();
        assert_eq!(spec.source.kind, SourceKind::Transcript);
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        assert_eq!(spec.title, "Fix refund flow");
        assert!(spec.description.contains("[EMAIL_1]"));
        assert_eq!(spec.acceptance_criteria, vec!["Ship the fix"]);
        spec.validate().unwrap();

        // Untagged and not in any configured database: skipped.
        let untagged = notion_page_untagged("db-1");
        assert_eq!(notion_page_to_task_spec(&untagged, &text, None, &target(), now()).unwrap(), None);
        // The same page matches once that database is configured.
        let in_db = notion_page_to_task_spec(&untagged, &text, Some("db-1"), &target(), now())
            .unwrap()
            .unwrap();
        assert_eq!(in_db.title, "Document API");
    }

    #[test]
    fn notion_and_drive_ids_are_validated() {
        assert!(valid_notion_id("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4"));
        assert!(valid_notion_id("a1b2c3d4-e5f6-a1b2-c3d4-e5f6a1b2c3d4"));
        assert!(!valid_notion_id("not-a-valid-id"));
        assert!(valid_drive_id("1A2b_3C-4d"));
        assert!(!valid_drive_id(""));
        assert!(!valid_drive_id("has space"));
        assert!(!valid_drive_id("quote'injection"));
    }

    // ------------------------------------------------------------ Drive (F4)

    fn drive_file() -> Value {
        json!({
            "id": "file-1",
            "name": "Meet transcript 2026-10-05",
            "webViewLink": "https://drive.google.com/file/d/file-1/view",
        })
    }

    #[test]
    fn drive_transcripts_are_untrusted() {
        let file = drive_file();
        let text = "Attendees discussed the refund bug.\nCall ana@acme.test for details.";
        let spec = drive_transcript_to_task_spec(&file, text, &target(), now()).unwrap().unwrap();
        assert_eq!(spec.source.kind, SourceKind::Transcript);
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        assert_eq!(spec.title, "Meet transcript 2026-10-05");
        assert!(spec.description.contains("[EMAIL_1]"));
        assert_eq!(spec.source.url.as_deref(), Some("https://drive.google.com/file/d/file-1/view"));
        spec.validate().unwrap();

        let mut no_name = file.clone();
        no_name["name"] = json!("");
        assert!(drive_transcript_to_task_spec(&no_name, text, &target(), now()).is_err());

        let mut no_id = file.clone();
        no_id.as_object_mut().unwrap().remove("id");
        assert!(drive_transcript_to_task_spec(&no_id, text, &target(), now()).is_err());
    }

    // ------------------------------------------------------- uploaded transcripts (F4)

    #[test]
    fn uploaded_transcripts_are_untrusted_and_dedupe_on_identical_content() {
        let spec =
            transcript_text_to_task_spec("Fix refund flow", "Contact ana@acme.test about this.", Some("acme/web"), now())
                .unwrap();
        assert_eq!(spec.source.kind, SourceKind::Transcript);
        assert_eq!(spec.origin_trust, OriginTrust::Untrusted);
        assert_eq!(spec.repository.remote, "acme/web");
        assert!(spec.description.contains("[EMAIL_1]"));
        spec.validate().unwrap();

        // No resolver: falls back to the placeholder repository, which is a
        // different target (and so a different task id).
        let no_resolver =
            transcript_text_to_task_spec("Fix refund flow", "Contact ana@acme.test about this.", None, now()).unwrap();
        assert_eq!(no_resolver.repository.remote, NO_RESOLVER_REPOSITORY);
        no_resolver.validate().unwrap();
        assert_ne!(spec.task_id, no_resolver.task_id);

        // The exact same title, text and target reproduces the same task id,
        // so re-uploading it is a duplicate rather than a second task.
        let again =
            transcript_text_to_task_spec("Fix refund flow", "Contact ana@acme.test about this.", Some("acme/web"), now())
                .unwrap();
        assert_eq!(spec.task_id, again.task_id);
    }

    // ------------------------------------------------------- source config (F4)

    #[test]
    fn f4_source_configs_are_validated_per_kind() {
        assert_eq!(parse_source_config("gmail", &json!({})), Ok(SourceConfig::Gmail));

        assert_eq!(parse_source_config("notion", &json!({})), Ok(SourceConfig::Notion { database_id: None }));
        let database_id = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4";
        assert_eq!(
            parse_source_config("notion", &json!({"database_id": database_id})),
            Ok(SourceConfig::Notion { database_id: Some(database_id.into()) })
        );
        assert!(parse_source_config("notion", &json!({"database_id": "not-an-id"})).is_err());

        assert_eq!(
            parse_source_config("drive", &json!({"folder_id": "1A2b_3C-4d"})),
            Ok(SourceConfig::Drive { folder_id: "1A2b_3C-4d".into() })
        );
        for bad in [json!({}), json!({"folder_id": ""}), json!({"folder_id": "has space"})] {
            assert!(parse_source_config("drive", &bad).is_err(), "{bad}");
        }
    }
}
