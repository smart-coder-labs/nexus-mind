//! `migrate-knowledge` — the local half of the knowledge-migration pipeline.
//!
//! # Why this runs here and not in the backend
//!
//! The two halves of a migration have never been on the same machine and never
//! will be. The material lives on a developer's laptop or in a client's network:
//! the repo, `~/.claude/`, a Postgres they can reach. The truth lives in the
//! backend container, which has none of those — and, by the BYOM principle
//! (`docs/ENGINEERING_PROCESS.md:14`), no model credentials either. So the scan
//! and the inference happen here, and only typed candidates cross the wire.
//!
//! `bin/import_sdd.rs` reached the same conclusion for the same reason; this is
//! that pattern applied to a second problem.
//!
//! # What this binary contains, and what it deliberately does not
//!
//! It contains the pipeline: the [`Connector`] contract, the `claude -p`
//! adapter, budgeting, dry-run, and the HTTP push. It contains exactly one
//! connector — [`NoopConnector`] — which reads nothing and calls nothing, so CI
//! can exercise the whole path without a filesystem, a database, or Claude Code
//! installed on the runner.
//!
//! The four real connectors are separate changes (`knowledge-migration-repo-docs`
//! and siblings). Landing one here would make this PR unreviewable and would
//! couple the pipeline to the first source that happened to be written.

use anyhow::{Context, Result};
use clap::Parser;
use nexusmind::repository_config::{self, ConfigSelection, DestinationOverride, ProjectResolver};

#[derive(Clone)]
struct ExecutionGroup {
    alias: String,
    project_id: String,
    client_id: Option<String>,
    item_indices: Vec<usize>,
    attestation: serde_json::Value,
}
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

// ── The connector contract ───────────────────────────────────────────────────
//
// Moved to `nexusmind::migration` so connectors can be tested with the rest of
// the library suite. Re-exported here so this binary reads as it did before.

pub use nexusmind::migration::{
    db_schema::{DbSchemaConnector, SamplingPolicy},
    events::{clip, EventSink, RunEvent},
    pg_reader::PgSchemaReader,
    CandidatePayload, ClaudeMemoriesConnector, Connector, GitHistoryConnector, RepoDocsConnector,
    ScanOptions, ScanProgress, SourceCodeConnector, SourceItem,
};

// ── The noop connector ───────────────────────────────────────────────────────

/// Fixed items, no I/O, no model. Exists so the pipeline itself is testable.
pub struct NoopConnector {
    pub items: Vec<SourceItem>,
}

impl NoopConnector {
    pub fn with_sample() -> Self {
        Self {
            items: vec![SourceItem {
                source_identity: "noop:sample:1".to_string(),
                display_origin: "noop sample".to_string(),
                routing_path: None,
                raw: "The team always writes the failing test first.".to_string(),
                meta: serde_json::json!({}),
            }],
        }
    }
}

impl Connector for NoopConnector {
    fn source_kind(&self) -> &'static str {
        "noop"
    }
    fn scan(&self, _opts: &ScanOptions) -> Result<Vec<SourceItem>> {
        Ok(self.items.clone())
    }
    fn classify_prompt(&self, item: &SourceItem) -> String {
        format!("Classify this: {}", item.raw)
    }
    fn fallback(&self, item: &SourceItem) -> Option<CandidatePayload> {
        Some(CandidatePayload {
            source_identity: item.source_identity.clone(),
            destination_kind: "memory".to_string(),
            content: item.raw.clone(),
            destination_hint: serde_json::json!({ "title": item.display_origin }),
            source_excerpt: Some(item.raw.clone()),
            confidence: None,
            provenance_kind: Some("verified_manifest".to_string()),
        })
    }
}

// ── The classifier adapter ───────────────────────────────────────────────────

/// Token usage reported by the CLI, when it can be read.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TokenUsage {
    pub input: i64,
    pub output: i64,
}

/// Why an item in a batch carries no candidate from the model.
///
/// The two cases must not share a counter, because they mean opposite things: a
/// sparse reply leaves most items `Passed` by design, and reporting those as
/// failures would put "112 of 120 item(s) unusable" on screen for a batch that
/// went perfectly.
#[derive(Debug, Clone, PartialEq)]
pub enum BulkMiss {
    /// The model did not put this item forward.
    ///
    /// This is a *decline*, not an approval. The deterministic draft's content
    /// is the section's raw text, so treating silence as assent would migrate
    /// the document verbatim, one chunk per heading — exactly the review queue
    /// full of unread prose that this pipeline exists to avoid. Nothing is
    /// staged for an item the model did not speak for.
    Passed,
    /// The model answered and the answer could not be used.
    Unusable(String),
}

impl std::fmt::Display for BulkMiss {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BulkMiss::Passed => f.write_str("not put forward"),
            BulkMiss::Unusable(why) => f.write_str(why),
        }
    }
}

