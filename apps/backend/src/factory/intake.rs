//! Intake: turns external work items into validated `TaskSpec`s (plan D13).
//!
//! Each source is a thin async fetcher over a pure normalizer. Normalizers decide
//! everything that matters — opt-in, trust, identity, task class — so they are
//! what the tests pin down. Intake does not persist or route; the router (F3)
//! consumes the specs. The target repository always comes from the control-plane
//! configuration of the source, never from the item itself.

use axum::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::factory::contracts::{
    Contract, OriginTrust, PrivacyClass, SchemaV1, SourceKind, TaskClass, TaskRepository,
    TaskSource, TaskSpec,
};
use crate::models::types::Task;

/// Only items carrying this label enter the factory.
pub const INTAKE_LABEL: &str = "factory";

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
    let trust = match issue.get("author_association").and_then(|v| v.as_str()) {
        Some("OWNER" | "MEMBER" | "COLLABORATOR") => OriginTrust::Trusted,
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

/// NexusMind tasks are authored by authenticated org members, so they are trusted.
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
        OriginTrust::Trusted,
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
}
