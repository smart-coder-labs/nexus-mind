//! Shadow decisions (factory F3, ADR 7d6f870f).
//!
//! After every PR review, the decision model (Jev) is asked whether this head
//! could be merged without a person. Its verdict is recorded next to the
//! deterministic floor that applies and, later, the real outcome; it never
//! changes what the factory does. The rows feed the false-low-risk rate that
//! must stay at or below 2% over 50 decisions per class before automatic
//! routing is enabled (OD-5).

use crate::db::factory_queries::{record_shadow_decision, shadow_decision_exists, ShadowDecision};
use crate::factory::jev;
use crate::store::sqlite::SqliteStore;

/// The Jev key, unless shadow decisions are switched off
/// (`FACTORY_JEV_SHADOW=off`). No key means no shadow.
pub fn shadow_key() -> Option<String> {
    if std::env::var("FACTORY_JEV_SHADOW").is_ok_and(|v| v.eq_ignore_ascii_case("off")) {
        return None;
    }
    std::env::var("TYPESAFE_API_KEY")
        .ok()
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Organizations whose PR metadata may be sent to the decision model's provider
/// (`FACTORY_JEV_SHADOW_ORGS`, comma-separated org ids). Empty means none: the
/// key alone never opts a tenant in.
pub fn shadow_enabled_for(org_id: &str) -> bool {
    org_listed(&std::env::var("FACTORY_JEV_SHADOW_ORGS").unwrap_or_default(), org_id)
}

fn org_listed(list: &str, org_id: &str) -> bool {
    !org_id.is_empty() && list.split(',').any(|allowed| allowed.trim() == org_id)
}

/// The PR's title, body and head commit, as the shadow decision needs them.
pub fn pull_summary(pull: &serde_json::Value) -> Option<(String, String, String)> {
    let head = pull.pointer("/head/sha")?.as_str()?;
    if head.len() != 40 || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let title = pull.get("title").and_then(|v| v.as_str()).unwrap_or_default();
    let body = pull.get("body").and_then(|v| v.as_str()).unwrap_or_default();
    Some((title.to_string(), body.to_string(), head.to_string()))
}

/// Records a shadow merge decision for a reviewed PR. Best effort: callers log
/// an error and carry on. Returns whether a new decision was recorded.
pub async fn record_merge_shadow(
    store: &SqliteStore,
    org_id: &str,
    token: &str,
    repository: &str,
    number: i64,
) -> anyhow::Result<bool> {
    let Some(key) = shadow_key() else {
        return Ok(false);
    };
    if !shadow_enabled_for(org_id) {
        return Ok(false);
    }
    let pull = super::connectors::get_github_pull(token, repository, number).await?;
    let Some((title, body, head_sha)) = pull_summary(&pull) else {
        anyhow::bail!("shadow_pull_unreadable");
    };
    {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        if shadow_decision_exists(&conn, org_id, repository, number, &head_sha)? {
            return Ok(false);
        }
    }
    let files = super::connectors::list_github_pull_files(token, repository, number).await?;
    let floor = super::merge_gate::auto_merge_path_verdict(&files).err();
    let paths: Vec<String> = files.into_iter().map(|f| f.filename).collect();
    let client = reqwest::Client::new();
    let change = jev::Change {
        title: &title,
        description: &body,
        paths: &paths,
    };
    let (answer, latency_ms) = jev::decide(&client, &key, &change).await?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
    record_shadow_decision(
        &conn,
        org_id,
        &ShadowDecision {
            provider: "jev",
            repository,
            pull_number: number,
            head_sha: &head_sha,
            answer: &answer,
            verdict: jev::verdict(&answer),
            floor: floor.as_deref(),
            latency_ms,
        },
    )
}

// ---------------------------------------------------------------- refresh

/// How a refresh pass went.
#[derive(Debug, Default, serde::Serialize)]
pub struct RefreshSummary {
    pub settled: usize,
    pub waiting: usize,
    pub failed: usize,
}

/// Pending decisions checked per org and pass.
const REFRESH_BATCH: i64 = 50;
/// A refresh pass never runs longer than this.
const REFRESH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15 * 60);

