//! The repo-docs connector: a repository's own Markdown, turned into candidates.
//!
//! The best-written knowledge a team has usually lives in Markdown and is
//! invisible to its agents. This repository is its own test case: 161 `.md`
//! files holding ADRs, engineering principles an agent should respect and does
//! not know about, roadmaps with unfiled work, and complete SDD specs.
//!
//! # The unit is the section, not the file
//!
//! `docs/ENGINEERING_PROCESS.md` holds, in one file, principles that are team
//! conventions and stack tables that are architectural context. At file
//! granularity you must choose: either the conventions are lost or the corpus
//! fills with tables. At section granularity each half goes where it belongs.
//!
//! The cost is a longer review queue — one document yields N candidates. That is
//! paid for by ordering the queue by confidence, which the review UI already
//! does.
//!
//! # Nothing here decides anything
//!
//! Every rule below proposes. The classifier may override the proposal and a
//! human always may. `reads_like_a_rule` in particular is a heuristic and is
//! documented as one: it orders the queue, it does not close it.

use anyhow::Result;
use sha2::{Digest, Sha256};

use super::{CandidatePayload, Connector, ScanOptions, SourceItem};
use crate::indexer::{
    chunker::{Chunker, MarkdownChunker},
    doc_walker::{walk_docs, DocWalkOptions},
    walker::read_file,
};

/// Paths excluded by default, and why.
///
/// Each one is documentation by extension and not engineering knowledge by
/// nature. They are **counted and reported**, never silently dropped: a run that
/// says "scanned 40 documents" when the tree held 161 is a run that lies.
const DEFAULT_EXCLUDED_PATHS: &[(&str, &str)] = &[
    ("/docs/marketing/", "marketing material is not engineering knowledge"),
    ("/docs/research/", "research material is not engineering knowledge"),
    (
        "/openspec/specs/",
        "the living specification is maintained by the archive flow, not imported",
    ),
    (
        "/openspec/changes/archive/",
        "closed changes already live in the SDD artifact store",
    ),
];

/// Words that mark a section as stating a rule rather than describing one.
///
/// Deliberately bilingual: this repository's own documentation mixes Spanish and
/// English, and a connector that only recognized one would miss half of it.
const RULE_MARKERS: &[&str] = &[
    "siempre", "nunca", "debe ", "deben ", "no se debe", "obligatorio", "prohibido",
    "always", "never", "must ", "must not", "should always", "required", "forbidden",
    "convention", "convención", "principio", "principle", "regla", "rule",
];

pub struct RepoDocsConnector {
    /// Repository name — the root directory's own name, never an absolute path.
    /// A source identity that carried `/Users/someone/` would leak the
    /// operator's home directory into a shared corpus.
    pub repo: String,
    /// Whether `openspec/changes/**` may produce `sdd_artifact` candidates.
    ///
    /// Off by default: in this repository `bin/import_sdd` already backfilled
    /// them, and two paths to one destination is how duplicates happen. On for
    /// somebody migrating a foreign repo where that importer never ran.
    pub include_sdd: bool,
}

/// One scanned document that produced no units, and the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExcludedDocument {
    pub path: String,
    pub reason: String,
}

/// What a scan found, including what it deliberately left out.
#[derive(Debug, Clone, Default)]
pub struct ScanSummary {
    pub documents_scanned: usize,
    pub units: usize,
    pub excluded: Vec<ExcludedDocument>,
}

impl ScanSummary {
    /// Rough token estimate for a full classification pass. Four bytes per token
    /// is the usual English approximation and is close enough to decide whether
    /// to spend; `--dry-run` reports it before anything is spent.
    pub fn estimated_tokens(&self, bytes: usize) -> usize {
        bytes / 4
    }
}

