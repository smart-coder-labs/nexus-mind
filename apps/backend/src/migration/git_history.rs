//! The git-history connector: the *why*, which the code index cannot hold.
//!
//! `POST /v1/code/index` answers what the code does today. It does not answer
//! why it is that way, and that is exactly what an agent loses and a human
//! spends months reconstructing.
//!
//! The why lives in the history: the commit saying "revert: back to X because Y
//! broke Z", the merge whose body weighs two alternatives before picking one.
//! `indexer::walker` walks the current working tree; it never reads a commit.
//!
//! # The deterministic prefilter is the design, not an optimisation
//!
//! Calling a model once per commit over years of history is expensive and,
//! worse, pointless: `chore(deps): bump serde` carries no knowledge. The filter
//! runs **before** any classification and costs zero tokens. This repository has
//! 452 commits and most of them are noise.
//!
//! # No network, no credentials
//!
//! Everything here is local `git`. Forge enrichment (PR comments, reviews) is
//! deliberately out of this delivery — see `design.md` §4 — because it cannot be
//! tested in CI without mocking the thing under test, and the merge subject and
//! body already carry the PR title and description in most cases.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::process::Command;

use super::redact::redact;
use super::{CandidatePayload, Connector, ScanOptions, SourceItem};

/// Subject prefixes that mark routine maintenance.
const CHORE_PREFIXES: &[&str] = &[
    "chore", "style", "ci", "build", "wip", "bump", "deps", "typo", "format", "fmt", "lint",
];

/// Author fragments that mark an automation account.
const BOT_MARKERS: &[&str] = &["[bot]", "dependabot", "renovate", "github-actions", "semantic-release"];

/// Below this, a body says nothing the subject did not already say.
const MIN_BODY_CHARS: usize = 40;

/// Why a commit was filtered out. Reported, never silently dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterReason {
    RoutineMaintenance,
    BotAuthor,
    NoSubstance,
    AbsorbedByMerge,
}

impl FilterReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FilterReason::RoutineMaintenance => "routine maintenance — no decision behind it",
            FilterReason::BotAuthor => "authored by automation — no human decision behind it",
            FilterReason::NoSubstance => "no body beyond the subject — nothing to learn",
            FilterReason::AbsorbedByMerge => "part of a merged group, represented by its merge",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub date: String,
    pub parents: Vec<String>,
}

impl Commit {
    pub fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    /// The PR number a merge subject names, in either shape git produces:
    /// `Merge pull request #250 from …` or `Some title (#252)`.
    pub fn pr_number(&self) -> Option<u64> {
        let s = &self.subject;
        let idx = s.find('#')?;
        let digits: String = s[idx + 1..].chars().take_while(|c| c.is_ascii_digit()).collect();
        digits.parse().ok()
    }

    fn substantive_body_len(&self) -> usize {
        self.body
            .lines()
            .filter(|l| {
                let t = l.trim();
                // Trailers and generated footers are not explanation.
                !t.is_empty()
                    && !t.starts_with("Co-Authored-By:")
                    && !t.starts_with("Signed-off-by:")
                    && !t.starts_with("Co-authored-by:")
                    && !t.starts_with('#')
            })
            .map(|l| l.trim().len())
            .sum()
    }
}

/// Is this commit mechanical? Runs before any model is called.
pub fn filter_reason(commit: &Commit) -> Option<FilterReason> {
    let author = commit.author.to_lowercase();
    if BOT_MARKERS.iter().any(|m| author.contains(m)) {
        return Some(FilterReason::BotAuthor);
    }

    let subject = commit.subject.to_lowercase();
    let prefix: String = subject
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    if CHORE_PREFIXES.contains(&prefix.as_str()) {
        return Some(FilterReason::RoutineMaintenance);
    }
    // `Merge branch 'x'` with nothing else is bookkeeping.
    if subject.starts_with("merge branch") && commit.substantive_body_len() < MIN_BODY_CHARS {
        return Some(FilterReason::RoutineMaintenance);
    }

    // A merge WITH a body survives: that is exactly where the alternatives were
    // weighed. A merge without one says nothing about what was decided.
    if commit.substantive_body_len() < MIN_BODY_CHARS {
        return Some(FilterReason::NoSubstance);
    }

    None
}