/// What settling one pending decision found.
enum Settled {
    /// Still open, or merged inside its outcome window.
    Waiting,
    Done,
}

async fn settle_one(
    store: &SqliteStore,
    org_id: &str,
    token: &str,
    row: &crate::db::factory_queries::PendingShadow,
) -> anyhow::Result<Settled> {
    use crate::db::factory_queries::set_shadow_outcome;
    let pull = super::connectors::get_github_pull(token, &row.repository, row.pull_number).await?;
    let merged_at = pull.get("merged_at").and_then(|v| v.as_str());
    let merge_sha = pull.get("merge_commit_sha").and_then(|v| v.as_str());
    let closed = pull.get("state").and_then(|v| v.as_str()) == Some("closed");
    let save = |outcome: &str, signals: &[String]| -> anyhow::Result<()> {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        set_shadow_outcome(&conn, org_id, &row.id, merged_at, merge_sha, outcome, signals)
    };
    let Some(merged) = merged_at else {
        if closed {
            save("not_merged", &[])?;
            return Ok(Settled::Done);
        }
        return Ok(Settled::Waiting);
    };
    // The decision was about this head. If another head was merged, the merged
    // code is not what the model judged: its outcome says nothing about it.
    if pull.pointer("/head/sha").and_then(|v| v.as_str()) != Some(row.head_sha.as_str()) {
        save("superseded", &[])?;
        return Ok(Settled::Done);
    }
    let window_end = chrono::DateTime::parse_from_rfc3339(merged)?.with_timezone(&chrono::Utc)
        + chrono::Duration::days(OUTCOME_WINDOW_DAYS);
    if chrono::Utc::now() < window_end {
        // Record the merge; the outcome is read once, when the window ends.
        save("pending", &[])?;
        return Ok(Settled::Waiting);
    }
    let (outcome, signals) = collect_outcome(token, &row.repository, &pull).await?;
    save(outcome, &signals)?;
    Ok(Settled::Done)
}

/// Settles up to [`REFRESH_BATCH`] of an org's pending shadow decisions:
/// merged heads get their outcome once their window has passed, a merge of a
/// different head is `superseded`, closed unmerged PRs are `not_merged`, open
/// ones keep waiting. A failure is counted; repeated failures end in
/// `unresolvable`.
pub async fn refresh_pending(
    store: &SqliteStore,
    org_id: &str,
    token: &str,
) -> anyhow::Result<RefreshSummary> {
    let pending = {
        let db = store.conn();
        let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
        crate::db::factory_queries::list_pending_shadow_decisions(&conn, org_id, REFRESH_BATCH)?
    };
    let mut summary = RefreshSummary::default();
    for row in pending {
        match settle_one(store, org_id, token, &row).await {
            Ok(Settled::Done) => summary.settled += 1,
            Ok(Settled::Waiting) => summary.waiting += 1,
            Err(error) => {
                summary.failed += 1;
                tracing::warn!(repository = row.repository, number = row.pull_number, "shadow outcome not refreshed: {error:#}");
                let db = store.conn();
                let recorded = match db.lock() {
                    Ok(conn) => crate::db::factory_queries::record_shadow_refresh_failure(&conn, org_id, &row.id),
                    Err(_) => Err(anyhow::anyhow!("database_lock")),
                };
                if let Err(error) = recorded {
                    tracing::warn!("shadow refresh failure not recorded: {error:#}");
                }
            }
        }
    }
    Ok(summary)
}