impl RepoDocsConnector {
    pub fn new(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            include_sdd: false,
        }
    }

    pub fn with_sdd(mut self, include: bool) -> Self {
        self.include_sdd = include;
        self
    }

    /// The repository name for a root path — its last component, or `repo` when
    /// the path has none (`.` and `/` both land here).
    pub fn repo_name_for(root: &str) -> String {
        std::path::Path::new(root)
            .canonicalize()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_else(|| "repo".to_string())
    }

    fn excluded_reason(path: &str) -> Option<&'static str> {
        // Normalize so a relative path matches the same rules an absolute one does.
        let probe = format!("/{}", path.trim_start_matches('/'));
        DEFAULT_EXCLUDED_PATHS
            .iter()
            .find(|(fragment, _)| probe.contains(fragment) || path.contains(fragment))
            .map(|(_, reason)| *reason)
    }

    /// Scan, and report what was left out alongside what was found.
    pub fn scan_with_summary(&self, opts: &ScanOptions) -> Result<(Vec<SourceItem>, ScanSummary)> {
        let files = walk_docs(
            &opts.root,
            &DocWalkOptions {
                extra_excludes: opts.excludes.clone(),
                includes: opts.includes.clone(),
                include_default_excluded: false,
            },
        )?;

        let chunker = MarkdownChunker::default();
        let mut items = Vec::new();
        let mut summary = ScanSummary::default();

        for file in &files {
            let rel = relative_path(&file.path, &opts.root);

            if let Some(reason) = Self::excluded_reason(&rel) {
                summary.excluded.push(ExcludedDocument {
                    path: rel,
                    reason: reason.to_string(),
                });
                continue;
            }

            let Some((content, content_sha)) = read_file(&file.path) else {
                continue;
            };
            summary.documents_scanned += 1;

            for chunk in chunker.chunk(&rel, &content_sha, Some("markdown"), &content) {
                let heading = chunk.symbol.clone().unwrap_or_default();
                let anchor = anchor_for(&heading, chunk.start_line);
                let origin = if heading.is_empty() {
                    rel.clone()
                } else {
                    format!("{rel} › {heading}")
                };

                // A checklist section is N units, not one.
                //
                // Collapsing twelve unchecked boxes into a single task candidate
                // titled after the first one silently loses eleven pieces of
                // work — and a reviewer approving it would believe the roadmap
                // had been captured. One unit per item, each with its own
                // identity, so each is decided on its own.
                let pending = unchecked_tasks(&chunk.content);
                if !pending.is_empty() && !is_under_adr(&rel) && !rel.contains("openspec/changes/")
                {
                    for (idx, task) in pending.iter().enumerate() {
                        items.push(SourceItem {
                            source_identity: self.identity(
                                &rel,
                                &format!("{anchor}-task{idx}"),
                                task,
                            ),
                            display_origin: format!("{origin} › task {}", idx + 1),
                            raw: task.clone(),
                            meta: serde_json::json!({
                                "path": rel,
                                "heading": heading,
                                "anchor": anchor,
                                "start_line": chunk.start_line,
                                "task_item": task,
                            }),
                        });
                    }
                    continue;
                }

                items.push(SourceItem {
                    source_identity: self.identity(&rel, &anchor, &chunk.content),
                    display_origin: origin,
                    raw: chunk.content.clone(),
                    meta: serde_json::json!({
                        "path": rel,
                        "heading": heading,
                        "anchor": anchor,
                        "start_line": chunk.start_line,
                    }),
                });
            }
        }

        summary.units = items.len();
        Ok((items, summary))
    }

    /// `repo-docs:{repo}:{path}#{anchor}:{sha16}`
    ///
    /// The hash is of the **section**, not the file: editing section 3 leaves
    /// sections 1 and 5 with the identities they already had, so a rescan after
    /// a small edit proposes one thing rather than forty.
    fn identity(&self, path: &str, anchor: &str, section: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(section.as_bytes());
        let sha = hex::encode(hasher.finalize());
        format!("repo-docs:{}:{}#{}:{}", self.repo, path, anchor, &sha[..16])
    }
}

/// Path relative to the scan root. An absolute path in a shared corpus carries
/// the operator's home directory to everyone who can read it.
fn relative_path(path: &str, root: &str) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .trim_start_matches('/')
        .to_string()
}

/// Same shape `doc_queries::slugify` produces, so a candidate's anchor and its
/// indexed chunk's anchor agree and a reviewer can jump between them.
fn anchor_for(heading: &str, start_line: i64) -> String {
    let base: String = heading
        .chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let trimmed = base.trim_matches('-').replace("--", "-");
    if trimmed.is_empty() {
        format!("l{start_line}")
    } else {
        format!("{trimmed}-l{start_line}")
    }
}