pub struct GitHistoryConnector {
    /// The repository's own directory name — never an absolute path.
    pub repo: String,
    /// Scan only what came after this commit, for a second pass over a
    /// long-lived repository.
    pub since_commit: Option<String>,
    /// Hard cap on how far back to read, so a first run on a decade-old repo
    /// does not read a decade.
    pub max_commits: usize,
}

impl GitHistoryConnector {
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            since_commit: None,
            max_commits: 2000,
        }
    }

    pub fn since(mut self, commit: Option<String>) -> Self {
        self.since_commit = commit;
        self
    }

    pub fn repo_name_for(root: &str) -> String {
        std::path::Path::new(root)
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "repo".to_string())
    }

    fn git(&self, root: &str, args: &[&str]) -> Result<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(args)
            .output()
            .with_context(|| format!("could not run git in {root}"))?;
        if !out.status.success() {
            anyhow::bail!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    }

    pub fn is_repository(&self, root: &str) -> bool {
        self.git(root, &["rev-parse", "--git-dir"]).is_ok()
    }

    /// Read the history. Uses a record separator no commit message contains, so
    /// a body with blank lines cannot be mistaken for a record boundary.
    /// Turns `--include` / `--exclude` into a git pathspec.
    ///
    /// # Why git does the matching and not this connector
    ///
    /// For a commit, "include this path" can only sensibly mean "commits that
    /// touched it" — and answering that in Rust would need the file list of
    /// every commit, i.e. one `git show` per commit over the whole history.
    /// `git log -- <pathspec>` already does it, prunes as it walks, and applies
    /// `-n` to *matching* commits, so a capped scan returns 200 relevant
    /// commits instead of 200 commits of which three are relevant.
    ///
    /// The cost is that matching follows git's pathspec rules rather than the
    /// substring rules the file-based connectors use. That is the right trade:
    /// re-implementing pathspec matching would be a worse version of something
    /// git is authoritative about, and `docs/adr` means the same thing under
    /// both readings.
    ///
    /// Unlike the file connectors, commits filtered out here are never read at
    /// all, so they are absent rather than listed in the exclusion report. That
    /// is the point — reading them to report them would undo the saving.
    fn pathspec(opts: &ScanOptions) -> Vec<String> {
        if opts.includes.is_empty() && opts.excludes.is_empty() {
            return Vec::new();
        }
        let mut spec = vec!["--".to_string()];
        if opts.includes.is_empty() {
            // An exclude with no include means "everything except"; git needs
            // something to subtract from.
            spec.push(".".to_string());
        }
        spec.extend(opts.includes.iter().cloned());
        spec.extend(opts.excludes.iter().map(|e| format!(":(exclude){e}")));
        spec
    }

    pub fn read_commits(&self, root: &str, opts: &ScanOptions) -> Result<Vec<Commit>> {
        const REC: &str = "\u{1e}";
        const FLD: &str = "\u{1f}";
        let format = format!("--format={REC}%H{FLD}%P{FLD}%an{FLD}%aI{FLD}%s{FLD}%b");

        // Every branch, not just the checked-out one. A decision can live on a
        // branch that was never merged into HEAD, and scanning only HEAD would
        // silently miss it. `HEAD` stays in the set so a detached checkout with
        // no local branch refs still yields its own history; tags and stash are
        // left out — a tag is not a branch and a stashed WIP is not a decision.
        // A `--since` commit becomes a negative ref (`^sha`), excluding its
        // ancestors across every branch, which generalises the old `sha..HEAD`.
        let max = self.max_commits.to_string();
        let mut args: Vec<String> = vec!["log".into(), format, "-n".into(), max];
        args.push("HEAD".into());
        args.push("--branches".into());
        args.push("--remotes".into());
        if let Some(c) = self.since_commit.as_ref() {
            args.push(format!("^{c}"));
        }
        let pathspec = Self::pathspec(opts);
        if !pathspec.is_empty() {
            // Without this, git simplifies merges away when a pathspec is given
            // and the PR grouping below would silently stop working — every
            // commit of a filtered PR would arrive on its own, which is the
            // exact failure `absorbed_by` exists to prevent.
            args.push("--full-history".into());
            args.extend(pathspec);
        }
        let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
        let raw = self.git(root, &borrowed)?;

        let mut commits = Vec::new();
        for record in raw.split(REC).skip(1) {
            let fields: Vec<&str> = record.splitn(6, FLD).collect();
            if fields.len() < 6 {
                continue;
            }
            commits.push(Commit {
                sha: fields[0].trim().to_string(),
                parents: fields[1]
                    .split_whitespace()
                    .map(str::to_string)
                    .collect(),
                author: fields[2].trim().to_string(),
                date: fields[3].trim().to_string(),
                subject: fields[4].trim().to_string(),
                body: fields[5].trim_end().to_string(),
            });
        }
        Ok(commits)
    }

    /// Commits a merge brought in — its second-parent side.
    ///
    /// A PR of thirty commits is ONE decision, not thirty. Without grouping,
    /// that decision shows up thirty times in the review queue and the reviewer
    /// gives up — the failure mode `repo-docs` measured at 3377 candidates.
    fn absorbed_by(&self, root: &str, merge: &Commit) -> HashSet<String> {
        if merge.parents.len() < 2 {
            return HashSet::new();
        }
        let range = format!("{}..{}", merge.parents[0], merge.parents[1]);
        self.git(root, &["rev-list", &range])
            .map(|out| out.split_whitespace().map(str::to_string).collect())
            .unwrap_or_default()
    }

    pub fn identity(&self, commit: &Commit) -> String {
        // No content hash, unlike the other connectors: a commit does not
        // change. Rewriting history produces a different SHA, which is a
        // different commit. The asymmetry is correct and worth stating.
        match commit.pr_number().filter(|_| commit.is_merge()) {
            Some(pr) => format!("git:{}:pr:{}", self.repo, pr),
            None => format!("git:{}:{}", self.repo, commit.sha),
        }
    }
}