/// Refreshes every org with pending shadow decisions.
pub async fn refresh_all(store: &SqliteStore) {
    let orgs: Vec<String> = {
        let db = store.conn();
        let Ok(conn) = db.lock() else { return };
        let Ok(mut stmt) = conn.prepare(
            "SELECT DISTINCT org_id FROM factory_shadow_decisions WHERE outcome = 'pending'",
        ) else {
            return;
        };
        let rows = stmt.query_map([], |r| r.get::<_, String>(0));
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => return,
        }
    };
    if orgs.is_empty() {
        return;
    }
    let token = match super::worker::server_github_token().await {
        Ok(token) => token,
        Err(error) => {
            tracing::warn!("shadow refresh skipped, no GitHub token: {error:#}");
            return;
        }
    };
    for org_id in orgs {
        match refresh_pending(store, &org_id, &token).await {
            Ok(summary) => tracing::info!(org_id, ?summary, "shadow outcomes refreshed"),
            Err(error) => tracing::warn!(org_id, "shadow refresh failed: {error:#}"),
        }
    }
}

static REFRESH_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Worker tick: starts a refresh pass beside the tick loop (which must keep
/// claiming work), unless one is still running. A pass is bounded by
/// [`REFRESH_TIMEOUT`].
pub fn spawn_refresh(store: SqliteStore) {
    use std::sync::atomic::Ordering;
    if REFRESH_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    tokio::spawn(async move {
        if tokio::time::timeout(REFRESH_TIMEOUT, refresh_all(&store)).await.is_err() {
            tracing::warn!("shadow refresh pass timed out");
        }
        REFRESH_RUNNING.store(false, Ordering::Release);
    });
}

// ---------------------------------------------------------------- outcomes

/// Days after a merge during which a revert, a follow-up fix or a red CI run
/// marks the change as having been high risk.
pub const OUTCOME_WINDOW_DAYS: i64 = 7;

/// A merged PR as the outcome signals need it.
pub struct MergedPull<'a> {
    pub number: i64,
    pub title: &'a str,
    pub merge_sha: &'a str,
    pub files: &'a [String],
}

/// A later commit on the base branch; `files` is loaded only for fix candidates.
pub struct LaterCommit {
    pub sha: String,
    pub message: String,
    pub files: Option<Vec<String>>,
}

/// A conventional `fix:`/`hotfix:` commit (scope and `!` allowed).
pub fn is_fix_message(message: &str) -> bool {
    let head = message.lines().next().unwrap_or_default().trim().to_ascii_lowercase();
    let kind = head.split([':', '(', '!']).next().unwrap_or_default();
    (kind == "fix" || kind == "hotfix") && head.contains(':')
}

/// Whether `message` reverts this PR: git's own `This reverts commit <sha>`, or a
/// `Revert` subject naming the PR title or number.
pub fn reverts(pull: &MergedPull<'_>, message: &str) -> bool {
    if message.contains(&format!("This reverts commit {}", pull.merge_sha)) {
        return true;
    }
    let subject = message.lines().next().unwrap_or_default();
    subject.starts_with("Revert")
        && ((!pull.title.is_empty() && subject.contains(pull.title))
            || names_pull(subject, pull.number))
}

/// `#<number>` in `text`, not as the prefix of a longer number (`#42` vs `#421`).
fn names_pull(text: &str, number: i64) -> bool {
    let tag = format!("#{number}");
    text.match_indices(&tag).any(|(at, _)| {
        !text[at + tag.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    })
}

/// The post-merge signals that mark a change as having been high risk.
pub fn outcome_signals(pull: &MergedPull<'_>, later: &[LaterCommit], ci_failed: bool) -> Vec<String> {
    let mut signals = Vec::new();
    for commit in later.iter().filter(|c| c.sha != pull.merge_sha) {
        let short = &commit.sha[..commit.sha.len().min(12)];
        if reverts(pull, &commit.message) {
            signals.push(format!("reverted_by:{short}"));
        } else if is_fix_message(&commit.message) {
            let touches = commit
                .files
                .as_ref()
                .is_some_and(|files| files.iter().any(|f| pull.files.contains(f)));
            if touches {
                signals.push(format!("follow_up_fix:{short}"));
            }
        }
    }
    if ci_failed {
        signals.push("merge_commit_ci_failed".to_string());
    }
    signals
}

/// `high_risk` with any signal; `clean` once the window passed without one.
pub fn outcome_of(signals: &[String], window_elapsed: bool) -> &'static str {
    if !signals.is_empty() {
        "high_risk"
    } else if window_elapsed {
        "clean"
    } else {
        "pending"
    }
}