/// The result of one batch call: one outcome per item, in order.
#[derive(Debug, Clone)]
pub struct BulkOutput {
    pub candidates: Vec<Result<CandidatePayload, BulkMiss>>,
    pub usage: Option<TokenUsage>,
    pub raw_response: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierOutput {
    /// What the model produced, or why it could not be used.
    ///
    /// Deliberately a `Result` *inside* a successful call rather than an outer
    /// `Err`: the tokens were spent either way, and an error that carries no
    /// usage is how a run burned an entire weekly quota while reporting
    /// "0 tokens spent" and a budget that could never trip.
    /// `Err` carries the reason as text so this type stays `Clone` and
    /// comparable, which its fixture tests rely on.
    pub candidate: Result<CandidatePayload, String>,
    /// `None` when the envelope did not carry usage in a shape we recognize.
    pub usage: Option<TokenUsage>,
    /// What the model actually wrote, kept whether or not it parsed. Without
    /// it, a run that falls back 249 times can only say *that* it failed.
    pub raw_response: String,
}

/// Pulls a JSON object out of whatever the model actually wrote.
///
/// A model asked for JSON commonly answers with a fenced block, or with a
/// sentence before it. Demanding a bare object made every classification in a
/// 249-file run fail and silently fall back — while still being billed for.
/// The transport's job is to get the object out; refusing on presentation is
/// not rigour, it is a 100% failure rate.
///
/// Scans for the first balanced `{...}`, respecting strings and escapes so a
/// brace inside a quoted value cannot end the object early.
pub fn extract_json_object(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let start = raw.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// The bulk equivalent of [`extract_json_object`].
///
/// Same reasoning: the transport's job is to get the payload out, not to grade
/// the model's presentation.
pub fn extract_json_array(raw: &str) -> Option<&str> {
    let bytes = raw.as_bytes();
    let start = raw.find('[')?;
    let (mut depth, mut in_string, mut escaped) = (0usize, false, false);
    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'[' => depth += 1,
            b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&raw[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Extracts the candidate from a `claude -p --output-format json` envelope.
///
/// Every assumption about the CLI's output shape lives in this function and its
/// sibling [`parse_usage`]. When a future CLI release changes the envelope, a
/// fixture test here fails — not the pipeline, and not silently.
pub fn parse_candidate(envelope: &serde_json::Value) -> Result<CandidatePayload> {
    // The CLI wraps the model's answer in `result` (a string) for text output and
    // may return the object directly when the prompt asked for JSON. Accept both
    // rather than making the caller care.
    let payload: serde_json::Value = match envelope.get("result") {
        Some(serde_json::Value::String(s)) => {
            let json = extract_json_object(s).unwrap_or(s.as_str());
            serde_json::from_str(json).with_context(|| {
                format!(
                    "the `result` field carried no parseable candidate JSON (started: {:.120})",
                    s.trim()
                )
            })?
        }
        Some(other) => other.clone(),
        None => envelope.clone(),
    };

    let candidate: CandidatePayload =
        serde_json::from_value(payload).context("classifier output is not a valid candidate")?;

    if candidate.source_identity.trim().is_empty() {
        anyhow::bail!("classifier returned a candidate with no source identity");
    }
    if candidate.content.trim().is_empty() {
        anyhow::bail!("classifier returned a candidate with no content");
    }
    Ok(candidate)
}

/// Reads token usage from the envelope, tolerating its absence.
pub fn parse_usage(envelope: &serde_json::Value) -> Option<TokenUsage> {
    let usage = envelope.get("usage")?;
    Some(TokenUsage {
        input: usage
            .get("input_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
        output: usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    })
}

pub struct ClaudeCli {
    pub bin: String,
    /// The model the classifier runs on, passed as `--model`.
    ///
    /// Without it the CLI inherits whatever the operator's default happens to
    /// be — which on a coding machine is a frontier model. Classification here
    /// is a short, highly structured judgement repeated once per unit, so a
    /// 3000-unit source billed at Opus rates costs several times what the same
    /// run costs on Haiku for no measurable gain in the answer.
    pub model: String,
}

impl ClaudeCli {
    /// Runs the CLI once. The identity is stamped onto the returned candidate so
    /// a model that paraphrases or omits it cannot break idempotency — provenance
    /// is the connector's to decide, never the classifier's.
    pub fn classify(&self, prompt: &str, item: &SourceItem) -> Result<ClassifierOutput> {
        let envelope = self.invoke(prompt)?;

        // Usage is read before the candidate, and kept whatever happens to it.
        let usage = parse_usage(&envelope);
        let candidate = parse_candidate(&envelope)
            .map(|mut c| {
                c.source_identity = item.source_identity.clone();
                c
            })
            .map_err(|e| format!("{e:#}"));
        let raw_response = match envelope.get("result") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => envelope.to_string(),
        };

        Ok(ClassifierOutput {
            candidate,
            usage,
            raw_response,
        })
    }

    /// One run of the CLI, returning its envelope. Shared by both classifying
    /// paths so the isolation flags below can only ever be set in one place.
    fn invoke(&self, prompt: &str) -> Result<serde_json::Value> {
        // Appended here rather than in each connector's `classify_prompt`:
        // needing parseable output is a property of this transport, and four
        // copies of the same sentence is four chances to forget one.
        let prompt = format!(
            "{prompt}\n\nOutput ONLY the JSON object. No prose before or after it, and no \
             markdown code fences."
        );
        let output = Command::new(&self.bin)
            // Run from a neutral directory, not the repository being migrated.
            //
            // The classifier's prompt is self-contained — the file's content is
            // inlined into it — but a `claude` started inside the repo also
            // loads that repo's CLAUDE.md, its `.claude/` skills and its
            // `.mcp.json`. Those are instructions for a coding session, and
            // they win: one run answered "No store_memory call: this turn's job
            // was to propose migration content…" instead of emitting a
            // candidate, and every unit fell back while still being billed.
            //
            // Isolating via CLAUDE_CONFIG_DIR is not an option: it takes the
            // credentials with it and every call answers "Not logged in".
            // `--setting-sources ""` below is what actually keeps the
            // operator's config out.
            .current_dir(std::env::temp_dir())
            .args([
                "-p",
                &prompt,
                // Pinned rather than inherited: the operator's default model is
                // whatever their coding session uses, and this classifier runs
                // once per unit across the whole source.
                "--model",
                &self.model,
                "--output-format",
                "json",
                // The classifier gets its own system prompt, which also replaces
                // the default one's dynamic sections — the operator's CLAUDE.md
                // among them.
                //
                // This is not tidiness. With the user's own instructions in
                // play the model answered "Nada que persistir en este turno…"
                // about a memory protocol instead of returning a candidate:
                // `--output-format json` reports the *last* message, so a
                // closing remark after the JSON replaces the JSON. Every unit
                // fell back, and every unit was billed.
                "--system-prompt",
                CLASSIFIER_SYSTEM_PROMPT,
                // A custom system prompt is not enough on its own, and the gap
                // is expensive: the operator's `settings.json` still loads, and
                // with it every enabled plugin — that plugin's skills and its
                // SessionStart hooks. Measured against a real CSS unit with
                // NexusMind's own plugin installed, the classifier replied "The
                // NexusMind protocol requires `project`… I can't call
                // `store_memory`" instead of a candidate. 42 of 56 units failed
                // that way in one run, all of them billed. Loading no setting
                // sources at all fixes it, and cuts the call from ~46s to ~16s
                // since none of that context is assembled or sent. Auth is not
                // a setting source, so the operator stays logged in.
                "--setting-sources",
                "",
                // Same reasoning for MCP: the servers configured for the
                // operator's coding sessions would otherwise hand the
                // classifier tools like `store_memory`, and a model given a
                // tool tends to reach for it instead of answering.
                //
                // `--mcp-config` is variadic, so it stays last: anything added
                // after it would be swallowed as a second config file.
                "--strict-mcp-config",
                "--mcp-config",
                r#"{"mcpServers":{}}"#,
            ])
            .output()
            .with_context(|| format!("could not run `{}`", self.bin))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let hint = if stderr.contains("unknown option") {
                "\nThis needs a `claude` new enough for --system-prompt, --setting-sources, \
                 --strict-mcp-config and --mcp-config. Without them the classifier inherits \
                 your settings, plugins and MCP servers, and answers the wrong question."
            } else {
                ""
            };
            anyhow::bail!(
                "{} exited with {}: {}{hint}",
                self.bin,
                output.status,
                stderr.trim()
            );
        }

        serde_json::from_slice(&output.stdout).context("classifier output was not valid JSON")
    }

    /// Classifies a whole batch in one call.
    ///
    /// Returns one entry per item in `items`, in order: `Ok` where the model
    /// supplied a usable candidate, `Err` where it did not. Nothing is dropped
    /// and nothing is reordered — the caller pairs the results back up by
    /// position, so a short or scrambled reply degrades per item instead of
    /// costing the batch.
    pub fn classify_bulk(
        &self,
        prompt: &str,
        items: &[SourceItem],
        drafts: &[Option<CandidatePayload>],
    ) -> Result<BulkOutput> {
        let envelope = self.invoke(prompt)?;
        let usage = parse_usage(&envelope);
        let raw_response = match envelope.get("result") {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
            None => envelope.to_string(),
        };

        let parsed: Vec<serde_json::Value> = extract_json_array(&raw_response)
            .and_then(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).ok())
            .unwrap_or_default();

        let candidates = map_bulk_reply(&parsed, items, drafts);

        Ok(BulkOutput {
            candidates,
            usage,
            raw_response,
        })
    }

    pub fn version(&self) -> Option<String> {
        let out = Command::new(&self.bin).arg("--version").output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

// ── Budget ───────────────────────────────────────────────────────────────────

/// A hard ceiling on token spend, checked before each classification.
///
/// A project whose stated goal is reducing token consumption has no business
/// starting with an unbounded burn. Exceeding the budget aborts the run and
/// leaves everything already staged intact — an aborted run is not a lost run,
/// because the staging lives in the backend.
#[derive(Debug, Clone, Default)]
pub struct Budget {
    pub max_tokens: Option<i64>,
    pub spent: i64,
}

impl Budget {
    pub fn would_exceed(&self) -> bool {
        matches!(self.max_tokens, Some(max) if self.spent >= max)
    }
    pub fn record(&mut self, usage: Option<TokenUsage>) {
        if let Some(u) = usage {
            self.spent += u.input + u.output;
        }
    }
}

// ── Pipeline ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSummary {
    pub scanned: usize,
    pub classified: usize,
    pub fallbacks: usize,
    pub failed: usize,
    /// Units the model read and did not put forward.
    ///
    /// Its own number rather than folded into `fallbacks`, because it is the one
    /// that says whether a run was selective. A scan of 3,000 sections that
    /// stages 3,000 candidates has not read anything; one that stages 200 and
    /// passes on 2,800 has.
    pub passed: usize,
    pub aborted_on_budget: bool,
    pub tokens_spent: i64,
}

/// Emits the per-unit outcome.
///
/// Its own function because the same event is produced from three arms of the
/// classification match, and an inlined copy in each is how the three drift
/// apart.
fn emit_classified(
    sink: &EventSink,
    index: usize,
    total: usize,
    item: &SourceItem,
    candidate: &CandidatePayload,
    via: &str,
    tokens_spent: i64,
) {
    if !sink.is_enabled() {
        return;
    }
    sink.emit(&RunEvent::Classified {
        index,
        total,
        origin: item.display_origin.clone(),
        destination_kind: candidate.destination_kind.clone(),
        via: via.to_string(),
        tokens_spent,
    });
}

/// Emits the outcome for a unit the model read and declined to put forward.
///
/// Separate from `emit_failed` because it is not a failure — it is the pipeline
/// doing its job. The TUI counts these so an operator can see, while the run is
/// still going, whether the classifier is being selective or waving everything
/// through.
fn emit_passed(sink: &EventSink, index: usize, total: usize, item: &SourceItem) {
    if !sink.is_enabled() {
        return;
    }
    sink.emit(&RunEvent::Classified {
        index,
        total,
        origin: item.display_origin.clone(),
        destination_kind: String::new(),
        via: "passed".to_string(),
        tokens_spent: 0,
    });
}

fn emit_failed(sink: &EventSink, index: usize, total: usize, item: &SourceItem, spent: i64) {
    sink.emit(&RunEvent::Classified {
        index,
        total,
        origin: item.display_origin.clone(),
        destination_kind: String::new(),
        via: "failed".to_string(),
        tokens_spent: spent,
    });
}

/// How many items a bulk prompt may carry, and how many bytes of them.
///
/// The byte cap matters more than the count: units carry their content, and a
/// batch of large documents is a prompt nobody should send. Whichever limit is
/// reached first closes the batch.
///
/// The count still bounds the *output*, which is what actually runs out: an
/// endorsed item carries its distilled content back, measured at ~450 tokens
/// before the four-sentence limit in [`bulk_prompt`] and well under it after. At
/// forty items a batch where everything is worth keeping still fits; the byte
/// cap is the one that rarely binds, since forty sections of prose are nowhere
/// near 200 KB.
///
/// It was 20 before the reply became sparse, when every item cost a full object
/// whether or not it was worth keeping.
///
/// This is the whole cost story. A call carries ~14k tokens of fixed context
/// before it reads a word of the prompt, so classifying a 500-byte section on
/// its own spends a hundred tokens of overhead for every token of work. Nothing
/// about the classification is expensive; calling is. Batching is not a
/// nice-to-have here, it is the only lever that matters.
pub const BULK_MAX_ITEMS: usize = 40;

/// Rejects `--parallel 0` instead of quietly treating it as serial.
///
/// The pool is only taken when the value is above 1, so a zero would run
/// serially — which was harmless when serial was also the default, and is a
/// silent surprise now that it is not.
fn parse_parallel(raw: &str) -> Result<usize, String> {
    match raw.parse::<usize>() {
        Ok(0) => Err("must be at least 1 — 1 means serial".to_string()),
        Ok(n) => Ok(n),
        Err(e) => Err(e.to_string()),
    }
}

/// Concurrent classifier calls when `--parallel` is not given.
///
/// Four, not one: a classifier call is ~16 s of waiting on the model, so
/// running them one at a time leaves the wall clock proportional to the source
/// size for no benefit. Four, not eight: the calls share one provider account
/// with whatever else the operator is running, and past ~6 the provider starts
/// rate-limiting — which costs more wall clock than it saves.
pub const DEFAULT_PARALLEL: usize = 4;
pub const BULK_MAX_BYTES: usize = 200_000;

/// Groups items into batches the classifier can be asked about in one call.
///
/// Batches never split a source document. A unit is a *section*, and asking
/// "is this section stating a rule?" about a fragment with the rest of its
/// document in another call is asking a worse question — the surrounding
/// headings are most of what makes a section legible. Keeping a document whole
/// costs nothing, because the limits below are reached by many documents, not
/// by one.
///
/// A single document larger than the byte budget is still sent whole rather
/// than split: it is one call either way, and splitting it would only trade the
/// context away for nothing.
pub fn chunk_for_bulk(items: &[SourceItem], max_items: usize) -> Vec<std::ops::Range<usize>> {
    let mut batches = Vec::new();
    let (mut start, mut bytes) = (0usize, 0usize);

    for group in document_groups(items) {
        let size: usize = items[group.clone()].iter().map(|i| i.raw.len()).sum();
        let count = group.end - start;
        // Close the batch *before* this document when adding it would go over,
        // so the document lands whole at the head of the next one.
        let full = group.start > start
            && (count > max_items || bytes + size > BULK_MAX_BYTES);
        if full {
            batches.push(start..group.start);
            start = group.start;
            bytes = 0;
        }
        bytes += size;
    }
    if start < items.len() {
        batches.push(start..items.len());
    }
    batches
}

/// Consecutive runs of items belonging to the same source document.
///
/// Connectors emit a document's units together, so grouping consecutive equal
/// paths is enough — and an item with no routing path (a connector whose units
/// have no document, like a database schema) is its own group, which degrades
/// to the old per-item packing rather than lumping unrelated units together.
fn document_groups(items: &[SourceItem]) -> Vec<std::ops::Range<usize>> {
    let mut groups: Vec<std::ops::Range<usize>> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let same = match (groups.last(), item.routing_path.as_deref()) {
            (Some(last), Some(path)) => {
                items[last.start].routing_path.as_deref() == Some(path)
            }
            _ => false,
        };
        match (same, groups.last_mut()) {
            (true, Some(last)) => last.end = i + 1,
            _ => groups.push(i..i + 1),
        }
    }
    groups
}

/// Pairs a sparse batch reply back to the items it answers.
///
/// Split out of [`ClaudeCli::classify_bulk`] so it can be tested without a
/// subprocess: this is where a model's freedom to answer badly meets the run's
/// need to keep going, and every branch here is one a real reply has taken.
///
/// Keyed by each object's own `item` number rather than by position, because a
/// sparse reply carries no positional meaning. An item nobody mentions is
/// `Unchanged` — that is how the model approves the deterministic draft, and it
/// is the expected answer for most items.
pub fn map_bulk_reply(
    parsed: &[serde_json::Value],
    items: &[SourceItem],
    drafts: &[Option<CandidatePayload>],
) -> Vec<Result<CandidatePayload, BulkMiss>> {
    let mut answers: std::collections::HashMap<usize, &serde_json::Value> =
        std::collections::HashMap::new();
    for value in parsed {
        match value.get("item").and_then(|v| v.as_u64()) {
            // Out-of-range numbers are dropped rather than clamped: a model
            // inventing item 300 of a 120-item batch must not silently
            // overwrite somebody else's unit.
            Some(n) if (1..=items.len() as u64).contains(&n) => {
                answers.insert(n as usize, value);
            }
            _ => {}
        }
    }

    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let value = answers.get(&(i + 1)).ok_or(BulkMiss::Passed)?;
            let mut candidate: CandidatePayload = serde_json::from_value((*value).clone())
                .map_err(|e| BulkMiss::Unusable(format!("item {}: {e}", i + 1)))?;
            if candidate.content.trim().is_empty() {
                return Err(BulkMiss::Unusable(format!("item {}: no content", i + 1)));
            }
            // Provenance is the connector's to decide, never the model's — the
            // same rule as the single-item path.
            candidate.source_identity = item.source_identity.clone();

            // Anything the model left to the pipeline comes from the draft. It
            // is asked for one thing — the knowledge, stated on its own — and
            // answering only that is correct; the draft already worked out the
            // destination and the title from the document's shape.
            let draft = drafts.get(i).and_then(|d| d.as_ref());
            if candidate.destination_kind.trim().is_empty() {
                match draft {
                    Some(d) => candidate.destination_kind = d.destination_kind.clone(),
                    // Nothing proposed a destination, so there is nowhere to put
                    // this. Better a recorded failure than a candidate the
                    // backend refuses at commit time.
                    None => {
                        return Err(BulkMiss::Unusable(format!(
                            "item {}: no destination_kind, and no draft to take one from",
                            i + 1
                        )))
                    }
                }
            }
            if candidate.destination_hint.get("title").is_none() {
                if let Some(title) = draft
                    .and_then(|d| d.destination_hint.get("title"))
                    .cloned()
                {
                    match candidate.destination_hint.as_object_mut() {
                        Some(map) => {
                            map.insert("title".into(), title);
                        }
                        None => {
                            candidate.destination_hint =
                                serde_json::json!({ "title": title })
                        }
                    }
                }
            }
            Ok(candidate)
        })
        .collect()
}