/// What the commit's shape proposes.
pub fn propose_memory_type(commit: &Commit) -> (&'static str, bool) {
    let subject = commit.subject.to_lowercase();
    let is_revert = subject.starts_with("revert") || subject.contains("revert:");
    if is_revert {
        return ("decision", true);
    }
    let prefix: String = subject
        .chars()
        .take_while(|c| c.is_ascii_alphabetic())
        .collect();
    match prefix.as_str() {
        "fix" | "bugfix" | "hotfix" => ("bugfix", false),
        "feat" | "refactor" | "perf" => ("decision", false),
        "docs" => ("architecture", false),
        _ => ("architecture", false),
    }
}

impl Connector for GitHistoryConnector {
    fn source_kind(&self) -> &'static str {
        "git-history"
    }

    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.scan_report(opts)?.items)
    }

    fn scan_report(&self, opts: &ScanOptions) -> Result<super::ScanReport> {
        let mut report = super::ScanReport::default();

        if !self.is_repository(&opts.root) {
            anyhow::bail!(
                "not_a_repository: {} is not a git repository",
                opts.root
            );
        }

        let commits = self.read_commits(&opts.root, opts)?;

        // Anything a merge brought in is represented by that merge.
        let mut absorbed: HashSet<String> = HashSet::new();
        for commit in commits.iter().filter(|c| c.is_merge()) {
            if filter_reason(commit).is_none() {
                absorbed.extend(self.absorbed_by(&opts.root, commit));
            }
        }

        let mut items = Vec::new();
        for (seen, commit) in commits.iter().enumerate() {
            opts.note(seen + 1, &commit.sha);
            if !commit.is_merge() && absorbed.contains(&commit.sha) {
                report.excluded.push((
                    commit.sha.clone(),
                    FilterReason::AbsorbedByMerge.as_str().to_string(),
                ));
                continue;
            }
            if let Some(reason) = filter_reason(commit) {
                report
                    .excluded
                    .push((commit.sha.clone(), reason.as_str().to_string()));
                continue;
            }

            let raw = format!("{}\n\n{}", commit.subject, commit.body);
            items.push(SourceItem {
                source_identity: self.identity(commit),
                display_origin: format!("{} — {}", &commit.sha[..7.min(commit.sha.len())], commit.subject),
                routing_path: None,
                raw,
                meta: serde_json::json!({
                    "sha": commit.sha,
                    "subject": commit.subject,
                    "author": commit.author,
                    "date": commit.date,
                    "is_merge": commit.is_merge(),
                    "pr": commit.pr_number(),
                }),
            });
        }

        report.documents = items.len();
        report.units = items.len();
        report.bytes = items.iter().map(|i| i.raw.len()).sum();
        report.items = items;
        Ok(report)
    }

    fn classify_prompt(&self, item: &SourceItem) -> String {
        let (redacted, _) = redact(&item.raw);
        let date = item.meta.get("date").and_then(|v| v.as_str()).unwrap_or("");
        format!(
            "You are reading one change from a software project's history so its reasoning can \
             be PROPOSED — never committed — as team knowledge. A human reviews everything.\n\n\
             Date: {date}\n\n---\n{redacted}\n---\n\n\
             Return ONE JSON object: {{\"source_identity\": \"\", \"destination_kind\": \
             \"memory|skip\", \"content\": \"...\", \"source_excerpt\": \"...\", \
             \"confidence\": 0.0, \"destination_hint\": {{\"title\": \"...\", \"type\": \"...\"}}}}\n\n\
             Rules:\n\
             1. PROPOSE, do not decide.\n\
             2. Capture the WHY, not the what. The diff already says what changed; this message \
             is the only place the reasoning survives.\n\
             3. `source_excerpt` MUST be copied verbatim from the message above.\n\
             4. If the message explains nothing — a mechanical change, a message that only \
             restates its own subject — return \"skip\" and say why.",
        )
    }

    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload> {
        let (content, report) = redact(&item.raw);
        let (mem_type, is_revert) = {
            let subject = item.meta.get("subject").and_then(|v| v.as_str()).unwrap_or("");
            propose_memory_type(&Commit {
                sha: String::new(),
                subject: subject.to_string(),
                body: String::new(),
                author: String::new(),
                date: String::new(),
                parents: vec![],
            })
        };
        let date = item.meta.get("date").and_then(|v| v.as_str()).unwrap_or("");
        let subject = item.meta.get("subject").and_then(|v| v.as_str()).unwrap_or("");

        let mut hint = serde_json::json!({
            "title": subject,
            "type": mem_type,
            // A reviewer must be able to weigh whether a decision from two years
            // ago still holds. The machine does not guess what aged; it shows
            // the date and lets the human decide.
            "occurred_at": date,
            "sha": item.meta.get("sha"),
            "redaction": report.summary(),
        });
        if is_revert {
            // Still knowledge, but historical knowledge. Detecting the full
            // semantics of a revert is out of scope; saying it is one is not.
            hint["reverts_earlier_work"] = serde_json::json!(true);
        }
        if let Some(pr) = item.meta.get("pr").and_then(|v| v.as_u64()) {
            hint["pull_request"] = serde_json::json!(pr);
        }

        Some(CandidatePayload {
            source_identity: item.source_identity.clone(),
            destination_kind: "memory".to_string(),
            content: content.clone(),
            source_excerpt: Some(
                content
                    .lines()
                    .filter(|l| !l.trim().is_empty())
                    .take(4)
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            destination_hint: hint,
            confidence: None,
            provenance_kind: Some("verified_manifest".to_string()),
        })
    }
}