// ── Mapping rules ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    pub destination_kind: &'static str,
    pub memory_type: Option<&'static str>,
    /// The task text, when the section is a checklist.
    pub task_title: Option<String>,
}

impl Proposal {
    fn memory(kind: &'static str) -> Self {
        Self {
            destination_kind: "memory",
            memory_type: Some(kind),
            task_title: None,
        }
    }
    fn convention() -> Self {
        Self {
            destination_kind: "convention",
            memory_type: None,
            task_title: None,
        }
    }
    fn task(title: String) -> Self {
        Self {
            destination_kind: "task",
            memory_type: None,
            task_title: Some(title),
        }
    }
    fn sdd() -> Self {
        Self {
            destination_kind: "sdd_artifact",
            memory_type: None,
            task_title: None,
        }
    }
}

fn is_under_adr(path: &str) -> bool {
    path.contains("/adr/") || path.starts_with("adr/")
}

/// Unchecked task-list items, in order.
///
/// A checked box is work already done. Proposing it as a task creates phantom
/// work somebody then has to close by hand, so only `- [ ]` counts.
pub fn unchecked_tasks(section: &str) -> Vec<String> {
    section
        .lines()
        .filter_map(|line| {
            let t = line.trim_start();
            for marker in ["- [ ] ", "* [ ] ", "- [] ", "* [] "] {
                if let Some(rest) = t.strip_prefix(marker) {
                    let text = rest.trim();
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
            None
        })
        .collect()
}

/// Heuristic: does this section *state* a rule, rather than describe one?
///
/// It looks for imperatives and absolutes in the heading and the opening lines.
/// It is right about `ENGINEERING_PROCESS.md` and wrong about prose that
/// describes a rule without stating it. That is acceptable precisely because it
/// only orders the review queue — the classifier may overrule it and the human
/// decides.
pub fn reads_like_a_rule(heading: &str, section: &str) -> bool {
    let head = heading.to_lowercase();
    let opening: String = section.lines().take(8).collect::<Vec<_>>().join(" ").to_lowercase();
    RULE_MARKERS
        .iter()
        .any(|m| head.contains(m) || opening.contains(m))
}

pub fn propose_destination(path: &str, heading: &str, section: &str, include_sdd: bool) -> Proposal {
    if path.contains("openspec/changes/") {
        // Off by default: `bin/import_sdd` already backfilled these here, and two
        // paths to one destination is how duplicates happen.
        if include_sdd {
            return Proposal::sdd();
        }
        return Proposal::memory("architecture");
    }
    if is_under_adr(path) {
        return Proposal::memory("decision");
    }
    let tasks = unchecked_tasks(section);
    if !tasks.is_empty() {
        return Proposal::task(tasks[0].clone());
    }
    if reads_like_a_rule(heading, section) {
        return Proposal::convention();
    }
    Proposal::memory("architecture")
}

/// The first N lines of a section, verbatim. Never a paraphrase: the excerpt is
/// the reviewer's only way to judge the proposal without opening the file.
fn excerpt(section: &str) -> String {
    section
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
}

fn title_for(heading: &str, section: &str) -> String {
    if !heading.trim().is_empty() {
        return heading.trim().trim_start_matches('#').trim().to_string();
    }
    section
        .lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("Untitled section")
        .trim()
        .trim_start_matches('#')
        .trim()
        .chars()
        .take(120)
        .collect()
}

impl Connector for RepoDocsConnector {
    fn source_kind(&self) -> &'static str {
        "repo-docs"
    }

    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.scan_with_summary(opts)?.0)
    }

    fn scan_report(&self, opts: &ScanOptions) -> Result<super::ScanReport> {
        let (items, summary) = self.scan_with_summary(opts)?;
        Ok(super::ScanReport {
            documents: summary.documents_scanned,
            units: items.len(),
            bytes: items.iter().map(|i| i.raw.len()).sum(),
            excluded: summary
                .excluded
                .into_iter()
                .map(|e| (e.path, e.reason))
                .collect(),
            items,
        })
    }

    fn classify_prompt(&self, item: &SourceItem) -> String {
        let path = item.meta.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let heading = item.meta.get("heading").and_then(|v| v.as_str()).unwrap_or("");
        format!(
            "You are classifying one section of a software team's documentation so it can be \
             proposed — never committed — as team knowledge. A human reviews everything you \
             return.\n\n\
             Document: {path}\n\
             Section: {heading}\n\n\
             ---\n{content}\n---\n\n\
             Return ONE JSON object and nothing else:\n\
             {{\"source_identity\": \"\", \"destination_kind\": \"memory|convention|task|skip\", \
             \"content\": \"...\", \"source_excerpt\": \"...\", \"confidence\": 0.0, \
             \"destination_hint\": {{\"title\": \"...\", \"type\": \"...\"}}}}\n\n\
             Rules:\n\
             1. PROPOSE, do not decide. A human will accept or reject this.\n\
             2. `source_excerpt` MUST be copied verbatim from the section above. Never \
             paraphrase it — it is how the reviewer judges you without opening the file.\n\
             3. If the section carries no reusable team knowledge (a table of contents, a \
             changelog fragment, boilerplate), return \"skip\" as the destination_kind and say \
             why in `content`. Proposing everything turns review into a job nobody does.\n\
             4. `convention` is for a rule the team must follow. `memory` is for context, a \
             decision, or a discovery. `task` is for work that is still pending.",
            path = path,
            heading = heading,
            content = item.raw,
        )
    }

    /// Always `Some`: no section is lost for want of a classifier. This is what
    /// makes `--no-llm` usable, which is the mode a client whose NDA forbids
    /// sending material to a third party actually needs.
    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload> {
        let path = item.meta.get("path").and_then(|v| v.as_str()).unwrap_or("");
        let heading = item.meta.get("heading").and_then(|v| v.as_str()).unwrap_or("");

        // A unit carrying `task_item` is one checklist entry, already isolated by
        // `scan`. Its raw text is the task itself, so there is nothing to infer.
        if let Some(task) = item.meta.get("task_item").and_then(|v| v.as_str()) {
            return Some(CandidatePayload {
                source_identity: item.source_identity.clone(),
                destination_kind: "task".to_string(),
                content: task.to_string(),
                destination_hint: serde_json::json!({
                    "title": task,
                    "source_document": path,
                    "source_heading": heading,
                }),
                source_excerpt: Some(task.to_string()),
                confidence: None,
                provenance_kind: Some("verified_manifest".to_string()),
            });
        }

        let proposal = propose_destination(path, heading, &item.raw, self.include_sdd);

        let mut hint = serde_json::json!({ "title": title_for(heading, &item.raw) });
        if let Some(t) = proposal.memory_type {
            hint["type"] = serde_json::json!(t);
        }
        if let Some(task) = &proposal.task_title {
            hint["title"] = serde_json::json!(task);
        }
        if proposal.destination_kind == "sdd_artifact" {
            hint["change_name"] = serde_json::json!(change_name_from(path));
            hint["kind"] = serde_json::json!(sdd_kind_from(path));
            hint["path"] = serde_json::json!(path);
        }

        Some(CandidatePayload {
            source_identity: item.source_identity.clone(),
            destination_kind: proposal.destination_kind.to_string(),
            content: item.raw.clone(),
            destination_hint: hint,
            source_excerpt: Some(excerpt(&item.raw)),
            // No model, no score. Inventing one would be worse than none: the
            // review queue sorts by confidence and a fabricated number would
            // reorder it on nothing.
            confidence: None,
            provenance_kind: Some("verified_manifest".to_string()),
        })
    }
}