/// The prompt for one batch.
///
/// Each item carries the connector's own per-item instructions verbatim. That
/// repeats them, which looks wasteful until you price it: a single call spends
/// ~14k tokens of fixed context before it reads a word of the prompt, so a
/// hundred repeats of a few hundred tokens is still far cheaper than a hundred
/// calls.
///
/// # Why the reply is sparse
///
/// The batch used to demand one full object per item, in order, "do not skip".
/// That made the *output* the binding constraint — a hundred objects carrying
/// their own content exhausts the output budget long before the input exhausts
/// the context — which is why the batch was capped at twenty items and a
/// 4,987-unit source still took 250 calls.
///
/// Most items do not need the model at all: the deterministic pass has already
/// produced a correct draft, and the honest answer is "yes, that one is fine".
/// Making silence mean assent turns those into zero output tokens instead of a
/// restated object, and the batch can then be sized by input alone.
///
/// The safety property is unchanged and is what makes this affordable: every
/// item already holds a candidate before the call, so a reply that is empty,
/// truncated, mangled or never arrives costs the run nothing.
///
/// # Why the header is about vetoing
///
/// Sparseness moves the model's job. When it had to answer for every item it
/// was improving drafts; when silence means assent it is vetoing them, and a
/// header that only explains the mechanism gets a rubber stamp. Measured on
/// Haiku with five items — three good drafts, one navigation stub, one
/// changelog line — a mechanism-only header returned `[]`, approving all five.
/// The same items with the veto duty stated first returned exactly the two
/// skips and stayed silent on the three good ones.
pub fn bulk_prompt(
    connector: &dyn Connector,
    items: &[SourceItem],
    drafts: &[Option<CandidatePayload>],
) -> String {
    let mut out = format!(
        "You are reading {} sections of a software team's material and putting forward \
         the few that carry knowledge worth keeping.\n\n\
         Reply with a JSON array holding ONLY the items worth migrating. Each object \
         must carry an \"item\" field with that item's number, plus the candidate fields. \
         Anything you leave out is dropped and never reaches a human — that is the \
         correct outcome for most sections, and an empty array [] is a valid reply.\n\n\
         Put an item forward ONLY if someone joining this team next month would be worse \
         off not knowing it. Leave out, without comment:\n\
         - navigation, tables of contents, links to other documents, headings with \
         nothing under them;\n\
         - progress notes, checklists of what was done, changelog entries, dated audits;\n\
         - anything whose meaning depends on the file it sits in.\n\n\
         For each item you do put forward, \"content\" must be the knowledge stated on its \
         own — a claim a reader can use without the surrounding document — in at most \
         four sentences, and including WHY where the section gives a reason. Do not copy \
         the section back. A section you cannot restate that way is a section to leave \
         out.\n\n\
         Each item carries a DRAFT built by rules that cannot read. Use it for the title \
         and the destination it suggests; its content is the raw section text and is NOT \
         an answer. Do not invent items that are not listed.\n",
        items.len()
    );
    for (i, item) in items.iter().enumerate() {
        out.push_str(&format!("\n=== ITEM {} ===\n", i + 1));
        out.push_str(&connector.classify_prompt(item));
        if let Some(Some(draft)) = drafts.get(i) {
            out.push_str(&format!(
                "\n\nDRAFT for this item:\n{}\n",
                serde_json::to_string(draft).unwrap_or_default()
            ));
        }
    }
    out
}

/// Turns scanned items into candidates, a batch at a time, over a worker pool.
///
/// # Why this exists beside `build_candidates`
///
/// A call spends ~14k tokens of fixed context before it reads a word of the
/// prompt, whatever the unit's size, so one call per unit spends a hundred
/// tokens of overhead for every token of work. Batching amortises that: a
/// 4,779-unit source is ~120 calls instead of 4,779.
///
/// # Why it is also parallel
///
/// Batching and a pool used to be alternatives — `--parallel` was ignored here
/// — and that made sense when a batch held twenty items and the pool's job was
/// to overlap thousands of single-unit calls. It stopped making sense when
/// batches became few and large: ~120 calls at ~40 s each is 80 minutes of
/// almost pure waiting, and the wait is the model's, not the machine's.
///
/// The deterministic pass still runs first, single-threaded, and the workers
/// only ever read its output. That is what lets `Connector` stay non-`Sync` —
/// the DB connector wraps a reader that is not `Sync`, and requiring it here
/// would stop that connector compiling.
pub fn build_candidates_bulk(
    connector: &dyn Connector,
    items: &[SourceItem],
    classifier: &ClaudeCli,
    budget: &mut Budget,
    sink: &EventSink,
    max_items: usize,
    workers: usize,
) -> (Vec<CandidatePayload>, RunSummary) {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    let total = items.len();
    let drafts: Vec<Option<CandidatePayload>> =
        items.iter().map(|item| connector.fallback(item)).collect();

    // Built before the pool starts, because building one needs the connector.
    let ranges = chunk_for_bulk(items, max_items);
    let prompts: Vec<String> = ranges
        .iter()
        .map(|r| bulk_prompt(connector, &items[r.clone()], &drafts[r.clone()]))
        .collect();

    let budget_m = Mutex::new(std::mem::take(budget));
    let cursor = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    // Collected per batch and folded in order afterwards. Counters assembled
    // inside the workers would depend on which batch happened to finish first,
    // and a summary that changes between identical runs is not a summary.
    let collected: Mutex<Vec<(usize, Vec<Result<CandidatePayload, BulkMiss>>)>> =
        Mutex::new(Vec::with_capacity(ranges.len()));

    std::thread::scope(|scope| {
        for _ in 0..workers.max(1) {
            scope.spawn(|| loop {
                let b = cursor.fetch_add(1, Ordering::Relaxed);
                if b >= ranges.len() || aborted.load(Ordering::Relaxed) {
                    break;
                }
                // The budget gate, under the lock. Once the ceiling is reached
                // no worker takes new work; whatever is already running finishes
                // and is counted — bounded overshoot, not a hard stop.
                if budget_m.lock().unwrap().would_exceed() {
                    aborted.store(true, Ordering::Relaxed);
                    break;
                }

                let range = ranges[b].clone();
                let slice = &items[range.clone()];
                let batch_drafts = &drafts[range.clone()];
                let prompt = &prompts[b];

                sink.emit(&RunEvent::Classifying {
                    index: range.start + 1,
                    total,
                    origin: format!(
                        "batch of {} · {} … {}",
                        slice.len(),
                        slice.first().map(|i| i.display_origin.as_str()).unwrap_or(""),
                        slice.last().map(|i| i.display_origin.as_str()).unwrap_or("")
                    ),
                });

                let started = std::time::Instant::now();
                let (usage, results, answer) =
                    match classifier.classify_bulk(prompt, slice, batch_drafts) {
                        Ok(out) => (out.usage, out.candidates, out.raw_response),
                        // The call itself failed, so no item was answered. That
                        // is `Unusable`, not `Passed`: nothing was declined
                        // here, nothing was read.
                        Err(e) => (
                            None,
                            slice
                                .iter()
                                .map(|_| Err(BulkMiss::Unusable(format!("{e:#}"))))
                                .collect(),
                            String::new(),
                        ),
                    };
                // This batch's own tokens, from its own usage rather than a
                // before/after diff of the shared budget: under concurrency that
                // diff would fold other workers' spend into this batch's number.
                let spent = usage.map(|u| u.input + u.output).unwrap_or(0);
                budget_m.lock().unwrap().record(usage);

                if sink.is_enabled() {
                    // Only genuine failures. Items the model declined are the
                    // expected majority, and counting them here would mark every
                    // healthy batch as broken.
                    let unusable = results
                        .iter()
                        .filter(|r| matches!(r, Err(BulkMiss::Unusable(_))))
                        .count();
                    let put_forward = results.iter().filter(|r| r.is_ok()).count();
                    sink.emit(&RunEvent::Agent {
                        index: range.start + 1,
                        total,
                        origin: format!(
                            "batch of {} · {put_forward} put forward",
                            slice.len()
                        ),
                        prompt: clip(prompt, AGENT_CLIP),
                        response: clip(&answer, AGENT_CLIP),
                        ok: unusable == 0,
                        error: (unusable > 0)
                            .then(|| format!("{unusable} of {} item(s) unusable", slice.len())),
                        tokens_spent: spent,
                        duration_ms: started.elapsed().as_millis() as u64,
                    });

                    // Per-unit progress is emitted here rather than in the fold
                    // so the operator sees movement while the run is going. The
                    // fold below owns the counters; this owns the screen.
                    for (offset, result) in results.iter().enumerate() {
                        let index = range.start + offset + 1;
                        let item = &slice[offset];
                        match result {
                            Ok(c) => {
                                emit_classified(sink, index, total, item, c, "classified", 0)
                            }
                            Err(BulkMiss::Passed) => emit_passed(sink, index, total, item),
                            Err(BulkMiss::Unusable(_)) => match &batch_drafts[offset] {
                                Some(d) => {
                                    emit_classified(sink, index, total, item, d, "fallback", 0)
                                }
                                None => emit_failed(sink, index, total, item, 0),
                            },
                        }
                    }
                }

                collected.lock().unwrap().push((b, results));
            });
        }
    });

    *budget = budget_m.into_inner().unwrap();
    let mut batches = collected.into_inner().unwrap();
    batches.sort_by_key(|(b, _)| *b);

    let mut summary = RunSummary {
        scanned: total,
        aborted_on_budget: aborted.load(Ordering::Relaxed),
        tokens_spent: budget.spent,
        ..Default::default()
    };
    let mut candidates = Vec::new();
    for (b, results) in batches {
        let range = ranges[b].clone();
        for (offset, result) in results.into_iter().enumerate() {
            match result {
                Ok(candidate) => {
                    summary.classified += 1;
                    candidates.push(candidate);
                }
                // A section the model did not put forward is dropped. It is NOT
                // filled in from the draft: the draft's content is the raw
                // section text, so doing that would stage the document verbatim
                // one heading at a time and bury the reviewer in prose nobody
                // vouched for.
                Err(BulkMiss::Passed) => summary.passed += 1,
                // A broken answer is different: the model tried and we could not
                // read it, so the deterministic draft is the floor that keeps
                // the run from losing the unit entirely.
                Err(BulkMiss::Unusable(_)) => match drafts[range.start + offset].clone() {
                    Some(draft) => {
                        summary.fallbacks += 1;
                        candidates.push(draft);
                    }
                    None => summary.failed += 1,
                },
            }
        }
    }

    (candidates, summary)
}

/// Turns scanned items into candidates.
///
/// Three properties, all deliberate:
///
/// 1. A classification failure is recorded and the run continues. A run over 500
///    documents cannot die on document three.
/// 2. When the classifier fails, the connector's deterministic fallback is used
///    if it has one, so the run degrades rather than stops.
/// 3. Exceeding the budget stops the loop cleanly and reports it, rather than
///    silently truncating.
pub fn build_candidates(
    connector: &dyn Connector,
    items: &[SourceItem],
    classifier: Option<&ClaudeCli>,
    budget: &mut Budget,
    sink: &EventSink,
) -> (Vec<CandidatePayload>, RunSummary) {
    let mut candidates = Vec::new();
    let mut summary = RunSummary {
        scanned: items.len(),
        ..Default::default()
    };

    let total = items.len();
    for (index, item) in items.iter().enumerate() {
        if budget.would_exceed() {
            summary.aborted_on_budget = true;
            break;
        }
        let index = index + 1;
        sink.emit(&RunEvent::Classifying {
            index,
            total,
            origin: item.display_origin.clone(),
        });
        let before = budget.spent;

        match classifier {
            Some(cli) => {
                // The model ran or it did not. When it ran, its usage is
                // recorded before anything else — including when its answer
                // turns out to be unusable, because those tokens are just as
                // spent and the budget must still see them.
                let prompt = connector.classify_prompt(item);
                let started = std::time::Instant::now();
                let (usage, outcome, answer) = match cli.classify(&prompt, item) {
                    Ok(out) => (out.usage, out.candidate, out.raw_response),
                    // The process never ran, so there is no answer to show and
                    // no usage to record — only the reason.
                    Err(e) => (None, Err(format!("{e:#}")), String::new()),
                };
                budget.record(usage);
                let spent = budget.spent - before;

                if sink.is_enabled() {
                    sink.emit(&RunEvent::Agent {
                        index,
                        total,
                        origin: item.display_origin.clone(),
                        prompt: clip(&prompt, AGENT_CLIP),
                        response: clip(&answer, AGENT_CLIP),
                        ok: outcome.is_ok(),
                        error: outcome.as_ref().err().cloned(),
                        tokens_spent: spent,
                        duration_ms: started.elapsed().as_millis() as u64,
                    });
                }

                match outcome {
                    Ok(candidate) => {
                        summary.classified += 1;
                        emit_classified(sink, index, total, item, &candidate, "classified", spent);
                        candidates.push(candidate);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "classifier failed for {}: {e}; falling back",
                            item.display_origin
                        );
                        match connector.fallback(item) {
                            Some(c) => {
                                summary.fallbacks += 1;
                                emit_classified(sink, index, total, item, &c, "fallback", spent);
                                candidates.push(c);
                            }
                            None => {
                                summary.failed += 1;
                                emit_failed(sink, index, total, item, spent);
                            }
                        }
                    }
                }
            }
            None => match connector.fallback(item) {
                Some(c) => {
                    summary.fallbacks += 1;
                    emit_classified(sink, index, total, item, &c, "fallback", 0);
                    candidates.push(c);
                }
                None => {
                    summary.failed += 1;
                    emit_failed(sink, index, total, item, 0);
                }
            },
        }
    }

    summary.tokens_spent = budget.spent;
    (candidates, summary)
}