/// A check run that failed (statuses are normalized to check runs).
fn failed(run: &serde_json::Value) -> bool {
    matches!(
        run.get("conclusion").and_then(|v| v.as_str()),
        Some("failure" | "timed_out" | "startup_failure")
    )
}

/// The outcome of a merged PR read from GitHub: (outcome, signals).
pub async fn collect_outcome(
    token: &str,
    repository: &str,
    pull: &serde_json::Value,
) -> anyhow::Result<(&'static str, Vec<String>)> {
    let str_at = |path: &str| pull.pointer(path).and_then(|v| v.as_str()).unwrap_or_default();
    let (merged_at, merge_sha, base) = (
        str_at("/merged_at"),
        str_at("/merge_commit_sha"),
        str_at("/base/ref"),
    );
    if merged_at.is_empty() || merge_sha.len() != 40 {
        anyhow::bail!("shadow_pull_not_merged");
    }
    let number = pull.get("number").and_then(|v| v.as_i64()).unwrap_or_default();
    let merged = chrono::DateTime::parse_from_rfc3339(merged_at)?.with_timezone(&chrono::Utc);
    let window_end = merged + chrono::Duration::days(OUTCOME_WINDOW_DAYS);
    let now = chrono::Utc::now();
    let until = window_end.min(now).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let files: Vec<String> = super::connectors::list_github_pull_files(token, repository, number)
        .await?
        .into_iter()
        .map(|f| f.filename)
        .collect();
    let mut later = Vec::new();
    for commit in super::connectors::list_branch_commits(token, repository, base, merged_at, &until).await? {
        let sha = commit.get("sha").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let message = commit
            .pointer("/commit/message")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if sha.is_empty() || sha == merge_sha {
            continue;
        }
        let files = if is_fix_message(&message) {
            let detail = super::connectors::get_commit(token, repository, &sha).await?;
            Some(
                detail
                    .get("files")
                    .and_then(|v| v.as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|f| f.get("filename").and_then(|v| v.as_str()).map(str::to_string))
                    .collect(),
            )
        } else {
            None
        };
        later.push(LaterCommit { sha, message, files });
    }
    // An unreadable CI is an error (the decision stays pending and is retried),
    // never "CI did not fail".
    let ci_failed = super::connectors::list_commit_ci_runs(token, repository, merge_sha)
        .await?
        .iter()
        .any(failed);
    let summary = MergedPull {
        number,
        title: pull.get("title").and_then(|v| v.as_str()).unwrap_or_default(),
        merge_sha,
        files: &files,
    };
    let signals = outcome_signals(&summary, &later, ci_failed);
    Ok((outcome_of(&signals, now >= window_end), signals))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{connection::connect, factory_queries, migrations, queries};

    #[test]
    fn a_pull_summary_needs_a_full_head_sha() {
        let pull = serde_json::json!({
            "title": "Fix typo", "body": null,
            "head": {"sha": "0123456789abcdef0123456789abcdef01234567"}
        });
        let (title, body, head) = pull_summary(&pull).unwrap();
        assert_eq!((title.as_str(), body.as_str()), ("Fix typo", ""));
        assert_eq!(head.len(), 40);
        assert!(pull_summary(&serde_json::json!({"head": {"sha": "abc"}})).is_none());
    }

    #[test]
    fn one_shadow_decision_per_reviewed_head() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        let answer = jev::Answer {
            model: Some("jev-1.13.0".into()),
            task_class: "docs".into(),
            class_confidence: 1.0,
            risk: 0.0,
            risk_confidence: 0.95,
            needs_human: Some(0.4),
            input_tokens: Some(640),
        };
        let head = "0123456789abcdef0123456789abcdef01234567";
        let decision = factory_queries::ShadowDecision {
            provider: "jev",
            repository: "acme/app",
            pull_number: 7,
            head_sha: head,
            answer: &answer,
            verdict: jev::verdict(&answer),
            floor: None,
            latency_ms: 340,
        };
        assert!(!factory_queries::shadow_decision_exists(&conn, &org.id, "acme/app", 7, head).unwrap());
        assert!(factory_queries::record_shadow_decision(&conn, &org.id, &decision).unwrap());
        assert!(!factory_queries::record_shadow_decision(&conn, &org.id, &decision).unwrap());
        assert!(factory_queries::shadow_decision_exists(&conn, &org.id, "acme/app", 7, head).unwrap());
        let (verdict, outcome): (String, String) = conn
            .query_row("SELECT verdict, outcome FROM factory_shadow_decisions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        assert_eq!((verdict.as_str(), outcome.as_str()), ("allow", "pending"));
    }

    fn commit(sha: &str, message: &str, files: Option<&[&str]>) -> LaterCommit {
        LaterCommit {
            sha: sha.into(),
            message: message.into(),
            files: files.map(|f| f.iter().map(|x| x.to_string()).collect()),
        }
    }

    #[test]
    fn fix_messages_are_conventional_fix_or_hotfix() {
        assert!(is_fix_message("fix(api): handle null"));
        assert!(is_fix_message("hotfix: restore login\n\nbody"));
        assert!(is_fix_message("Fix!: breaking"));
        assert!(!is_fix_message("feat: fixes nothing"));
        assert!(!is_fix_message("prefix stuff: no"));
        assert!(!is_fix_message("fix the build"));
    }

    #[test]
    fn reverts_follow_up_fixes_and_red_ci_mark_high_risk() {
        let files = vec!["src/a.ts".to_string(), "src/b.ts".to_string()];
        let merge = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        let pull = MergedPull { number: 42, title: "Add refund button", merge_sha: merge, files: &files };
        let later = [
            commit("b1", &format!("Revert \"Add refund button\"\n\nThis reverts commit {merge}."), None),
            commit("c2", "fix(refund): rounding", Some(&["src/b.ts"])),
            commit("d3", "fix: unrelated", Some(&["src/z.ts"])),
            commit("e4", "feat: other work", None),
            commit(merge, "Add refund button (#42)", None),
        ];
        let signals = outcome_signals(&pull, &later, true);
        assert_eq!(signals, ["reverted_by:b1", "follow_up_fix:c2", "merge_commit_ci_failed"]);
        assert_eq!(outcome_of(&signals, false), "high_risk");
        assert_eq!(outcome_of(&[], false), "pending");
        assert_eq!(outcome_of(&[], true), "clean");
        // A revert subject naming the PR number counts too.
        assert!(reverts(&pull, "Revert #42: broke checkout"));
        assert!(!reverts(&pull, "Revert #421"), "a longer number is another PR");
    }

    #[test]
    fn the_report_counts_false_lows_and_lets_a_human_label_win() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        let record = |n: i64, risk: f64, outcome: &str, label: Option<&str>| {
            let answer = jev::Answer {
                model: None,
                task_class: "docs".into(),
                class_confidence: 1.0,
                risk,
                risk_confidence: 0.95,
                needs_human: None,
                input_tokens: None,
            };
            let head = format!("{n:040}");
            factory_queries::record_shadow_decision(
                &conn,
                &org.id,
                &factory_queries::ShadowDecision {
                    provider: "jev",
                    repository: "acme/app",
                    pull_number: n,
                    head_sha: &head,
                    answer: &answer,
                    verdict: jev::verdict(&answer),
                    floor: None,
                    latency_ms: 1,
                },
            )
            .unwrap();
            let id = factory_queries::shadow_decision_id(&conn, &org.id, "acme/app", n, &head)
                .unwrap()
                .unwrap();
            factory_queries::set_shadow_outcome(&conn, &org.id, &id, None, None, outcome, &[]).unwrap();
            if let Some(label) = label {
                conn.execute(
                    "UPDATE factory_shadow_decisions SET human_label = ?1 WHERE id = ?2",
                    [label, &id],
                )
                .unwrap();
            }
        };
        record(1, 0.0, "clean", None); // allowed, fine
        record(2, 0.0, "high_risk", None); // allowed, false low
        record(3, 0.0, "high_risk", Some("low")); // flaky CI: the human says low
        record(4, 0.0, "pending", None); // allowed, not settled
        record(5, 0.0, "pending", Some("high")); // a human settles it as high
        record(6, 1.0, "high_risk", None); // held: not a false low
        record(7, 0.0, "not_merged", None); // never merged: excluded
        let report = factory_queries::shadow_report(&conn, &org.id).unwrap();
        assert_eq!(report.len(), 1);
        let docs = &report[0];
        assert_eq!((docs.decisions, docs.allowed, docs.settled_allowed, docs.false_low), (6, 5, 4, 2));
        assert_eq!(docs.false_low_rate, Some(0.5));
        assert!(!docs.meets_od5);
    }

    #[test]
    fn only_listed_orgs_send_pull_requests_to_the_decision_model() {
        let list = "aaa, bbb";
        assert!(org_listed(list, "aaa"));
        assert!(org_listed(list, "bbb"));
        assert!(!org_listed(list, "ccc"));
        assert!(!org_listed("", "aaa"));
        assert!(!org_listed(",", ""));
    }

    #[test]
    fn superseded_unresolvable_and_in_window_rows_stay_out_of_the_way() {
        let conn = connect(":memory:").unwrap();
        migrations::run_all(&conn).unwrap();
        let (org, _, _) = queries::bootstrap(&conn, "Acme", "acme", "a@acme.com", "A").unwrap();
        let answer = jev::Answer {
            model: None,
            task_class: "docs".into(),
            class_confidence: 1.0,
            risk: 0.0,
            risk_confidence: 0.95,
            needs_human: None,
            input_tokens: None,
        };
        let mut ids = Vec::new();
        for n in 1..=4 {
            let head = format!("{n:040}");
            factory_queries::record_shadow_decision(
                &conn,
                &org.id,
                &factory_queries::ShadowDecision {
                    provider: "jev",
                    repository: "acme/app",
                    pull_number: n,
                    head_sha: &head,
                    answer: &answer,
                    verdict: jev::verdict(&answer),
                    floor: None,
                    latency_ms: 1,
                },
            )
            .unwrap();
            ids.push(factory_queries::shadow_decision_id(&conn, &org.id, "acme/app", n, &head).unwrap().unwrap());
        }
        // 1: a later head was merged. 2: merged an hour ago, inside its window.
        // 3: fails every refresh. 4: open PR, still pending.
        factory_queries::set_shadow_outcome(&conn, &org.id, &ids[0], Some("2026-01-01T00:00:00Z"), None, "superseded", &[]).unwrap();
        let recent = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        factory_queries::set_shadow_outcome(&conn, &org.id, &ids[1], Some(&recent), None, "pending", &[]).unwrap();
        for _ in 0..factory_queries::SHADOW_MAX_REFRESH_FAILURES {
            factory_queries::record_shadow_refresh_failure(&conn, &org.id, &ids[2]).unwrap();
        }
        let pending: Vec<String> = factory_queries::list_pending_shadow_decisions(&conn, &org.id, 50)
            .unwrap()
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(pending, [ids[3].clone()], "only the open PR is checked now");
        let outcome: String = conn
            .query_row("SELECT outcome FROM factory_shadow_decisions WHERE id = ?1", [&ids[2]], |r| r.get(0))
            .unwrap();
        assert_eq!(outcome, "unresolvable");
        // Superseded and unresolvable rows leave the report; pending ones stay.
        let report = factory_queries::shadow_report(&conn, &org.id).unwrap();
        assert_eq!(report[0].decisions, 2);
    }
}