fn change_name_from(path: &str) -> String {
    path.split("openspec/changes/")
        .nth(1)
        .and_then(|rest| rest.split('/').next())
        .unwrap_or("migrated")
        .to_string()
}

fn sdd_kind_from(path: &str) -> String {
    let file = path.rsplit('/').next().unwrap_or("");
    match file.trim_end_matches(".md") {
        "proposal" | "design" | "tasks" | "exploration" | "spec" | "apply-progress"
        | "verify-report" | "archive-report" => file.trim_end_matches(".md").to_string(),
        _ => "proposal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const PROCESS_DOC: &str = "# Engineering Process\n\nIntro prose about the document.\n\n\
## Principles\n\n1. BYOM — never depend on an LLM provider.\n\n\
## Stack\n\n| Component | Tech |\n|---|---|\n| Backend | Rust |\n";

    fn tree() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("docs/adr")).unwrap();
        fs::create_dir_all(dir.path().join("docs/marketing")).unwrap();
        fs::create_dir_all(dir.path().join("docs/research")).unwrap();
        fs::create_dir_all(dir.path().join("openspec/specs/harness-library")).unwrap();
        fs::create_dir_all(dir.path().join("openspec/changes/some-change")).unwrap();
        fs::write(dir.path().join("docs/ENGINEERING_PROCESS.md"), PROCESS_DOC).unwrap();
        fs::write(
            dir.path().join("docs/adr/ADR-001.md"),
            "# ADR-001: Rust for the backend\n\nWe chose Rust over Go for deterministic latency.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("docs/ROADMAP.md"),
            "# Roadmap\n\n## Next\n\n- [ ] Wire the doc index into search\n- [x] Ship run_v60\n",
        )
        .unwrap();
        fs::write(dir.path().join("docs/marketing/pitch.md"), "# Pitch\n\nBuy it.\n").unwrap();
        fs::write(dir.path().join("docs/research/notes.md"), "# Notes\n\nRead this.\n").unwrap();
        fs::write(
            dir.path().join("openspec/specs/harness-library/spec.md"),
            "# Harness Library\n\nThe living spec.\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("openspec/changes/some-change/proposal.md"),
            "# Proposal\n\nSomething is proposed.\n",
        )
        .unwrap();
        dir
    }

    fn opts(dir: &TempDir) -> ScanOptions {
        ScanOptions {
            root: dir.path().to_string_lossy().to_string(),
            includes: vec![],
            excludes: vec![],
        }
    }

    fn connector() -> RepoDocsConnector {
        RepoDocsConnector::new("nexusmind")
    }

    // ── T-03: scanning ───────────────────────────────────────────────────────

    #[test]
    fn scan_splits_a_document_into_sections() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let (items, summary) = connector().scan_with_summary(&opts(&dir)).unwrap();

        assert_eq!(summary.documents_scanned, 1);
        assert!(items.len() >= 3, "one unit per section; got {}", items.len());
        let headings: Vec<String> = items
            .iter()
            .map(|i| i.meta["heading"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(headings.iter().any(|h| h.contains("Principles")));
        assert!(headings.iter().any(|h| h.contains("Stack")));
    }

    #[test]
    fn a_document_without_headings_yields_one_unit() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("plain.md"), "Just prose.\nMore prose.\n").unwrap();
        let (items, _) = connector().scan_with_summary(&opts(&dir)).unwrap();
        assert_eq!(items.len(), 1);
    }

    // ── T-04: identity ───────────────────────────────────────────────────────

    #[test]
    fn identity_is_stable_across_rescans() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let first = connector().scan(&opts(&dir)).unwrap();
        let second = connector().scan(&opts(&dir)).unwrap();
        let ids = |v: &Vec<SourceItem>| -> Vec<String> {
            v.iter().map(|i| i.source_identity.clone()).collect()
        };
        assert_eq!(ids(&first), ids(&second));
    }

    /// The whole point of hashing the section rather than the file: a small edit
    /// must propose one thing, not the whole document again.
    #[test]
    fn editing_one_section_changes_only_its_identity() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let before = connector().scan(&opts(&dir)).unwrap();

        let edited = PROCESS_DOC.replace("| Backend | Rust |", "| Backend | Rust 2021 |");
        fs::write(dir.path().join("a.md"), &edited).unwrap();
        let after = connector().scan(&opts(&dir)).unwrap();

        assert_eq!(before.len(), after.len());
        let changed = before
            .iter()
            .zip(after.iter())
            .filter(|(b, a)| b.source_identity != a.source_identity)
            .count();
        assert_eq!(changed, 1, "only the edited section's identity may change");
    }

    #[test]
    fn identity_never_contains_an_absolute_path() {
        let dir = tree();
        let items = connector().scan(&opts(&dir)).unwrap();
        assert!(!items.is_empty());
        let root = dir.path().to_string_lossy().to_string();
        for item in &items {
            assert!(
                !item.source_identity.contains(&root) && !item.source_identity.contains("/Users/"),
                "identity leaks an absolute path: {}",
                item.source_identity
            );
            assert!(!item.display_origin.contains(&root));
        }
    }

    #[test]
    fn moving_a_document_changes_its_identities() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let at_root = connector().scan(&opts(&dir)).unwrap();

        fs::remove_file(dir.path().join("a.md")).unwrap();
        fs::create_dir_all(dir.path().join("sub")).unwrap();
        fs::write(dir.path().join("sub/a.md"), PROCESS_DOC).unwrap();
        let nested = connector().scan(&opts(&dir)).unwrap();

        assert_ne!(at_root[0].source_identity, nested[0].source_identity);
    }

    // ── T-05: mapping rules ──────────────────────────────────────────────────

    #[test]
    fn adr_path_proposes_a_decision_memory() {
        let p = propose_destination("docs/adr/ADR-001.md", "ADR-001", "We chose Rust.", false);
        assert_eq!(p.destination_kind, "memory");
        assert_eq!(p.memory_type, Some("decision"));
    }

    #[test]
    fn unchecked_checklist_item_proposes_a_task() {
        let section = "## Next\n\n- [ ] Wire the doc index into search\n- [x] Ship run_v60\n";
        let p = propose_destination("docs/ROADMAP.md", "Next", section, false);
        assert_eq!(p.destination_kind, "task");
        assert_eq!(
            p.task_title.as_deref(),
            Some("Wire the doc index into search")
        );
    }

    /// A checked box is work already done. Proposing it creates phantom work.
    #[test]
    fn checked_items_propose_no_task() {
        let section = "## Done\n\n- [x] Ship run_v60\n- [x] Ship the review UI\n";
        assert!(unchecked_tasks(section).is_empty());
        let p = propose_destination("docs/ROADMAP.md", "Done", section, false);
        assert_ne!(p.destination_kind, "task");
    }

    #[test]
    fn rule_shaped_section_proposes_a_convention() {
        let section = "## Principles\n\n1. BYOM — never depend on an LLM provider.\n";
        let p = propose_destination("docs/ENGINEERING_PROCESS.md", "Principles", section, false);
        assert_eq!(p.destination_kind, "convention");
    }

    #[test]
    fn plain_prose_falls_back_to_an_architecture_memory() {
        let section = "## Overview\n\nThe service receives events and writes them down.\n";
        let p = propose_destination("docs/ARCHITECTURE.md", "Overview", section, false);
        assert_eq!(p.destination_kind, "memory");
        assert_eq!(p.memory_type, Some("architecture"));
    }

    /// `import_sdd` already backfilled these in this repository; two paths to one
    /// destination is how duplicates happen. The flag exists for foreign repos.
    #[test]
    fn openspec_change_proposes_an_sdd_artifact_only_with_the_flag() {
        let path = "openspec/changes/some-change/proposal.md";
        let without = propose_destination(path, "Proposal", "Something.", false);
        assert_eq!(without.destination_kind, "memory");

        let with = propose_destination(path, "Proposal", "Something.", true);
        assert_eq!(with.destination_kind, "sdd_artifact");
    }

    // ── T-06: exclusions ─────────────────────────────────────────────────────

    #[test]
    fn default_excludes_skip_marketing_research_and_living_specs() {
        let dir = tree();
        let (items, summary) = connector().scan_with_summary(&opts(&dir)).unwrap();
        let paths: Vec<String> = items
            .iter()
            .map(|i| i.meta["path"].as_str().unwrap_or("").to_string())
            .collect();

        for forbidden in ["docs/marketing", "docs/research", "openspec/specs"] {
            assert!(
                !paths.iter().any(|p| p.contains(forbidden)),
                "{forbidden} must not produce units; got {paths:?}"
            );
        }
        assert!(summary.documents_scanned >= 3, "the rest is still scanned");
    }

    /// A run that says "scanned 40" when the tree held 161 is a run that lies.
    #[test]
    fn excluded_documents_are_reported_not_omitted() {
        let dir = tree();
        let (_, summary) = connector().scan_with_summary(&opts(&dir)).unwrap();
        assert_eq!(summary.excluded.len(), 3, "marketing, research, living spec");
        for ex in &summary.excluded {
            assert!(!ex.reason.is_empty(), "every exclusion carries its reason");
        }
        assert!(summary
            .excluded
            .iter()
            .any(|e| e.reason.contains("living specification")));
    }

    // ── T-07/T-08: prompt and fallback ───────────────────────────────────────

    #[test]
    fn prompt_includes_the_section_its_path_and_asks_for_a_verbatim_excerpt() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let items = connector().scan(&opts(&dir)).unwrap();
        let prompt = connector().classify_prompt(&items[0]);

        assert!(prompt.contains("a.md"));
        assert!(prompt.contains(&items[0].raw));
        assert!(prompt.contains("verbatim"));
        assert!(prompt.contains("PROPOSE, do not decide"));
        assert!(prompt.contains("skip"), "the model must be able to decline");
    }

    #[test]
    fn fallback_produces_a_candidate_for_every_unit() {
        let dir = tree();
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        assert!(!items.is_empty());
        for item in &items {
            assert!(
                c.fallback(item).is_some(),
                "no unit may be dropped for want of a classifier: {}",
                item.display_origin
            );
        }
    }

    #[test]
    fn every_candidate_carries_a_verbatim_excerpt() {
        let dir = tree();
        let c = connector();
        for item in c.scan(&opts(&dir)).unwrap() {
            let candidate = c.fallback(&item).unwrap();
            let ex = candidate.source_excerpt.expect("excerpt is mandatory");
            for line in ex.lines() {
                assert!(
                    item.raw.contains(line),
                    "the excerpt must be copied from the source, not written: {line:?}"
                );
            }
        }
    }

    #[test]
    fn fallback_reports_no_confidence() {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("a.md"), PROCESS_DOC).unwrap();
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        assert!(
            c.fallback(&items[0]).unwrap().confidence.is_none(),
            "without a model there is no score, and inventing one would reorder the queue on nothing"
        );
    }

    // ── T-11: against this repository ────────────────────────────────────────

    /// Runs the connector over this checkout's own `docs/`. It asserts
    /// properties rather than counts — the corpus changes — but it is what turns
    /// "should work" into "works over the real tree".
    #[test]
    fn scanning_this_repository_produces_plausible_candidates() {
        // apps/backend/src/migration → repo root
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("repo root")
            .to_string_lossy()
            .to_string();

        let docs = format!("{root}/docs");
        if !std::path::Path::new(&docs).is_dir() {
            // Running from a packaged crate without the docs tree — nothing to assert.
            return;
        }

        let c = RepoDocsConnector::new(RepoDocsConnector::repo_name_for(&root));
        let (items, summary) = c
            .scan_with_summary(&ScanOptions {
                root: root.clone(),
                includes: vec!["/docs/".to_string()],
                excludes: vec![],
            })
            .unwrap();

        assert!(
            summary.documents_scanned >= 10,
            "this repo's docs/ tree should yield real documents; got {}",
            summary.documents_scanned
        );
        assert!(items.len() > summary.documents_scanned, "sections outnumber files");

        // Nothing leaks an absolute path into a shared corpus.
        for item in &items {
            assert!(!item.source_identity.contains(&root));
            assert!(
                c.fallback(item).unwrap().source_excerpt.is_some(),
                "every candidate carries an excerpt"
            );
        }

        // The case the heuristic exists for: the engineering principles are rules.
        let conventions_from_process: Vec<&SourceItem> = items
            .iter()
            .filter(|i| {
                i.meta["path"]
                    .as_str()
                    .unwrap_or("")
                    .ends_with("ENGINEERING_PROCESS.md")
                    && c.fallback(i).unwrap().destination_kind == "convention"
            })
            .collect();
        assert!(
            !conventions_from_process.is_empty(),
            "ENGINEERING_PROCESS.md states team rules and must yield at least one convention"
        );
    }

    // ── T-10: the dry run must report what it left out ───────────────────────

    /// The spec asks for documents, units and an estimate — not just units. An
    /// operator deciding whether to spend needs to know the tree was 161 files
    /// and 3 were skipped, not only that there are N sections.
    #[test]
    fn scan_report_counts_documents_units_and_exclusions() {
        let dir = tree();
        let report = connector().scan_report(&opts(&dir)).unwrap();

        assert!(report.documents >= 3, "documents are counted, not just units");
        assert_eq!(report.units, report.items.len());
        assert!(report.bytes > 0);
        assert!(report.estimated_tokens() > 0);
        assert_eq!(
            report.excluded.len(),
            3,
            "marketing, research and the living spec are reported, not omitted"
        );
        for (path, reason) in &report.excluded {
            assert!(!path.is_empty() && !reason.is_empty());
        }
    }

    #[test]
    fn scan_report_and_scan_agree() {
        let dir = tree();
        let c = connector();
        let plain = c.scan(&opts(&dir)).unwrap();
        let report = c.scan_report(&opts(&dir)).unwrap();
        assert_eq!(plain.len(), report.items.len());
    }

    // ── Hallazgo de la revisión adversarial ──────────────────────────────────

    /// A roadmap section with twelve unchecked boxes used to collapse into ONE
    /// task candidate titled after the first — silently losing eleven pieces of
    /// work, while a reviewer approving it would believe the roadmap had been
    /// captured. Each item is its own unit now.
    #[test]
    fn each_unchecked_item_becomes_its_own_task_unit() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("ROADMAP.md"),
            "# Roadmap\n\n## Next\n\n- [ ] Wire the doc index\n- [ ] Ship the connectors\n\
             - [x] Ship run_v60\n- [ ] Answer the NDA question\n",
        )
        .unwrap();

        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        let tasks: Vec<&SourceItem> = items
            .iter()
            .filter(|i| i.meta.get("task_item").is_some())
            .collect();

        assert_eq!(tasks.len(), 3, "three unchecked boxes, three units — not one");

        let titles: Vec<String> = tasks
            .iter()
            .map(|t| c.fallback(t).unwrap().destination_hint["title"].as_str().unwrap().to_string())
            .collect();
        assert!(titles.contains(&"Wire the doc index".to_string()));
        assert!(titles.contains(&"Ship the connectors".to_string()));
        assert!(titles.contains(&"Answer the NDA question".to_string()));
        assert!(
            !titles.iter().any(|t| t.contains("run_v60")),
            "the checked box is work already done and must not become a task"
        );

        for t in &tasks {
            let cand = c.fallback(t).unwrap();
            assert_eq!(cand.destination_kind, "task");
            assert_eq!(
                cand.source_excerpt.as_deref(),
                Some(t.raw.as_str()),
                "the excerpt is the item itself, verbatim"
            );
        }
    }

    #[test]
    fn task_units_have_distinct_identities() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join("ROADMAP.md"),
            "## Next\n\n- [ ] First thing\n- [ ] Second thing\n",
        )
        .unwrap();
        let items = connector().scan(&opts(&dir)).unwrap();
        let ids: Vec<&str> = items.iter().map(|i| i.source_identity.as_str()).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1], "two tasks must not share one identity");
    }

    /// A checklist inside an ADR is a decision's own to-do list, not the team's
    /// backlog. ADRs and openspec changes keep their section-level treatment.
    #[test]
    fn checklists_inside_adrs_do_not_become_tasks() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join("docs/adr")).unwrap();
        fs::write(
            dir.path().join("docs/adr/ADR-002.md"),
            "# ADR-002\n\nWe decided X.\n\n- [ ] follow-up inside the decision\n",
        )
        .unwrap();
        let c = connector();
        let items = c.scan(&opts(&dir)).unwrap();
        assert!(items.iter().all(|i| i.meta.get("task_item").is_none()));
        assert_eq!(c.fallback(&items[0]).unwrap().destination_kind, "memory");
    }
}