/// The fate of one item, collected out of order by the pool and reassembled by
/// index in [`assemble_parallel`].
enum ItemOutcome {
    Classified(CandidatePayload),
    Fallback(CandidatePayload),
    Failed,
}

/// Reassembles the workers' out-of-order results into the same ordered output
/// the serial path produces, and tallies the summary from it.
///
/// Split out from the threading on purpose: the part most likely to hide a bug —
/// ordering, the hole a budget abort leaves where an item never ran, counting —
/// is a pure function with a test, not something only a live `claude` can reach.
fn assemble_parallel(
    mut results: Vec<(usize, ItemOutcome)>,
    scanned: usize,
    aborted_on_budget: bool,
    tokens_spent: i64,
) -> (Vec<CandidatePayload>, RunSummary) {
    results.sort_by_key(|(i, _)| *i);
    let mut summary = RunSummary {
        scanned,
        aborted_on_budget,
        tokens_spent,
        ..Default::default()
    };
    let mut candidates = Vec::new();
    for (_, outcome) in results {
        match outcome {
            ItemOutcome::Classified(c) => {
                summary.classified += 1;
                candidates.push(c);
            }
            ItemOutcome::Fallback(c) => {
                summary.fallbacks += 1;
                candidates.push(c);
            }
            ItemOutcome::Failed => summary.failed += 1,
        }
    }
    (candidates, summary)
}

/// The per-item classifier, run across a small pool of workers.
///
/// # Why this exists beside `build_candidates`
///
/// One `claude -p` per unit is ~32 s of mostly waiting: the process blocks on
/// the model. A few of those at once cut the wall clock by roughly the pool
/// size without changing what any single call does — every invariant of the
/// serial path is preserved here deliberately, including that usage is recorded
/// whether or not the answer turned out usable.
///
/// It does NOT reduce spend: each item still costs its full call, so the budget
/// behaves as in the serial path (`--bulk` is the mode that lowers cost). It
/// does add *bounded* overshoot — with N workers, up to N-1 calls can already
/// be in flight when the ceiling trips, so the budget is honoured to within one
/// round of concurrency, not to the token.
///
/// Prompts and drafts are built up front, single-threaded, so the workers never
/// touch the connector. That is what lets `Connector` stay non-`Sync` — the DB
/// connector wraps a reader that is not `Sync`, and requiring it here would stop
/// that connector compiling.
fn build_candidates_parallel(
    connector: &dyn Connector,
    items: &[SourceItem],
    classifier: &ClaudeCli,
    budget: &mut Budget,
    sink: &EventSink,
    workers: usize,
) -> (Vec<CandidatePayload>, RunSummary) {
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Mutex;

    let total = items.len();
    // Built before the pool starts; the workers only ever read these.
    let prompts: Vec<String> = items.iter().map(|i| connector.classify_prompt(i)).collect();
    let drafts: Vec<Option<CandidatePayload>> =
        items.iter().map(|i| connector.fallback(i)).collect();

    let budget_m = Mutex::new(std::mem::take(budget));
    let cursor = AtomicUsize::new(0);
    let aborted = AtomicBool::new(false);
    let results: Mutex<Vec<(usize, ItemOutcome)>> = Mutex::new(Vec::with_capacity(total));

    std::thread::scope(|scope| {
        for _ in 0..workers.max(1) {
            scope.spawn(|| loop {
                let i = cursor.fetch_add(1, Ordering::Relaxed);
                if i >= total {
                    break;
                }
                if aborted.load(Ordering::Relaxed) {
                    break;
                }
                // The budget gate, under the lock. Once the ceiling is reached
                // no worker takes new work; whatever is already running finishes
                // and is counted — that is the bounded overshoot above.
                if budget_m.lock().unwrap().would_exceed() {
                    aborted.store(true, Ordering::Relaxed);
                    break;
                }

                let item = &items[i];
                let index = i + 1;
                let prompt = &prompts[i];
                sink.emit(&RunEvent::Classifying {
                    index,
                    total,
                    origin: item.display_origin.clone(),
                });

                let started = std::time::Instant::now();
                let (usage, outcome, answer) = match classifier.classify(prompt, item) {
                    Ok(out) => (out.usage, out.candidate, out.raw_response),
                    Err(e) => (None, Err(format!("{e:#}")), String::new()),
                };
                // This call's own tokens, taken from its own usage rather than a
                // before/after diff of the shared budget: under concurrency that
                // diff would fold other workers' spend into this item's number.
                let spent = usage.map(|u| u.input + u.output).unwrap_or(0);
                budget_m.lock().unwrap().record(usage);

                if sink.is_enabled() {
                    sink.emit(&RunEvent::Agent {
                        index,
                        total,
                        origin: item.display_origin.clone(),
                        prompt: clip(prompt, AGENT_CLIP),
                        response: clip(&answer, AGENT_CLIP),
                        ok: outcome.is_ok(),
                        error: outcome.as_ref().err().cloned(),
                        tokens_spent: spent,
                        duration_ms: started.elapsed().as_millis() as u64,
                    });
                }

                let resolved = match outcome {
                    Ok(candidate) => {
                        emit_classified(sink, index, total, item, &candidate, "classified", spent);
                        ItemOutcome::Classified(candidate)
                    }
                    Err(e) => {
                        tracing::warn!(
                            "classifier failed for {}: {e}; falling back",
                            item.display_origin
                        );
                        match drafts[i].clone() {
                            Some(c) => {
                                emit_classified(sink, index, total, item, &c, "fallback", spent);
                                ItemOutcome::Fallback(c)
                            }
                            None => {
                                emit_failed(sink, index, total, item, spent);
                                ItemOutcome::Failed
                            }
                        }
                    }
                };
                results.lock().unwrap().push((i, resolved));
            });
        }
    });

    *budget = budget_m.into_inner().unwrap();
    let results = results.into_inner().unwrap();
    assemble_parallel(
        results,
        total,
        aborted.load(Ordering::Relaxed),
        budget.spent,
    )
}

// ── CLI ──────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "migrate-knowledge",
    about = "Scan a source, classify locally, and stage candidates for human review."
)]
struct Args {
    /// Which connector to run. Only `noop` ships in the core change; the real
    /// connectors arrive with their own changes.
    #[arg(long, default_value = "noop")]
    source: String,

    /// Classify in batches instead of one call per unit.
    ///
    /// The deterministic pass runs first either way; in this mode its output is
    /// sent to the model in groups to be improved, which trades a little
    /// per-item attention for a large amount of time and spend.
    #[arg(long)]
    bulk: bool,

    /// Items per batch in `--bulk`. Capped by a byte budget as well.
    #[arg(long, default_value_t = BULK_MAX_ITEMS)]
    batch_size: usize,

    /// Classify this many units at once, each in its own `claude` call.
    ///
    /// The per-item path is otherwise one call at a time, and each call is ~16 s
    /// of mostly waiting on the model — so a 249-unit source spends over an
    /// hour almost entirely idle. A small pool cuts the wall clock by roughly
    /// this factor.
    ///
    /// Defaults to a pool rather than serial: the wait is the model's, not the
    /// machine's, so serial buys nothing and costs the operator the difference.
    /// Pass `--parallel 1` to get the old one-at-a-time behaviour back.
    ///
    /// Applies to both paths: per-unit calls, and — since batches became few
    /// and large — the batches themselves. It does NOT lower token spend on the
    /// per-unit path, where every unit still costs its own call; `--bulk` is the
    /// mode that lowers cost, and this is the one that lowers the clock. Values
    /// much above ~6 risk the provider rate-limiting the concurrent calls.
    #[arg(long, default_value_t = DEFAULT_PARALLEL, value_parser = parse_parallel)]
    parallel: usize,

    /// Emit NDJSON progress events on stdout instead of prose.
    ///
    /// For supervising processes — the TUI in `apps/migrator-tui` is the first
    /// one. In this mode stdout carries *only* events, and logs are redirected
    /// to stderr, because a stray log line inside the stream breaks every
    /// consumer that reads it line by line.
    #[arg(long)]
    json: bool,

    #[arg(long, default_value = ".")]
    path: String,

    /// Explicit NexusMind repository config. Otherwise `.nexusmind.yaml` is
    /// discovered upward from --path, stopping at the Git root.
    #[arg(long)]
    config: Option<String>,

    /// Fail when no repository config is discoverable.
    #[arg(long)]
    require_config: bool,

    #[arg(long, env = "NEXUSMIND_BASE_URL")]
    api_url: Option<String>,

    #[arg(long, env = "NEXUSMIND_API_KEY", hide_env_values = true)]
    api_key: Option<String>,

    #[arg(long)]
    client: Option<String>,

    #[arg(long)]
    project: Option<String>,

    /// Scan and estimate without classifying or posting anything.
    #[arg(long)]
    dry_run: bool,

    /// Skip the model entirely and use each connector's deterministic fallback.
    /// The mode to use when a client's material may not leave the machine.
    #[arg(long)]
    no_llm: bool,

    #[arg(long)]
    max_tokens: Option<i64>,

    #[arg(long, default_value = "claude")]
    claude_bin: String,

    /// Model for the classifier. Defaults to Haiku: classification is a short,
    /// repetitive judgement, and it runs once per unit — a frontier model here
    /// multiplies the bill of a large source without improving the answer.
    #[arg(long, default_value = "claude-haiku-4-5")]
    model: String,

    /// Narrow the scan to paths containing any of these fragments. The way to
    /// keep a first pass affordable: `--include docs/adr` instead of the whole
    /// tree. Repeatable, or comma-separated.
    #[arg(long, value_delimiter = ',')]
    include: Vec<String>,

    /// Skip paths containing any of these fragments, on top of each connector's
    /// own defaults.
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,

    /// Declared only so passing it can be REFUSED with an explanation. The DSN
    /// belongs in NEXUSMIND_SOURCE_DSN; on a command line it survives in shell
    /// history, in `ps`, and in anything that logs commands.
    #[arg(long)]
    dsn: Option<String>,

    /// Describe row-level security policies as the access rules they are.
    #[arg(long)]
    supabase: bool,

    /// Read business rows. OFF by default and gated by four cumulative
    /// conditions — see ADR db359a75. A flag you forget to set is a mistake;
    /// a flag you must set on purpose is a decision.
    #[arg(long)]
    include_data: bool,

    /// The explicit table allowlist for sampling. There is deliberately no --all.
    #[arg(long, value_delimiter = ',')]
    tables: Vec<String>,

    #[arg(long)]
    sample_limit: Option<usize>,

    /// Redact PII locally before any sample leaves this process.
    #[arg(long)]
    redact_pii: bool,

    /// Who authorised reading this client's data, and when. Recorded on the run.
    #[arg(long)]
    attest: Option<String>,

    /// Scan only the history after this commit — the incremental second pass
    /// over a long-lived repository.
    #[arg(long)]
    since_commit: Option<String>,

    /// Which slice of the machine this material belongs to: `global`, or a
    /// project slug. Never the machine or user name — that would be PII inside a
    /// source identity.
    #[arg(long)]
    host_scope: Option<String>,

    /// Let `openspec/changes/**` produce `sdd_artifact` candidates.
    ///
    /// Off by default: in this repository `import-sdd` already backfilled them,
    /// and two paths to one destination is how duplicates happen. Turn it on
    /// when migrating a repo where that importer never ran.
    #[arg(long)]
    include_sdd: bool,
}

/// Args → `ScanOptions`.
///
/// Its own function because it was once inlined and silently dropped `include`
/// and `exclude`: the flags existed in the guide and never reached the scan, so
/// every run was a full-tree run. A mapping worth a test.
fn scan_options_for(args: &Args) -> ScanOptions {
    ScanOptions {
        root: args.path.clone(),
        includes: args.include.clone(),
        excludes: args.exclude.clone(),
        progress: None,
    }
}

/// How often a walking scan reports in.
///
/// The first few are emitted unthrottled so a consumer sees life immediately —
/// the difference between "starting" and "hung" is decided in the first second,
/// not the first thousand files.
const SCAN_HEARTBEAT_EVERY: usize = 25;
const SCAN_HEARTBEAT_FIRST: usize = 5;