/// Unused today, kept because the identity scheme documents it: a content hash
/// is deliberately absent from git identities.
#[allow(dead_code)]
fn content_sha16(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git must be available");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn commit(dir: &std::path::Path, file: &str, subject: &str, body: &str, author: &str) {
        std::fs::write(dir.join(file), format!("{subject}\n")).unwrap();
        git(dir, &["add", "."]);
        let message = if body.is_empty() {
            subject.to_string()
        } else {
            format!("{subject}\n\n{body}")
        };
        git(
            dir,
            &[
                "-c",
                &format!("user.name={author}"),
                "-c",
                "user.email=dev@example.com",
                "commit",
                "-m",
                &message,
            ],
        );
    }

    /// Like `commit`, but for a path in a subdirectory.
    fn commit_at(dir: &std::path::Path, rel: &str, subject: &str, body: &str) {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{subject}\n")).unwrap();
        git(dir, &["add", "."]);
        git(
            dir,
            &["commit", "-m", &format!("{subject}\n\n{body}")],
        );
    }

    fn subjects_of(report: &super::super::ScanReport) -> Vec<String> {
        report
            .items
            .iter()
            .map(|i| i.meta["subject"].as_str().unwrap_or("").to_string())
            .collect()
    }

    fn repo() -> TempDir {
        let dir = TempDir::new().unwrap();
        git(dir.path(), &["init", "-q", "-b", "main"]);
        git(dir.path(), &["config", "user.name", "Dev"]);
        git(dir.path(), &["config", "user.email", "dev@example.com"]);
        dir
    }

    fn connector() -> GitHistoryConnector {
        GitHistoryConnector::new("testrepo")
    }

    fn opts(dir: &TempDir) -> ScanOptions {
        ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            includes: vec![],
            excludes: vec![],
            ..Default::default()
}
    }

    const REAL_BODY: &str = "The pod was never restarted because the workflow \
                             lacked a rollout step, so the new image sat unused.";

    // ── Reading history ──────────────────────────────────────────────────────

    #[test]
    fn a_repository_without_a_remote_still_scans() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let items = connector().scan(&opts(&dir)).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].raw.contains("rollout step"));
    }

    #[test]
    fn a_non_repository_path_is_refused_clearly() {
        let dir = TempDir::new().unwrap();
        let err = connector().scan(&opts(&dir)).unwrap_err();
        assert!(
            err.to_string().contains("not_a_repository"),
            "the refusal must name the problem: {err}"
        );
    }

    // ── The prefilter ────────────────────────────────────────────────────────

    #[test]
    fn chores_bots_and_bodyless_commits_are_filtered_without_a_model() {
        let dir = repo();
        commit(dir.path(), "a.txt", "chore(deps): bump serde", "Routine dependency bump for the week.", "Dev");
        commit(dir.path(), "b.txt", "feat: add widget", "", "Dev");
        commit(dir.path(), "c.txt", "fix: patch it", REAL_BODY, "dependabot[bot]");
        commit(dir.path(), "d.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");

        let report = connector().scan_report(&opts(&dir)).unwrap();
        assert_eq!(report.units, 1, "only the substantial human commit survives");
        assert_eq!(report.excluded.len(), 3);

        let reasons: Vec<&str> = report.excluded.iter().map(|(_, r)| r.as_str()).collect();
        assert!(reasons.iter().any(|r| r.contains("routine maintenance")));
        assert!(reasons.iter().any(|r| r.contains("automation")));
        assert!(reasons.iter().any(|r| r.contains("no body beyond the subject")));
    }

    /// The filter must not eat the thing it exists to find.
    #[test]
    fn a_commit_with_a_real_explanation_survives() {
        let dir = repo();
        commit(
            dir.path(),
            "a.txt",
            "refactor: move the connector contract to the library",
            "Inside the binary it is only reachable by `cargo test --bin`, which \
             does not see the rest of the suite.",
            "Dev",
        );
        assert_eq!(connector().scan(&opts(&dir)).unwrap().len(), 1);
    }

    #[test]
    fn the_filter_reports_what_it_removed() {
        let dir = repo();
        commit(dir.path(), "a.txt", "chore: tidy", "Just tidying up the imports here.", "Dev");
        let report = connector().scan_report(&opts(&dir)).unwrap();
        assert_eq!(report.units, 0);
        assert_eq!(report.excluded.len(), 1);
        let (sha, reason) = &report.excluded[0];
        assert!(!sha.is_empty() && !reason.is_empty());
    }

    // ── Identity ─────────────────────────────────────────────────────────────

    #[test]
    fn identity_is_the_commit_sha_and_is_stable() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let first = connector().scan(&opts(&dir)).unwrap();
        let second = connector().scan(&opts(&dir)).unwrap();
        assert_eq!(first[0].source_identity, second[0].source_identity);
        assert!(first[0].source_identity.starts_with("git:testrepo:"));
    }

    #[test]
    fn identity_never_contains_an_absolute_path() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let root = dir.path().to_string_lossy().to_string();
        for item in connector().scan(&opts(&dir)).unwrap() {
            assert!(!item.source_identity.contains(&root));
            assert!(!item.source_identity.to_lowercase().contains("/users/"));
        }
    }

    #[test]
    fn scanning_is_incremental_from_a_given_commit() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(one): first change", REAL_BODY, "Dev");
        let all = connector().scan(&opts(&dir)).unwrap();
        let first_sha = all[0].meta["sha"].as_str().unwrap().to_string();

        commit(dir.path(), "b.txt", "fix(two): second change", REAL_BODY, "Dev");
        let everything = connector().scan(&opts(&dir)).unwrap();
        assert_eq!(everything.len(), 2);

        let incremental = GitHistoryConnector::new("testrepo")
            .since(Some(first_sha))
            .scan(&opts(&dir))
            .unwrap();
        assert_eq!(incremental.len(), 1, "only what came after");
        assert!(incremental[0].meta["subject"].as_str().unwrap().contains("second"));
    }

    /// A decision on a branch that was never merged into HEAD must still be
    /// found: the scan walks every branch, not just the checked-out one.
    #[test]
    fn commits_on_an_unmerged_branch_are_scanned() {
        let dir = repo();
        commit(dir.path(), "base.txt", "feat: base", REAL_BODY, "Dev");
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit(
            dir.path(),
            "f.txt",
            "feat(feature): a decision made off main",
            REAL_BODY,
            "Dev",
        );
        // Back on main, with `feature` left UNMERGED — the old HEAD-only scan
        // would never have seen its commit.
        git(dir.path(), &["checkout", "-q", "main"]);

        let subjects: Vec<String> = connector()
            .scan(&opts(&dir))
            .unwrap()
            .iter()
            .map(|u| u.meta["subject"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(
            subjects.iter().any(|s| s.contains("a decision made off main")),
            "the unmerged branch's commit must be scanned: {subjects:?}"
        );
    }

    // ── Grouping ─────────────────────────────────────────────────────────────

    /// A PR of thirty commits is ONE decision. Without this the reviewer sees it
    /// thirty times and gives up.
    #[test]
    fn a_merged_group_produces_one_unit_not_one_per_commit() {
        let dir = repo();
        commit(dir.path(), "base.txt", "feat: base", "Setting up the baseline for the work.", "Dev");
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit(dir.path(), "f1.txt", "feat: part one", "First half of the feature, explained at length here.", "Dev");
        commit(dir.path(), "f2.txt", "feat: part two", "Second half of the feature, explained at length here.", "Dev");
        git(dir.path(), &["checkout", "-q", "main"]);
        git(
            dir.path(),
            &[
                "merge",
                "--no-ff",
                "feature",
                "-m",
                "Merge pull request #42 from feature\n\nWe chose the streaming approach over \
                 batching because the batch window made latency unpredictable.",
            ],
        );

        let report = connector().scan_report(&opts(&dir)).unwrap();
        let subjects: Vec<&str> = report
            .items
            .iter()
            .map(|i| i.meta["subject"].as_str().unwrap_or(""))
            .collect();

        assert!(
            subjects.iter().any(|s| s.contains("#42")),
            "the merge represents the group: {subjects:?}"
        );
        assert!(
            !subjects.iter().any(|s| s.contains("part one") || s.contains("part two")),
            "the absorbed commits must not each produce a unit: {subjects:?}"
        );

        let merge_item = report.items.iter().find(|i| i.meta["pr"].as_u64() == Some(42)).unwrap();
        assert_eq!(merge_item.source_identity, "git:testrepo:pr:42");

        assert!(report
            .excluded
            .iter()
            .any(|(_, r)| r.contains("merged group")));
    }

    // ── Narrowing by path ────────────────────────────────────────────────────

    /// `--include` and `--exclude` were accepted and ignored here, exactly as
    /// they were in `claude-memories`: a request for one area scanned — and
    /// billed for — the whole history.
    fn history_across_two_areas() -> TempDir {
        let dir = repo();
        commit_at(
            dir.path(),
            "docs/adr/ADR-001.md",
            "docs: record the storage decision",
            "We chose SQLite because the deployment target is a single node.",
        );
        commit_at(
            dir.path(),
            "src/api/handler.rs",
            "fix(api): stop dropping the request id",
            "The header was read before the middleware ran, so it was always empty.",
        );
        // Deliberately NOT a `chore:` subject. With one, the chore filter would
        // drop it anyway and the exclusion tests below would pass without the
        // pathspec doing anything at all.
        commit_at(
            dir.path(),
            "vendor/lib/thing.rs",
            "fix(vendor): patch the bundled parser",
            "Upstream mis-handles CRLF, so the fork carries a patch until 2.1 ships.",
        );
        dir
    }

    #[test]
    fn include_narrows_history_to_commits_touching_those_paths() {
        let dir = history_across_two_areas();
        let narrowed = ScanOptions {
            includes: vec!["docs".to_string()],
            ..opts(&dir)
        };
        let subjects = subjects_of(&connector().scan_report(&narrowed).unwrap());
        assert_eq!(subjects.len(), 1, "{subjects:?}");
        assert!(subjects[0].contains("storage decision"), "{subjects:?}");
    }

    #[test]
    fn exclude_drops_commits_that_only_touch_excluded_paths() {
        let dir = history_across_two_areas();
        let filtered = ScanOptions {
            excludes: vec!["vendor".to_string()],
            ..opts(&dir)
        };
        let subjects = subjects_of(&connector().scan_report(&filtered).unwrap());
        assert!(
            !subjects.iter().any(|s| s.contains("vendor")),
            "{subjects:?}"
        );
        assert_eq!(subjects.len(), 2, "the other two survive: {subjects:?}");
    }

    /// An exclude alone must subtract from everything, not from nothing.
    #[test]
    fn an_exclude_without_an_include_still_reads_the_rest() {
        let dir = history_across_two_areas();
        let filtered = ScanOptions {
            excludes: vec!["docs".to_string(), "vendor".to_string()],
            ..opts(&dir)
        };
        let subjects = subjects_of(&connector().scan_report(&filtered).unwrap());
        assert_eq!(subjects.len(), 1, "{subjects:?}");
        assert!(subjects[0].contains("request id"), "{subjects:?}");
    }

    /// A commit that touches both keeps its place: the include is what it
    /// matched on, and one excluded file alongside does not disqualify it.
    #[test]
    fn a_commit_touching_both_included_and_excluded_paths_is_kept() {
        let dir = repo();
        let root = dir.path();
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::create_dir_all(root.join("vendor")).unwrap();
        std::fs::write(root.join("docs/note.md"), "a\n").unwrap();
        std::fs::write(root.join("vendor/dep.rs"), "b\n").unwrap();
        git(root, &["add", "."]);
        git(
            root,
            &[
                "commit",
                "-m",
                "feat: document the vendored dependency\n\nExplains why the fork exists and \
                 when it can be dropped again.",
            ],
        );
        let filtered = ScanOptions {
            includes: vec!["docs".to_string()],
            excludes: vec!["vendor".to_string()],
            ..opts(&dir)
        };
        assert_eq!(subjects_of(&connector().scan_report(&filtered).unwrap()).len(), 1);
    }

    #[test]
    fn no_filters_still_reads_the_whole_history() {
        let dir = history_across_two_areas();
        let subjects = subjects_of(&connector().scan_report(&opts(&dir)).unwrap());
        assert_eq!(subjects.len(), 3, "{subjects:?}");
    }

    /// A pathspec makes git simplify merges away by default, which would
    /// silently undo the PR grouping. `--full-history` is what keeps one PR
    /// worth one candidate even when the scan is narrowed.
    #[test]
    fn pr_grouping_survives_a_pathspec() {
        let dir = repo();
        commit_at(dir.path(), "docs/base.md", "docs: base", "Baseline for the work described here.");
        git(dir.path(), &["checkout", "-q", "-b", "feature"]);
        commit_at(dir.path(), "docs/one.md", "docs: part one", "First half, explained at length here.");
        commit_at(dir.path(), "docs/two.md", "docs: part two", "Second half, explained at length here.");
        git(dir.path(), &["checkout", "-q", "main"]);
        git(
            dir.path(),
            &[
                "merge",
                "--no-ff",
                "feature",
                "-m",
                "Merge pull request #7 from feature\n\nWe chose the streaming approach over \
                 batching because the batch window made latency unpredictable.",
            ],
        );

        let narrowed = ScanOptions {
            includes: vec!["docs".to_string()],
            ..opts(&dir)
        };
        let subjects = subjects_of(&connector().scan_report(&narrowed).unwrap());
        assert!(subjects.iter().any(|s| s.contains("#7")), "{subjects:?}");
        assert!(
            !subjects.iter().any(|s| s.contains("part one") || s.contains("part two")),
            "the absorbed commits must not each produce a unit: {subjects:?}"
        );
    }

    // ── Mapping ──────────────────────────────────────────────────────────────

    #[test]
    fn fix_with_a_cause_proposes_a_bugfix_memory() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let cand = c.fallback(&items[0]).unwrap();
        assert_eq!(cand.destination_kind, "memory");
        assert_eq!(cand.destination_hint["type"], serde_json::json!("bugfix"));
    }

    #[test]
    fn a_revert_is_marked_as_such() {
        let dir = repo();
        commit(
            dir.path(),
            "a.txt",
            "revert: go back to the batching approach",
            "The streaming rewrite made p99 worse under load, so we are reverting it.",
            "Dev",
        );
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let cand = c.fallback(&items[0]).unwrap();
        assert_eq!(cand.destination_hint["type"], serde_json::json!("decision"));
        assert_eq!(
            cand.destination_hint["reverts_earlier_work"],
            serde_json::json!(true),
            "still knowledge, but historical — and the reviewer must see that"
        );
    }

    /// A reviewer has to be able to weigh whether a decision from two years ago
    /// still holds. Only the connector has that information.
    #[test]
    fn every_candidate_carries_the_date_of_the_work() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let c = connector();
        for item in c.scan(&opts(&dir)).unwrap() {
            let cand = c.fallback(&item).unwrap();
            let date = cand.destination_hint["occurred_at"].as_str().unwrap_or("");
            assert!(date.contains('T') && date.len() >= 10, "expected an ISO date, got {date:?}");
        }
    }

    #[test]
    fn credentials_in_commit_messages_are_redacted() {
        let dir = repo();
        commit(
            dir.path(),
            "a.txt",
            "fix(ci): rotate the token",
            "The old token ghp_abcdefghijklmnopqrstuvwxyz01 leaked into the log output.",
            "Dev",
        );
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let cand = c.fallback(&items[0]).unwrap();
        assert!(!cand.content.contains("ghp_"), "the token must not reach staging");
        assert!(cand.destination_hint["redaction"].as_str().unwrap().contains("credential"));
    }

    #[test]
    fn the_prompt_asks_for_the_why_and_allows_skipping() {
        let dir = repo();
        commit(dir.path(), "a.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let prompt = c.classify_prompt(&items[0]);
        assert!(prompt.contains("Capture the WHY"));
        assert!(prompt.contains("PROPOSE, do not decide"));
        assert!(prompt.contains("skip"));
        assert!(prompt.contains("rollout step"), "the message itself must be in the prompt");
    }

    #[test]
    fn dry_run_reports_examined_surviving_and_estimated_tokens() {
        let dir = repo();
        commit(dir.path(), "a.txt", "chore: tidy", "Tidying the imports across the tree.", "Dev");
        commit(dir.path(), "b.txt", "fix(deploy): restart the pod", REAL_BODY, "Dev");
        let report = connector().scan_report(&opts(&dir)).unwrap();
        assert_eq!(report.units, 1);
        assert_eq!(report.excluded.len(), 1);
        assert!(report.bytes > 0 && report.estimated_tokens() > 0);
    }

    // ── Against this repository ──────────────────────────────────────────────

    /// Runs over this checkout's real history. Asserts properties, not counts:
    /// that the filter removes a substantial share, that no identity leaks a path,
    /// and — the calibration check — that the knowledge-migration commits survive.
    /// They have long bodies explaining decisions; a filter that discarded them
    /// would be mis-tuned.
    #[test]
    fn scanning_this_repository_filters_most_of_its_history() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repo root")
            .to_string_lossy()
            .to_string();

        let c = GitHistoryConnector::new(GitHistoryConnector::repo_name_for(&root));
        if !c.is_repository(&root) {
            return; // packaged crate without a checkout
        }
        // A shallow clone has no history to calibrate against. CI fetches the
        // full history for this job precisely so this test can run; anywhere
        // else, skipping is the honest outcome rather than asserting against
        // one commit.
        let shallow = std::process::Command::new("git")
            .args(["-C", &root, "rev-parse", "--is-shallow-repository"])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
            .unwrap_or(false);
        if shallow {
            eprintln!("skipping: shallow clone has no history to calibrate against");
            return;
        }

        let report = c
            .scan_report(&ScanOptions {
                root: root.clone(),
                includes: vec![],
                excludes: vec![],
                ..Default::default()
})
            .unwrap();

        let examined = report.units + report.excluded.len();
        assert!(examined > 100, "this repo has real history; examined {examined}");
        // The filter must do substantial work — dropping merges and low-signal
        // commits — but NOT a strict majority: as the repo accrues commits with
        // real explanatory bodies (exactly what the filter keeps), the kept share
        // grows past 50% while the filter is still correct. Assert a durable floor
        // (at least a third filtered) instead of an exact ratio that drifts.
        assert!(
            report.excluded.len() * 3 >= examined,
            "the filter must remove a substantial share: {} kept, {} filtered ({examined} examined)",
            report.units,
            report.excluded.len()
        );

        for item in &report.items {
            assert!(!item.source_identity.contains(&root));
            let cand = c.fallback(item).unwrap();
            assert!(cand.source_excerpt.is_some());
            assert!(!cand.destination_hint["occurred_at"].as_str().unwrap_or("").is_empty());
        }

        // Calibration: the epic's own commits explain decisions at length.
        assert!(
            report
                .items
                .iter()
                .any(|i| i.meta["subject"].as_str().unwrap_or("").contains("knowledge-migration")),
            "a filter that discards the knowledge-migration commits is mis-tuned"
        );
    }
}
