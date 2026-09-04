//! Knowledge-migration connectors: the local half of the pipeline.
//!
//! The contract lives in the library rather than in `bin/migrate_knowledge.rs`
//! for two reasons that only show up once there is more than one connector:
//!
//! 1. A connector inside the binary is only reachable by `cargo test --bin`,
//!    which does not see the rest of the suite or its fixtures.
//! 2. Three more connectors are coming (`git-history`, `claude-memories`,
//!    `db-schema`). This is where they go, and creating the module with the
//!    first one avoids moving all of them later.
//!
//! The binary re-exports everything here, so it reads exactly as before.

pub mod claude_memories;
pub mod events;
pub mod db_schema;
pub mod git_history;
pub mod pg_reader;
pub mod redact;
pub mod repo_docs;
pub mod source_code;

pub use claude_memories::ClaudeMemoriesConnector;
pub use git_history::GitHistoryConnector;
pub use repo_docs::RepoDocsConnector;
pub use source_code::SourceCodeConnector;

use anyhow::Result;
use std::fmt;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

// ── The connector contract ───────────────────────────────────────────────────

/// One unit of source material, addressed by an identity its connector computes.
#[derive(Debug, Clone, PartialEq)]
pub struct SourceItem {
    /// Deterministic and provenance-derived, including a hash of the content.
    /// Editing the source changes the identity, which is what makes an edited
    /// document a *new* candidate rather than a silent overwrite.
    pub source_identity: String,
    /// Human-readable origin, shown to the reviewer. Never an absolute path.
    pub display_origin: String,
    /// Repository-relative path used for project routing. `None` means the
    /// source has no meaningful repository path and therefore needs a config
    /// default or explicit destination.
    pub routing_path: Option<String>,
    pub raw: String,
    pub meta: serde_json::Value,
}

/// A candidate as posted to the backend. Mirrors `models::types::CandidateInput`
/// on the wire; kept as its own type so the runner does not depend on the
/// backend's internal shapes beyond the JSON contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CandidatePayload {
    /// Always supplied by the pipeline, never read from the model.
    ///
    /// Every path overwrites this with the `SourceItem`'s own identity before
    /// the candidate is used, because provenance is the connector's to decide.
    /// It defaults on the way in for that reason: requiring the model to echo a
    /// hash it must not choose rejected otherwise-valid answers outright —
    /// "classifier output is not a valid candidate: missing field
    /// `source_identity`" was a real and frequent failure — and every echoed
    /// copy was output tokens spent to be discarded.
    #[serde(default)]
    pub source_identity: String,
    /// Where this candidate lands. Defaults on the way in and is filled from the
    /// deterministic draft when the model omits it.
    ///
    /// Measured against the real corpus: asked for distilled knowledge, the
    /// model returns `item` and `content` and leaves the routing to the
    /// pipeline — which is the right instinct, since the draft already proposed
    /// a destination from the document's own shape. Requiring it turned seven
    /// good answers into seven deserialization failures, and each failure fell
    /// back to the draft, whose content is the raw section text. The strictness
    /// produced exactly the outcome it was meant to prevent.
    #[serde(default)]
    pub destination_kind: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub destination_hint: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_kind: Option<String>,
}

/// A source of pre-NexusMind knowledge.
pub trait Connector {
    fn source_kind(&self) -> &'static str;

    /// Enumerate source units. Must not call a language model: scanning has to
    /// work under `--dry-run` and under `--no-llm`.
    fn scan(&self, opts: &ScanOptions) -> Result<Vec<SourceItem>>;

    /// The classification prompt for one unit. The connector knows its own
    /// domain; the pipeline only knows how to run a prompt.
    fn classify_prompt(&self, item: &SourceItem) -> String;

    /// Scan, and report what was deliberately left out alongside what was found.
    ///
    /// The default implementation reports no exclusions, which is honest for a
    /// connector that excludes nothing. A connector that DOES exclude must
    /// override this: a run that says "scanned 40 documents" when the tree held
    /// 161 is a run that lies, and `--dry-run` is where an operator decides
    /// whether to spend money.
    fn scan_report(&self, opts: &ScanOptions) -> Result<ScanReport> {
        let items = self.scan(opts)?;
        Ok(ScanReport {
            documents: 0,
            units: items.len(),
            bytes: items.iter().map(|i| i.raw.len()).sum(),
            excluded: Vec::new(),
            items,
        })
    }

    /// A deterministic candidate derived without any model, or `None` when the
    /// unit genuinely needs judgement.
    ///
    /// This is not a nicety. A connector that only works with an LLM stops
    /// working when the CLI changes its output format, when there is no network,
    /// and — the case that actually matters — when a client's NDA forbids
    /// sending their material to a third party for processing.
    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload>;
}

/// What a scan found and what it left behind.
#[derive(Debug, Clone, Default)]
pub struct ScanReport {
    /// Documents actually read. Zero means the connector does not track it.
    pub documents: usize,
    pub units: usize,
    pub bytes: usize,
    /// `(path, reason)` for everything skipped on purpose.
    pub excluded: Vec<(String, String)>,
    pub items: Vec<SourceItem>,
}

impl ScanReport {
    /// Four bytes per token is the usual approximation — close enough to decide
    /// whether to spend, which is all a dry run has to support.
    pub fn estimated_tokens(&self) -> usize {
        self.bytes / 4
    }
}

/// Where a scan currently is.
///
/// `seen` counts sources examined so far, not units produced — a scan does not
/// know how many units it will yield until it is done.
#[derive(Debug, Clone)]
pub struct ScanProgress {
    pub seen: usize,
    pub current: String,
}

#[derive(Clone, Default)]
pub struct ScanOptions {
    pub root: String,
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    /// Called as the walk advances, if anyone is listening.
    ///
    /// # Why this exists
    ///
    /// A scan is one blocking call that can take minutes over a large
    /// repository, and between entering it and returning there was no way for a
    /// caller to know anything at all. A supervising process could not tell a
    /// slow scan from a hung one — and neither could the operator watching it,
    /// which is the whole failure this hook removes.
    ///
    /// It is on `ScanOptions` rather than on the `Connector` trait so that a
    /// connector which has nothing useful to report simply never calls it, and
    /// no signature changes.
    pub progress: Option<Arc<dyn Fn(ScanProgress) + Send + Sync>>,
}

impl fmt::Debug for ScanOptions {
    /// Hand-written because a closure has no `Debug`. Reports whether anyone is
    /// listening, which is the only thing about it worth printing.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ScanOptions")
            .field("root", &self.root)
            .field("includes", &self.includes)
            .field("excludes", &self.excludes)
            .field("progress", &self.progress.is_some())
            .finish()
    }
}

impl ScanOptions {
    /// Reports one examined source. Costs an `Option` check when unobserved.
    pub fn note(&self, seen: usize, current: impl Into<String>) {
        if let Some(sink) = self.progress.as_ref() {
            sink(ScanProgress {
                seen,
                current: current.into(),
            });
        }
    }
}