/// How much of each side of an exchange reaches the stream.
///
/// Enough to see the question, the answer, and where an answer went wrong;
/// small enough that a 3000-unit run does not write a second copy of the
/// repository to stdout.
const AGENT_CLIP: usize = 2_000;

/// The classifier's whole persona. Short on purpose: it has one job, and any
/// room left over is room for a different one.
const CLASSIFIER_SYSTEM_PROMPT: &str =
    "You are a classifier. Your entire reply is one JSON object and nothing else — no prose \
     before or after it, no markdown fences, and no closing remarks.";

/// Attaches the heartbeat to a scan, when anyone is listening for it.
///
/// Throttling lives here rather than in `ScanOptions::note` so the hook itself
/// stays honest: connectors report every source they examine, and the decision
/// about how much of that to put on the wire belongs to whoever is writing to
/// the wire.
fn with_scan_progress(mut opts: ScanOptions, sink: &Arc<EventSink>) -> ScanOptions {
    if !sink.is_enabled() {
        return opts;
    }
    let sink = Arc::clone(sink);
    opts.progress = Some(Arc::new(move |p: ScanProgress| {
        if p.seen <= SCAN_HEARTBEAT_FIRST || p.seen.is_multiple_of(SCAN_HEARTBEAT_EVERY) {
            sink.emit(&RunEvent::Scanning {
                seen: p.seen,
                current: p.current,
            });
        }
    }));
    opts
}

fn connector_for(source: &str, args: &Args) -> Result<Box<dyn Connector>> {
    match source {
        "noop" => Ok(Box::new(NoopConnector::with_sample())),
        "repo-docs" => Ok(Box::new(
            RepoDocsConnector::new(RepoDocsConnector::repo_name_for(&args.path))
                .with_sdd(args.include_sdd),
        )),
        "claude-memories" => Ok(Box::new(ClaudeMemoriesConnector::new(
            args.host_scope
                .clone()
                .unwrap_or_else(|| "global".to_string()),
        ))),
        "git-history" => Ok(Box::new(
            GitHistoryConnector::new(GitHistoryConnector::repo_name_for(&args.path))
                .since(args.since_commit.clone()),
        )),
        "source-code" => Ok(Box::new(SourceCodeConnector::new(
            SourceCodeConnector::repo_name_for(&args.path),
        ))),
        "db-schema" => {
            // The DSN never comes from argv: it would survive in shell history,
            // in `ps`, and in anything that logs commands.
            if args.dsn.is_some() {
                anyhow::bail!(
                    "dsn_in_argv: pass the connection string in NEXUSMIND_SOURCE_DSN instead. \
                     A DSN on the command line survives in your shell history, in `ps` output \
                     and in any command logging."
                );
            }
            let dsn = std::env::var("NEXUSMIND_SOURCE_DSN").map_err(|_| {
                anyhow::anyhow!(
                    "missing_dsn: set NEXUSMIND_SOURCE_DSN to the source database connection \
                     string. Use a READ-ONLY role — the connector refuses one that can write."
                )
            })?;
            let reader = PgSchemaReader::connect(&dsn)?;
            Ok(Box::new(
                DbSchemaConnector::new(reader)
                    .with_supabase(args.supabase)
                    .with_sampling(SamplingPolicy {
                        enabled: args.include_data,
                        allowlist: args.tables.clone(),
                        limit: args.sample_limit,
                        redact_pii: args.redact_pii,
                        attestation: args.attest.clone(),
                    }),
            ))
        }
        other => anyhow::bail!("unknown source `{other}`"),
    }
}

/// Splits candidates into batches the backend will accept.
///
/// # Why bytes and not just a count
///
/// The server caps a batch at 500 candidates, and its own comment says "the
/// runner chunks; the cap is a backstop". The runner did not chunk. A scan of
/// 248 Claude assets produced a 2.6 MB request, which the framework refused to
/// buffer at all — `400 Failed to buffer the request body`, with the entire
/// batch lost. Candidates carry their full content, so the binding constraint
/// is size, not population: 248 items is a comfortable count and an impossible
/// request.
///
/// A candidate that exceeds the limit on its own is still sent, alone. It will
/// be rejected with a message naming it, which is strictly better than being
/// dropped here where nobody would ever hear about it.
const MAX_BATCH_BYTES: usize = 1_000_000;
const MAX_BATCH_ITEMS: usize = 200;

pub fn chunk_candidates(candidates: Vec<CandidatePayload>) -> Vec<Vec<CandidatePayload>> {
    let mut batches: Vec<Vec<CandidatePayload>> = Vec::new();
    let mut current: Vec<CandidatePayload> = Vec::new();
    let mut bytes = 0usize;

    for candidate in candidates {
        let size = serde_json::to_string(&candidate)
            .map(|s| s.len())
            .unwrap_or(0);
        let would_overflow = !current.is_empty()
            && (bytes + size > MAX_BATCH_BYTES || current.len() >= MAX_BATCH_ITEMS);
        if would_overflow {
            batches.push(std::mem::take(&mut current));
            bytes = 0;
        }
        bytes += size;
        current.push(candidate);
    }
    if !current.is_empty() {
        batches.push(current);
    }
    batches
}

/// Turns a failed response into an error that says what the server said.
///
/// `error_for_status` reports only the code. A backend that answers
/// `400 {"error":"...","code":"..."}` is telling you exactly what is wrong, and
/// discarding that leaves the operator with a number and no next step — which
/// is precisely what happened on a 400 from the staging endpoint.
fn read_body(response: reqwest::blocking::Response, what: &str) -> Result<serde_json::Value> {
    let status = response.status();
    if status.is_success() {
        return response
            .json()
            .with_context(|| format!("{what}: response was not JSON"));
    }
    let body = response.text().unwrap_or_default();
    let detail = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| {
            let msg = v.get("error")?.as_str()?.to_string();
            let code = v
                .get("code")
                .and_then(|c| c.as_str())
                .map(|c| format!(" [{c}]"))
                .unwrap_or_default();
            Some(format!("{msg}{code}"))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    anyhow::bail!("{what}: {status} — {detail}")
}

/// Runs the pipeline, reporting the outcome exactly once.
///
/// The summary lives here rather than inside `execute` so a failure cannot
/// erase it. A run that classified a unit and then failed to reach the backend
/// used to emit `finished` with zeros — the same lie as the token counter it
/// sat next to, and for the same reason: real work reported as nothing because
/// the error path built a fresh, empty record.
fn run(args: &Args) -> Result<()> {
    let sink = Arc::new(EventSink::new(args.json));
    let mut summary = RunSummary::default();
    let result = execute(args, &sink, &mut summary);
    match &result {
        Ok(()) => emit_finished(&sink, &summary, None),
        Err(e) => emit_finished(&sink, &summary, Some(format!("{e:#}"))),
    }
    result
}

fn execute(args: &Args, sink: &Arc<EventSink>, summary: &mut RunSummary) -> Result<()> {
    let prose = !args.json;
    macro_rules! say {
        ($($arg:tt)*) => { if prose { println!($($arg)*) } };
    }

    let connector = connector_for(&args.source, args)?;
    let opts = with_scan_progress(scan_options_for(args), sink);

    sink.emit(&RunEvent::Started {
        source: connector.source_kind().to_string(),
        path: args.path.clone(),
        dry_run: args.dry_run,
        no_llm: args.no_llm,
        max_tokens: args.max_tokens,
    });

    let report = connector.scan_report(&opts)?;

    let selection = match args.config.as_deref() {
        Some(config) => ConfigSelection::ExplicitFrom {
            config: PathBuf::from(config),
            source: PathBuf::from(&args.path),
        },
        None => ConfigSelection::DiscoverFrom(PathBuf::from(&args.path)),
    };
    let snapshot = repository_config::load(selection, args.require_config)?;
    let mut groups = Vec::new();
    let mut routing_unmapped = 0usize;
    if let Some(snapshot_value) = snapshot.clone() {
        sink.emit(&RunEvent::ConfigLoaded {
            repository_id: snapshot_value.config.repository.id.clone(),
            path: snapshot_value.relative_path.clone(),
            sha256: snapshot_value.sha256.clone(),
            project_count: snapshot_value.config.projects.len(),
        });
        let resolver = ProjectResolver::compile(snapshot_value.clone())?;
        let override_ = DestinationOverride {
            project_id: args.project.clone(),
            client_id: args.client.clone(),
        };
        let plan = resolver.plan_paths(
            report.items.iter().map(|item| item.routing_path.as_deref()),
            &override_,
        )?;
        routing_unmapped = plan.unmapped_indices.len();
        for group in plan.groups {
            let sample_paths = group
                .item_indices
                .iter()
                .filter_map(|i| report.items[*i].routing_path.clone())
                .take(3)
                .collect::<Vec<_>>();
            sink.emit(&RunEvent::RoutingGroup {
                alias: group.destination.alias.clone(),
                project_id: group.destination.project_id.clone(),
                client_id: group.destination.client_id.clone(),
                item_count: group.item_indices.len(),
                sample_paths,
            });
            groups.push(ExecutionGroup {
                alias: group.destination.alias.clone(),
                project_id: group.destination.project_id.clone(),
                client_id: group.destination.client_id.clone(),
                item_indices: group.item_indices,
                attestation: serde_json::json!({ "repository_config": {
                    "schema_version": 1,
                    "repository_id": snapshot_value.config.repository.id,
                    "path": snapshot_value.relative_path,
                    "sha256": snapshot_value.sha256,
                    "project_alias": group.destination.alias,
                    "project_id": group.destination.project_id,
                    "client_id": group.destination.client_id,
                    "selection": group.destination.basis,
                }}),
            });
        }
    } else if let Some(project) = args.project.clone() {
        groups.push(ExecutionGroup {
            alias: "explicit".into(),
            project_id: project,
            client_id: args.client.clone(),
            item_indices: (0..report.items.len()).collect(),
            attestation: serde_json::json!({}),
        });
    } else {
        routing_unmapped = report.items.len();
    }
    if routing_unmapped > 0 {
        sink.emit(&RunEvent::RoutingIssue {
            kind: "unmapped".into(),
            count: routing_unmapped,
            sample: report
                .items
                .iter()
                .find(|item| item.routing_path.is_none())
                .map(|item| item.display_origin.clone()),
        });
    }
    sink.emit(&RunEvent::RoutingReady {
        groups: groups.len(),
        mapped_items: groups.iter().map(|g| g.item_indices.len()).sum(),
        unmapped_items: routing_unmapped,
    });

    sink.emit(&RunEvent::Scanned {
        documents: report.documents,
        units: report.units,
        bytes: report.bytes,
        estimated_tokens: report.estimated_tokens(),
        excluded: report.excluded.len(),
    });
    if sink.is_enabled() {
        // Grouped, not enumerated. See `RunEvent::Excluded`.
        let mut by_reason: std::collections::BTreeMap<&str, (usize, &str)> = Default::default();
        for (path, reason) in &report.excluded {
            let entry = by_reason
                .entry(reason.as_str())
                .or_insert((0, path.as_str()));
            entry.0 += 1;
        }
        for (reason, (count, sample)) in by_reason {
            sink.emit(&RunEvent::Excluded {
                reason: reason.to_string(),
                count,
                sample: sample.to_string(),
            });
        }
    }

    if args.dry_run {
        summary.scanned = report.units;
        say!(
            "dry run — source={} documents={} units={} bytes={} estimated_tokens≈{}",
            connector.source_kind(),
            report.documents,
            report.units,
            report.bytes,
            report.estimated_tokens(),
        );
        if !report.excluded.is_empty() {
            say!("excluded {} document(s):", report.excluded.len());
            for (path, reason) in &report.excluded {
                say!("  - {path} — {reason}");
            }
        }
        say!("no classification was run and nothing was posted.");
        say!(
            "routing — groups={} unmapped={routing_unmapped}",
            groups.len()
        );
        return Ok(());
    }
    if routing_unmapped > 0 {
        anyhow::bail!("ROUTING_UNMAPPED: {routing_unmapped} item(s) have no project destination");
    }
    let items = report.items;

    let cli = (!args.no_llm).then(|| ClaudeCli {
        bin: args.claude_bin.clone(),
        model: args.model.clone(),
    });
    let mut budget = Budget {
        max_tokens: args.max_tokens,
        spent: 0,
    };
    let mut candidate_groups = Vec::new();
    summary.scanned = items.len();
    for group in &groups {
        let group_items = group
            .item_indices
            .iter()
            .map(|i| items[*i].clone())
            .collect::<Vec<_>>();
        let (candidates, built) = match (&cli, args.bulk) {
            (Some(cli), true) => build_candidates_bulk(
                connector.as_ref(),
                &group_items,
                cli,
                &mut budget,
                sink,
                args.batch_size.max(1),
                args.parallel,
            ),
            // Parallel is the per-item path with a worker pool; it remains
            // scoped to the resolved client/project execution group.
            (Some(cli), false) if args.parallel > 1 => build_candidates_parallel(
                connector.as_ref(),
                &group_items,
                cli,
                &mut budget,
                sink,
                args.parallel,
            ),
            _ => build_candidates(
                connector.as_ref(),
                &group_items,
                cli.as_ref(),
                &mut budget,
                sink,
            ),
        };
        summary.classified += built.classified;
        summary.fallbacks += built.fallbacks;
        summary.failed += built.failed;
        summary.tokens_spent = budget.spent;
        summary.aborted_on_budget |= built.aborted_on_budget;
        candidate_groups.push((group.clone(), candidates));
        if summary.aborted_on_budget {
            break;
        }
    }

    // A blank value counts as absent. `NEXUSMIND_BASE_URL=` in a shell — the
    // usual way to neutralise the production default for one command — arrives
    // as `Some("")`, and posting to it fails with `builder error: relative URL
    // without a base`, which tells the operator nothing about what they did.
    let api_url = args
        .api_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    let Some(api_url) = api_url else {
        say!(
            "{} candidate(s) built; no --api-url given, so nothing was posted.",
            candidate_groups.iter().map(|(_, c)| c.len()).sum::<usize>()
        );
        say!("{summary:?}");
        return Ok(());
    };
    let api_key = args
        .api_key
        .as_deref()
        .context("--api-key (or NEXUSMIND_API_KEY) is required to post candidates")?;

    if let Some(snapshot) = snapshot.as_ref() {
        snapshot.verify_current()?;
    }
    let http = reqwest::blocking::Client::new();
    for (group, candidates) in candidate_groups {
        let run_body = serde_json::json!({
            "source_kind": connector.source_kind(), "client_id": group.client_id,
            "project_id": group.project_id, "source_ref": args.path,
            "runner_version": cli.as_ref().and_then(|c| c.version()), "attestation": group.attestation,
        });
        let created = read_body(
            http.post(format!("{api_url}/v1/migrations"))
                .bearer_auth(api_key)
                .json(&run_body)
                .send()?,
            "creating the run",
        )?;
        let run_id = created
            .get("id")
            .and_then(|v| v.as_str())
            .context("backend did not return a run id")?;
        sink.emit(&RunEvent::RunCreated {
            alias: group.alias.clone(),
            project_id: group.project_id.clone(),
            run_id: run_id.to_string(),
        });

        let total_candidates = candidates.len();
        let batches = chunk_candidates(candidates);
        let batch_count = batches.len();
        let mut totals = (0usize, 0usize, 0usize);

        for (i, batch) in batches.into_iter().enumerate() {
            let size = batch.len();
            let reply = read_body(
                http.post(format!("{api_url}/v1/migrations/{run_id}/candidates"))
                    .bearer_auth(api_key)
                    .json(&serde_json::json!({ "candidates": batch }))
                    .send()?,
                &format!(
                    "staging batch {}/{batch_count} ({size} candidate(s))",
                    i + 1
                ),
            )?;
            let count =
                |key: &str| reply.get(key).and_then(|v| v.as_u64()).unwrap_or_default() as usize;
            totals.0 += count("staged");
            totals.1 += count("skipped");
            totals.2 += count("rejected");

            // Emitted per batch with running totals, so a long staging phase shows
            // movement instead of a pause at 100% of classification.
            sink.emit(&RunEvent::Staged {
                run_id: run_id.to_string(),
                staged: totals.0,
                skipped: totals.1,
                rejected: totals.2,
            });
        }
        let staged = serde_json::json!({
            "staged": totals.0,
            "skipped": totals.1,
            "rejected": totals.2,
            "batches": batch_count,
            "candidates": total_candidates,
        });

        say!("run {run_id}: {staged}");
    }
    say!("{summary:?}");
    if summary.aborted_on_budget {
        say!(
            "token budget reached — the candidates already staged are intact and the run is \
             resumable."
        );
    }
    say!("nothing has been committed: every candidate awaits human review.");
    Ok(())
}

/// The terminal event.
///
/// Extracted so every exit path from `run` goes through one place — including
/// the error path in `main`, which is the one an unsupervised consumer depends
/// on most: without a `finished` event it cannot distinguish a crash from a run
/// still in progress.
fn emit_finished(sink: &EventSink, summary: &RunSummary, error: Option<String>) {
    sink.emit(&RunEvent::Finished {
        ok: error.is_none(),
        scanned: summary.scanned,
        classified: summary.classified,
        fallbacks: summary.fallbacks,
        failed: summary.failed,
        tokens_spent: summary.tokens_spent,
        aborted_on_budget: summary.aborted_on_budget,
        error,
    });
}

fn main() {
    let args = Args::parse();
    let filter =
        tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    // In `--json` mode stdout belongs to the event stream alone. A log line in
    // the middle of it is not a cosmetic problem: it is a parse error for the
    // consumer reading a line at a time.
    if args.json {
        tracing_subscriber::fmt()
            .with_env_filter(filter)
            .with_writer(std::io::stderr)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    if let Err(e) = run(&args) {
        // `run` has already emitted the terminal event, with the real counts.
        eprintln!("✗ {e:#}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str) -> SourceItem {
        SourceItem {
            source_identity: id.to_string(),
            display_origin: format!("origin of {id}"),
            routing_path: None,
            raw: "some prose".to_string(),
            meta: serde_json::json!({}),
        }
    }

    /// A connector whose fallback returns `None` — the "needs judgement" case.
    struct NoFallback;
    impl Connector for NoFallback {
        fn source_kind(&self) -> &'static str {
            "noop"
        }
        fn scan(&self, _: &ScanOptions) -> Result<Vec<SourceItem>> {
            Ok(vec![item("a"), item("b")])
        }
        fn classify_prompt(&self, _: &SourceItem) -> String {
            "classify".to_string()
        }
        fn fallback(&self, _: &SourceItem) -> Option<CandidatePayload> {
            None
        }
    }

    #[test]
    fn noop_connector_produces_a_stageable_candidate() {
        let c = NoopConnector::with_sample();
        let items = c.scan(&ScanOptions::default()).unwrap();
        let mut budget = Budget::default();
        let (candidates, summary) =
            build_candidates(&c, &items, None, &mut budget, &EventSink::new(false));

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].destination_kind, "memory");
        assert_eq!(candidates[0].source_identity, "noop:sample:1");
        assert_eq!(summary.fallbacks, 1);
        assert_eq!(summary.failed, 0);
    }

    #[test]
    fn without_a_classifier_the_deterministic_fallback_is_used() {
        let c = NoopConnector::with_sample();
        let items = c.scan(&ScanOptions::default()).unwrap();
        let mut budget = Budget::default();
        let (candidates, summary) =
            build_candidates(&c, &items, None, &mut budget, &EventSink::new(false));
        assert_eq!(
            candidates.len(),
            1,
            "--no-llm must still produce candidates"
        );
        assert_eq!(summary.classified, 0);
    }

    /// Batches now run concurrently, so the one thing that can silently break is
    /// ordering: candidates must come back in item order regardless of which
    /// worker finished first, or a reviewer's queue no longer matches the
    /// documents it came from.
    ///
    /// Driven through a stub `claude` that endorses the first item of every
    /// batch, so the expected output is known exactly.
    #[test]
    fn parallel_batches_return_candidates_in_item_order() {
        let dir = std::env::temp_dir().join(format!("nm-bulk-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("claude-stub");
        // Endorses item 1 of whatever batch it is given. Sleeps a little on a
        // coin flip so batches genuinely finish out of order.
        std::fs::write(
            &bin,
            "#!/bin/sh\n\
             [ $(( $$ % 2 )) -eq 0 ] && sleep 0.2\n\
             printf '%s' '{\"result\":\"[{\\\"item\\\":1,\\\"content\\\":\\\"kept\\\"}]\"}'\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        }

        let c = NoopConnector {
            items: (0..12)
                .map(|i| SourceItem {
                    source_identity: format!("id-{i:02}"),
                    display_origin: format!("doc-{i:02}.md"),
                    routing_path: Some(format!("doc-{i:02}.md")),
                    raw: "body".to_string(),
                    meta: serde_json::json!({}),
                })
                .collect(),
        };
        let items = c.scan(&ScanOptions::default()).unwrap();
        let cli = ClaudeCli {
            bin: bin.to_string_lossy().to_string(),
            model: "stub".to_string(),
        };

        let mut budget = Budget::default();
        // One item per batch, so every item is the "item 1" the stub endorses.
        let (candidates, summary) = build_candidates_bulk(
            &c,
            &items,
            &cli,
            &mut budget,
            &EventSink::new(false),
            1,
            4,
        );

        assert_eq!(candidates.len(), 12, "every batch contributed its one item");
        assert_eq!(summary.classified, 12);
        assert_eq!(summary.passed, 0);
        let order: Vec<&str> = candidates.iter().map(|c| c.source_identity.as_str()).collect();
        let expected: Vec<String> = (0..12).map(|i| format!("id-{i:02}")).collect();
        assert_eq!(
            order,
            expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "concurrency must not reorder the queue"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_item_with_no_fallback_is_counted_as_failed_not_fatal() {
        let c = NoFallback;
        let items = c.scan(&ScanOptions::default()).unwrap();
        let mut budget = Budget::default();
        let (candidates, summary) =
            build_candidates(&c, &items, None, &mut budget, &EventSink::new(false));
        assert!(candidates.is_empty());
        assert_eq!(
            summary.failed, 2,
            "both items are reported, neither aborts the run"
        );
        assert_eq!(summary.scanned, 2);
    }

    // ── The CLI envelope contract ────────────────────────────────────────────

    fn envelope(result: serde_json::Value, usage: Option<serde_json::Value>) -> serde_json::Value {
        let mut e = serde_json::json!({ "type": "result", "result": result });
        if let Some(u) = usage {
            e["usage"] = u;
        }
        e
    }

    fn candidate_json() -> serde_json::Value {
        serde_json::json!({
            "source_identity": "repo-docs:docs/a.md:abc",
            "destination_kind": "convention",
            "content": "Always write the failing test first.",
            "source_excerpt": "the team always writes the failing test first",
            "confidence": 0.82
        })
    }

    #[test]
    fn classifier_parses_valid_envelope() {
        let e = envelope(
            serde_json::Value::String(candidate_json().to_string()),
            Some(serde_json::json!({ "input_tokens": 1200, "output_tokens": 300 })),
        );
        let c = parse_candidate(&e).unwrap();
        assert_eq!(c.destination_kind, "convention");
        assert_eq!(
            parse_usage(&e),
            Some(TokenUsage {
                input: 1200,
                output: 300
            })
        );
    }

    fn big_candidate(id: &str, kb: usize) -> CandidatePayload {
        CandidatePayload {
            source_identity: id.to_string(),
            destination_kind: "memory".into(),
            content: "x".repeat(kb * 1024),
            destination_hint: serde_json::json!({}),
            source_excerpt: None,
            confidence: None,
            provenance_kind: None,
        }
    }

    /// The reported failure: 248 candidates carrying their content made a 2.6 MB
    /// request, which the framework refused to buffer at all — `400 Failed to
    /// buffer the request body`, whole batch lost. The count was never the
    /// binding constraint; the bytes were.
    #[test]
    fn candidates_are_split_by_size_not_only_by_count() {
        let candidates: Vec<CandidatePayload> = (0..10)
            .map(|i| big_candidate(&format!("c{i}"), 300))
            .collect();
        let batches = chunk_candidates(candidates);
        assert!(batches.len() > 1, "3 MB cannot go in one request");
        for batch in &batches {
            let bytes: usize = batch
                .iter()
                .map(|c| serde_json::to_string(c).unwrap().len())
                .sum();
            assert!(
                bytes <= MAX_BATCH_BYTES + 400 * 1024,
                "a batch of {bytes} bytes is over the cap"
            );
        }
    }

    /// Chunking must never be a place where work quietly disappears.
    #[test]
    fn every_candidate_survives_chunking_exactly_once() {
        let candidates: Vec<CandidatePayload> = (0..500)
            .map(|i| big_candidate(&format!("c{i}"), 8))
            .collect();
        let expected: Vec<String> = candidates
            .iter()
            .map(|c| c.source_identity.clone())
            .collect();
        let seen: Vec<String> = chunk_candidates(candidates)
            .into_iter()
            .flatten()
            .map(|c| c.source_identity)
            .collect();
        assert_eq!(seen, expected, "same candidates, same order, none lost");
    }

    #[test]
    fn the_item_cap_still_applies_to_small_candidates() {
        let candidates: Vec<CandidatePayload> = (0..450)
            .map(|i| big_candidate(&format!("c{i}"), 0))
            .collect();
        let batches = chunk_candidates(candidates);
        assert!(batches.iter().all(|b| b.len() <= MAX_BATCH_ITEMS));
        assert_eq!(batches.len(), 3);
    }

    /// One candidate over the cap cannot be sent successfully, but dropping it
    /// here would mean nobody ever learns it exists. It goes alone and the
    /// server names it.
    #[test]
    fn an_oversized_candidate_is_sent_alone_rather_than_dropped() {
        let batches = chunk_candidates(vec![
            big_candidate("small", 1),
            big_candidate("huge", 2_000),
            big_candidate("also-small", 1),
        ]);
        let flat: Vec<&str> = batches
            .iter()
            .flatten()
            .map(|c| c.source_identity.as_str())
            .collect();
        assert_eq!(flat, vec!["small", "huge", "also-small"]);
        let huge = batches
            .iter()
            .find(|b| b.iter().any(|c| c.source_identity == "huge"))
            .unwrap();
        assert_eq!(huge.len(), 1, "it does not drag others down with it");
    }

    #[test]
    fn no_candidates_means_no_requests() {
        assert!(chunk_candidates(vec![]).is_empty());
    }

    /// `NEXUSMIND_BASE_URL=` is how you neutralise the production default for
    /// one command. It must read as "no backend", not as an unusable URL.
    #[test]
    fn a_blank_backend_url_reads_as_no_backend() {
        for raw in ["", "   "] {
            let cleaned = Some(raw).map(str::trim).filter(|u| !u.is_empty());
            assert_eq!(cleaned, None, "{raw:?} must not be treated as a URL");
        }
        assert_eq!(
            Some(" http://localhost:8080 ")
                .map(str::trim)
                .filter(|u| !u.is_empty()),
            Some("http://localhost:8080")
        );
    }

    #[test]
    fn a_json_array_is_extracted_from_around_prose_and_fences() {
        let raw = "Here you go:\n```json\n[{\"a\":1},{\"b\":[2,3]}]\n```\nDone.";
        let extracted = extract_json_array(raw).expect("an array is in there");
        assert_eq!(extracted, r#"[{"a":1},{"b":[2,3]}]"#);
        assert!(serde_json::from_str::<Vec<serde_json::Value>>(extracted).is_ok());
    }

    #[test]
    fn a_bracket_inside_a_string_does_not_end_the_array_early() {
        let raw = r#"[{"a": "not ] the end"}, {"b": 1}]"#;
        assert_eq!(extract_json_array(raw), Some(raw));
    }

    /// The deterministic drafts the pipeline always has alongside a batch.
    fn drafts_for(items: &[SourceItem]) -> Vec<Option<CandidatePayload>> {
        let c = NoopConnector { items: vec![] };
        items.iter().map(|i| c.fallback(i)).collect()
    }

    fn bulk_items(n: usize, size: usize) -> Vec<SourceItem> {
        (0..n)
            .map(|i| SourceItem {
                source_identity: format!("id-{i}"),
                display_origin: format!("doc-{i}.md"),
                routing_path: Some(format!("doc-{i}.md")),
                raw: "x".repeat(size),
                meta: serde_json::json!({}),
            })
            .collect()
    }

    #[test]
    fn batches_are_capped_by_count() {
        let items = bulk_items(45, 10);
        let batches = chunk_for_bulk(&items, 20);
        assert_eq!(batches.len(), 3);
        assert!(batches.iter().all(|r| r.len() <= 20));
        assert_eq!(batches.iter().map(|r| r.len()).sum::<usize>(), 45);
    }

    /// The byte cap is the one that matters: units carry their content, and
    /// twenty large documents is a prompt nobody should send.
    #[test]
    fn batches_are_capped_by_bytes_before_count() {
        // Sized off the budget, not off a number that was true when the budget
        // was 60 KB: this test exists to prove bytes close a batch before the
        // count does, and it must keep proving that when the budget moves.
        let items = bulk_items(10, BULK_MAX_BYTES / 4);
        let batches = chunk_for_bulk(&items, 20);
        assert!(
            batches.len() > 1,
            "{} bytes cannot ride in one prompt",
            BULK_MAX_BYTES * 10 / 4
        );
        for range in &batches {
            let bytes: usize = items[range.clone()].iter().map(|i| i.raw.len()).sum();
            assert!(
                bytes <= BULK_MAX_BYTES || range.len() == 1,
                "{range:?} carries {bytes} bytes"
            );
        }
    }

    /// An item larger than the budget still goes, alone. Dropping it here would
    /// lose it silently.
    #[test]
    fn an_oversized_item_travels_alone_rather_than_being_dropped() {
        let mut items = bulk_items(3, 100);
        items[1].raw = "x".repeat(BULK_MAX_BYTES * 2);
        let batches = chunk_for_bulk(&items, 20);
        let flat: Vec<usize> = batches.iter().flat_map(|r| r.clone()).collect();
        assert_eq!(flat, vec![0, 1, 2], "nothing is lost or reordered");
        assert!(batches.iter().any(|r| r.len() == 1 && r.start == 1));
    }

    /// A document's sections travel together. Judging "is this section a rule?"
    /// without the rest of its document in the same call is a worse question,
    /// and the batch limits are reached by many documents rather than by one.
    #[test]
    fn a_documents_sections_are_never_split_across_batches() {
        // Three documents of four sections each, with a count cap that would
        // cut straight through the middle of the second one.
        let mut items = Vec::new();
        for doc in 0..3 {
            for sec in 0..4 {
                items.push(SourceItem {
                    source_identity: format!("id-{doc}-{sec}"),
                    display_origin: format!("doc-{doc}.md > s{sec}"),
                    routing_path: Some(format!("doc-{doc}.md")),
                    raw: "x".repeat(50),
                    meta: serde_json::json!({}),
                });
            }
        }
        let batches = chunk_for_bulk(&items, 5);

        let flat: Vec<usize> = batches.iter().flat_map(|r| r.clone()).collect();
        assert_eq!(flat, (0..12).collect::<Vec<_>>(), "nothing lost or reordered");
        for range in &batches {
            let docs: std::collections::BTreeSet<&str> = items[range.clone()]
                .iter()
                .map(|i| i.routing_path.as_deref().unwrap())
                .collect();
            for doc in docs {
                let in_batch = items[range.clone()]
                    .iter()
                    .filter(|i| i.routing_path.as_deref() == Some(doc))
                    .count();
                let total = items
                    .iter()
                    .filter(|i| i.routing_path.as_deref() == Some(doc))
                    .count();
                assert_eq!(in_batch, total, "{doc} was split across batches");
            }
        }
    }

    /// The heart of the sparse contract: an item the model says nothing about is
    /// declined, and that is not a failure. If this ever degrades to `Unusable`,
    /// every healthy batch reports itself as broken and the cost win disappears
    /// behind a wall of false alarms.
    #[test]
    fn an_unmentioned_item_is_passed_over_and_is_not_a_failure() {
        let items = bulk_items(3, 20);
        let reply = vec![serde_json::json!({
            "item": 2,
            "destination_kind": "convention",
            "content": "the model's improved wording",
        })];
        let out = map_bulk_reply(&reply, &items, &drafts_for(&items));

        assert_eq!(out.len(), 3);
        assert_eq!(out[0], Err(BulkMiss::Passed));
        assert_eq!(out[2], Err(BulkMiss::Passed));
        let changed = out[1].as_ref().expect("item 2 was answered");
        assert_eq!(changed.content, "the model's improved wording");
        assert_eq!(
            changed.source_identity, items[1].source_identity,
            "identity is the connector's, never the model's"
        );
    }

    /// An empty reply means the model found nothing worth keeping in that batch.
    /// It must stage nothing — not fall back to drafts whose content is the raw
    /// section text, which is how a review queue fills with unread prose.
    #[test]
    fn an_empty_reply_puts_nothing_forward() {
        let items = bulk_items(4, 20);
        let out = map_bulk_reply(&[], &items, &drafts_for(&items));
        assert!(out.iter().all(|r| r == &Err(BulkMiss::Passed)));
    }

    /// The regression this pins, and the reason `Passed` exists apart from
    /// `Unusable`: a declined section must not be staged from its draft. The
    /// draft's content is `item.raw` — the chunk itself — so filling it in would
    /// migrate the document verbatim, one heading at a time, with nobody having
    /// vouched for any of it.
    #[test]
    fn a_declined_section_is_dropped_rather_than_staged_from_its_raw_draft() {
        let c = NoopConnector { items: vec![] };
        let items = bulk_items(3, 20);
        let drafts: Vec<Option<CandidatePayload>> = items.iter().map(|i| c.fallback(i)).collect();
        assert!(
            drafts.iter().all(|d| d.is_some()),
            "the premise: every unit does have a draft to fall back on"
        );

        let out = map_bulk_reply(&[], &items, &drafts_for(&items));
        let staged = out.iter().filter(|r| r.is_ok()).count();
        assert_eq!(staged, 0, "a batch nobody vouched for stages nothing");
        assert!(
            out.iter().all(|r| matches!(r, Err(BulkMiss::Passed))),
            "and it is recorded as a decline, not as a failure"
        );
    }

    /// A model that answers about an item outside the batch must not land on
    /// somebody else's unit. Dropping it leaves that item on its draft.
    #[test]
    fn an_out_of_range_item_number_is_dropped_not_clamped() {
        let items = bulk_items(2, 20);
        let reply = vec![
            serde_json::json!({"item": 99, "destination_kind": "memory", "content": "stray"}),
            serde_json::json!({"item": 0, "destination_kind": "memory", "content": "stray"}),
        ];
        let out = map_bulk_reply(&reply, &items, &drafts_for(&items));
        assert!(
            out.iter().all(|r| r == &Err(BulkMiss::Passed)),
            "a stray item number must not overwrite a real one"
        );
    }

    /// Measured against the real corpus: asked for distilled knowledge, the model
    /// answers with `item` and `content` and leaves the routing alone. That is
    /// the right instinct — the draft worked the destination out from the
    /// document's shape — but it used to be a deserialization failure, and every
    /// failure fell back to the draft, whose content is the raw section text.
    /// Seven good answers became seven raw chunks.
    #[test]
    fn an_answer_that_gives_only_content_inherits_its_routing_from_the_draft() {
        let items = bulk_items(2, 20);
        let drafts = drafts_for(&items);
        let reply = vec![serde_json::json!({
            "item": 1,
            "content": "pnpm is mandatory here; npm's hoisting breaks the two Tailwind majors.",
        })];
        let out = map_bulk_reply(&reply, &items, &drafts);

        let kept = out[0].as_ref().expect("an answer with only content is usable");
        assert_eq!(
            kept.content,
            "pnpm is mandatory here; npm's hoisting breaks the two Tailwind majors."
        );
        assert_eq!(
            kept.destination_kind,
            drafts[0].as_ref().unwrap().destination_kind,
            "the destination comes from the draft"
        );
        assert_eq!(
            kept.destination_hint.get("title"),
            drafts[0].as_ref().unwrap().destination_hint.get("title"),
            "and so does the title"
        );
        assert_eq!(out[1], Err(BulkMiss::Passed));
    }

    /// A genuinely broken object is `Unusable`, which is what the operator
    /// needs to see — separate from the silence that means assent.
    #[test]
    fn a_malformed_answer_is_unusable_rather_than_silent() {
        let items = bulk_items(2, 20);
        let reply = vec![
            serde_json::json!({"item": 1, "destination_kind": "memory", "content": "   "}),
            serde_json::json!({"item": 2, "nonsense": true}),
        ];
        let out = map_bulk_reply(&reply, &items, &drafts_for(&items));
        assert!(matches!(out[0], Err(BulkMiss::Unusable(_))), "{:?}", out[0]);
        assert!(matches!(out[1], Err(BulkMiss::Unusable(_))), "{:?}", out[1]);
    }

    #[test]
    fn no_items_means_no_batches() {
        assert!(chunk_for_bulk(&[], 20).is_empty());
    }

    /// The prompt has to establish three things, and each one has a failure it
    /// prevents: name the item number (or a sparse reply is unmappable), say
    /// that omission drops the item (or the model assumes silence is assent and
    /// says nothing about anything), and forbid copying the section back (or
    /// "content" comes back as the raw chunk the draft already held).
    #[test]
    fn the_bulk_prompt_asks_for_a_sparse_reply_and_carries_the_drafts() {
        let c = NoopConnector { items: vec![] };
        let items = bulk_items(3, 20);
        let drafts: Vec<Option<CandidatePayload>> = items.iter().map(|i| c.fallback(i)).collect();
        let prompt = bulk_prompt(&c, &items, &drafts);

        assert!(prompt.contains("ONLY the items worth migrating"), "{prompt}");
        assert!(prompt.contains("\"item\""), "the reply has to name the item number");
        assert!(
            prompt.contains("dropped and never reaches a human"),
            "omission must be spelled out as a decline; if the model reads it as \
             assent it stays silent and the raw drafts ship"
        );
        assert!(
            prompt.contains("Do not copy the section back"),
            "without this, content comes back as the chunk the draft already held"
        );
        assert!(
            prompt.contains("its content is the raw section text and is NOT an answer"),
            "the draft has to be introduced as a hint, not as a floor"
        );
        assert!(prompt.contains("=== ITEM 1 ==="));
        assert!(prompt.contains("=== ITEM 3 ==="));
        assert!(!prompt.contains("=== ITEM 4 ==="));
        assert_eq!(prompt.matches("DRAFT for this item").count(), 3);
    }

    #[test]
    fn a_fenced_answer_is_accepted() {
        // What the model actually sends when the prompt does not forbid it.
        let fenced = format!("```json\n{}\n```", candidate_json());
        let e = envelope(serde_json::Value::String(fenced), None);
        assert!(
            parse_candidate(&e).is_ok(),
            "a fenced object is still an object"
        );
    }

    #[test]
    fn prose_around_the_object_is_tolerated() {
        let s = format!(
            "Here is the classification:\n\n{}\n\nLet me know if you need changes.",
            candidate_json()
        );
        let e = envelope(serde_json::Value::String(s), None);
        assert!(parse_candidate(&e).is_ok());
    }

    #[test]
    fn a_brace_inside_a_string_does_not_end_the_object_early() {
        let raw = r#"prefix {"a": "not } the end", "b": {"c": 1}} suffix"#;
        assert_eq!(
            extract_json_object(raw),
            Some(r#"{"a": "not } the end", "b": {"c": 1}}"#)
        );
    }

    #[test]
    fn an_escaped_quote_does_not_end_the_string() {
        let raw = r#"{"a": "he said \"}\" loudly", "b": 1}"#;
        let extracted = extract_json_object(raw).expect("balanced");
        assert!(
            serde_json::from_str::<serde_json::Value>(extracted).is_ok(),
            "{extracted}"
        );
    }

    #[test]
    fn an_answer_with_no_object_at_all_is_reported_not_guessed() {
        let e = envelope(
            serde_json::Value::String("I cannot classify this file.".into()),
            None,
        );
        let err = parse_candidate(&e).unwrap_err().to_string();
        assert!(err.contains("no parseable candidate JSON"), "{err}");
        assert!(
            err.contains("I cannot classify"),
            "the answer must be quoted so the cause is visible: {err}"
        );
    }

    /// The reported failure: a run showed "0 tokens spent" while the weekly
    /// quota visibly moved, because usage was discarded on the error path — so
    /// `--max-tokens` could never trip and a broken classifier could burn an
    /// entire quota unchecked.
    #[test]
    fn tokens_are_counted_even_when_the_answer_cannot_be_used() {
        let e = envelope(
            serde_json::Value::String("sorry, no JSON here".into()),
            Some(serde_json::json!({ "input_tokens": 1200, "output_tokens": 300 })),
        );
        assert!(parse_candidate(&e).is_err(), "the answer is unusable");
        assert_eq!(
            parse_usage(&e),
            Some(TokenUsage {
                input: 1200,
                output: 300
            }),
            "but the spend is real and must still be visible"
        );
    }

    #[test]
    fn classifier_accepts_an_object_result_as_well_as_a_json_string() {
        let e = envelope(candidate_json(), None);
        assert_eq!(parse_candidate(&e).unwrap().destination_kind, "convention");
    }

    #[test]
    fn classifier_rejects_output_missing_required_fields() {
        let e = envelope(serde_json::json!({ "destination_kind": "memory" }), None);
        assert!(
            parse_candidate(&e).is_err(),
            "a candidate with no content is not a candidate"
        );

        let empty_content = envelope(
            serde_json::json!({
                "source_identity": "x",
                "destination_kind": "memory",
                "content": "   "
            }),
            None,
        );
        assert!(parse_candidate(&empty_content).is_err());
    }

    /// Losing the metric must not lose the candidate.
    #[test]
    fn usage_parse_failure_does_not_fail_the_item() {
        let e = envelope(candidate_json(), None);
        assert!(parse_usage(&e).is_none());
        assert!(parse_candidate(&e).is_ok());

        let renamed = envelope(
            candidate_json(),
            Some(serde_json::json!({ "prompt_tokens": 10, "completion_tokens": 5 })),
        );
        assert_eq!(
            parse_usage(&renamed),
            Some(TokenUsage {
                input: 0,
                output: 0
            }),
            "an unrecognized usage shape degrades to zero rather than erroring"
        );
        assert!(parse_candidate(&renamed).is_ok());
    }

    // ── Budget ───────────────────────────────────────────────────────────────

    #[test]
    fn budget_records_and_trips() {
        let mut b = Budget {
            max_tokens: Some(1000),
            spent: 0,
        };
        assert!(!b.would_exceed());
        b.record(Some(TokenUsage {
            input: 600,
            output: 500,
        }));
        assert_eq!(b.spent, 1100);
        assert!(b.would_exceed(), "the ceiling is hard, not advisory");
    }

    #[test]
    fn token_budget_exceeded_aborts_leaving_staged_intact() {
        // Three items, a budget already at its ceiling: the loop must stop
        // immediately and say so, rather than truncating silently.
        let c = NoopConnector {
            items: vec![item("a"), item("b"), item("c")],
        };
        let items = c.scan(&ScanOptions::default()).unwrap();
        let mut budget = Budget {
            max_tokens: Some(10),
            spent: 10,
        };
        let (candidates, summary) =
            build_candidates(&c, &items, None, &mut budget, &EventSink::new(false));
        assert!(candidates.is_empty());
        assert!(
            summary.aborted_on_budget,
            "the abort must be reported, not inferred"
        );
        assert_eq!(summary.scanned, 3);
    }

    /// The pool delivers items in whatever order they finish, and a budget abort
    /// leaves a hole where an item never ran. Reassembly must restore item order,
    /// drop nothing that produced a candidate, and count each fate once.
    #[test]
    fn parallel_results_reassemble_in_index_order_with_holes() {
        let cand = |id: &str| CandidatePayload {
            source_identity: id.into(),
            destination_kind: "memory".into(),
            content: "c".into(),
            destination_hint: serde_json::json!({}),
            source_excerpt: None,
            confidence: None,
            provenance_kind: None,
        };
        // Arrive out of order; index 1 is missing — its worker broke on the
        // budget before running it.
        let results = vec![
            (2, ItemOutcome::Fallback(cand("c2"))),
            (0, ItemOutcome::Classified(cand("c0"))),
            (3, ItemOutcome::Failed),
            (4, ItemOutcome::Classified(cand("c4"))),
        ];
        let (candidates, summary) = assemble_parallel(results, 5, true, 1234);

        assert_eq!(
            candidates
                .iter()
                .map(|c| c.source_identity.as_str())
                .collect::<Vec<_>>(),
            vec!["c0", "c2", "c4"],
            "output follows item index, not completion order; failed/skipped add nothing"
        );
        assert_eq!(summary.classified, 2);
        assert_eq!(summary.fallbacks, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(
            summary.scanned, 5,
            "scanned counts the whole source, holes included"
        );
        assert!(
            summary.aborted_on_budget,
            "the abort must survive reassembly"
        );
        assert_eq!(summary.tokens_spent, 1234);
    }

    #[test]
    fn no_budget_means_no_ceiling() {
        let mut b = Budget::default();
        b.record(Some(TokenUsage {
            input: 10_000_000,
            output: 0,
        }));
        assert!(!b.would_exceed());
    }

    /// The classifier must never inherit the operator's default model. On a
    /// coding machine that default is a frontier model, and this prompt runs
    /// once per unit — a large source then costs several times what the same
    /// run costs on Haiku, for a short structured judgement that does not
    /// benefit from the bigger model.
    #[test]
    fn the_classifier_defaults_to_haiku_rather_than_the_operators_model() {
        let args = Args::parse_from(["migrate-knowledge", "--source", "repo-docs"]);
        assert_eq!(args.model, "claude-haiku-4-5");
        // And it reaches the CLI: the flag is what stops the inheritance.
        let cli = ClaudeCli {
            bin: args.claude_bin.clone(),
            model: args.model.clone(),
        };
        assert_eq!(cli.model, "claude-haiku-4-5");
    }

    // ── The core change ships no real connector ──────────────────────────────

    fn args_for(source: &str) -> Args {
        Args {
            json: false,
            bulk: false,
            batch_size: BULK_MAX_ITEMS,
            parallel: 1,
            source: source.to_string(),
            path: ".".to_string(),
            config: None,
            require_config: false,
            api_url: None,
            api_key: None,
            client: None,
            project: None,
            dry_run: false,
            no_llm: false,
            max_tokens: None,
            claude_bin: "claude".to_string(),
            model: "claude-haiku-4-5".to_string(),
            include_sdd: false,
            host_scope: None,
            since_commit: None,
            include: vec![],
            exclude: vec![],
            dsn: None,
            supabase: false,
            include_data: false,
            tables: vec![],
            sample_limit: None,
            redact_pii: false,
            attest: None,
        }
    }

    /// `repo-docs` now exists. The other three still do not, and the refusal
    /// still has to say where they live rather than just failing.
    #[test]
    fn repo_docs_is_available_and_the_other_three_are_not() {
        for available in ["noop", "repo-docs", "claude-memories", "git-history", "source-code"] {
            assert!(
                connector_for(available, &args_for(available)).is_ok(),
                "`{available}` must be available"
            );
        }
        assert!(connector_for("notion", &args_for("notion")).is_err());
    }

    /// The flags that keep a first pass affordable have to actually reach the
    /// scan. They did not, once.
    #[test]
    fn include_and_exclude_reach_the_scan_options() {
        let mut args = args_for("repo-docs");
        args.path = "/tmp/x".to_string();
        args.include = vec!["docs/adr".to_string()];
        args.exclude = vec!["docs/marketing".to_string()];

        let opts = scan_options_for(&args);
        assert_eq!(opts.root, "/tmp/x");
        assert_eq!(opts.includes, vec!["docs/adr".to_string()]);
        assert_eq!(opts.excludes, vec!["docs/marketing".to_string()]);
    }

    #[test]
    fn no_filters_means_an_unfiltered_scan() {
        let opts = scan_options_for(&args_for("repo-docs"));
        assert!(opts.includes.is_empty() && opts.excludes.is_empty());
    }

    /// A DSN on the command line survives in shell history, in `ps`, and in
    /// anything that logs commands. The flag exists only so passing it can be
    /// refused with that explanation rather than silently accepted.
    #[test]
    fn a_dsn_passed_as_an_argument_is_refused() {
        let mut args = args_for("db-schema");
        args.dsn = Some("postgres://admin:hunter2@db.internal/prod".to_string());
        let msg = match connector_for("db-schema", &args) {
            Ok(_) => panic!("a DSN in argv must be refused"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("dsn_in_argv"), "{msg}");
        assert!(msg.contains("NEXUSMIND_SOURCE_DSN"));
        assert!(msg.contains("shell history"));
    }

    #[test]
    fn db_schema_without_a_dsn_says_where_to_put_it() {
        std::env::remove_var("NEXUSMIND_SOURCE_DSN");
        let msg = match connector_for("db-schema", &args_for("db-schema")) {
            Ok(_) => panic!("no DSN means no scan"),
            Err(e) => e.to_string(),
        };
        assert!(msg.contains("missing_dsn"), "{msg}");
        assert!(
            msg.contains("READ-ONLY"),
            "the refusal must say which role to use: {msg}"
        );
    }

    #[test]
    fn repo_docs_connector_reports_its_source_kind() {
        let c = connector_for("repo-docs", &args_for("repo-docs")).unwrap();
        assert_eq!(c.source_kind(), "repo-docs");
    }
}
