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
use std::process::Command;

// ── The connector contract ───────────────────────────────────────────────────
//
// Moved to `nexusmind::migration` so connectors can be tested with the rest of
// the library suite. Re-exported here so this binary reads as it did before.

pub use nexusmind::migration::{
    CandidatePayload, Connector, RepoDocsConnector, ScanOptions, SourceItem,
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

#[derive(Debug, Clone, PartialEq)]
pub struct ClassifierOutput {
    pub candidate: CandidatePayload,
    /// `None` when the envelope did not carry usage in a shape we recognize.
    /// Losing the metric is annoying; losing the candidate would be work lost.
    pub usage: Option<TokenUsage>,
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
        Some(serde_json::Value::String(s)) => serde_json::from_str(s)
            .context("the `result` field was a string but not valid candidate JSON")?,
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
        input: usage.get("input_tokens").and_then(|v| v.as_i64()).unwrap_or(0),
        output: usage
            .get("output_tokens")
            .and_then(|v| v.as_i64())
            .unwrap_or(0),
    })
}

pub struct ClaudeCli {
    pub bin: String,
}

impl ClaudeCli {
    /// Runs the CLI once. The identity is stamped onto the returned candidate so
    /// a model that paraphrases or omits it cannot break idempotency — provenance
    /// is the connector's to decide, never the classifier's.
    pub fn classify(&self, prompt: &str, item: &SourceItem) -> Result<ClassifierOutput> {
        let output = Command::new(&self.bin)
            .args(["-p", prompt, "--output-format", "json"])
            .output()
            .with_context(|| format!("could not run `{}`", self.bin))?;

        if !output.status.success() {
            anyhow::bail!(
                "{} exited with {}: {}",
                self.bin,
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let envelope: serde_json::Value = serde_json::from_slice(&output.stdout)
            .context("classifier output was not valid JSON")?;
        let mut candidate = parse_candidate(&envelope)?;
        candidate.source_identity = item.source_identity.clone();

        Ok(ClassifierOutput {
            candidate,
            usage: parse_usage(&envelope),
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
    pub aborted_on_budget: bool,
    pub tokens_spent: i64,
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
) -> (Vec<CandidatePayload>, RunSummary) {
    let mut candidates = Vec::new();
    let mut summary = RunSummary {
        scanned: items.len(),
        ..Default::default()
    };

    for item in items {
        if budget.would_exceed() {
            summary.aborted_on_budget = true;
            break;
        }

        match classifier {
            Some(cli) => match cli.classify(&connector.classify_prompt(item), item) {
                Ok(out) => {
                    budget.record(out.usage);
                    summary.classified += 1;
                    candidates.push(out.candidate);
                }
                Err(e) => {
                    tracing::warn!(
                        "classifier failed for {}: {e}; falling back",
                        item.display_origin
                    );
                    match connector.fallback(item) {
                        Some(c) => {
                            summary.fallbacks += 1;
                            candidates.push(c);
                        }
                        None => summary.failed += 1,
                    }
                }
            },
            None => match connector.fallback(item) {
                Some(c) => {
                    summary.fallbacks += 1;
                    candidates.push(c);
                }
                None => summary.failed += 1,
            },
        }
    }

    summary.tokens_spent = budget.spent;
    (candidates, summary)
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

    #[arg(long, default_value = ".")]
    path: String,

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

    /// Let `openspec/changes/**` produce `sdd_artifact` candidates.
    ///
    /// Off by default: in this repository `import-sdd` already backfilled them,
    /// and two paths to one destination is how duplicates happen. Turn it on
    /// when migrating a repo where that importer never ran.
    #[arg(long)]
    include_sdd: bool,
}

fn connector_for(source: &str, args: &Args) -> Result<Box<dyn Connector>> {
    match source {
        "noop" => Ok(Box::new(NoopConnector::with_sample())),
        "repo-docs" => Ok(Box::new(
            RepoDocsConnector::new(RepoDocsConnector::repo_name_for(&args.path))
                .with_sdd(args.include_sdd),
        )),
        "git-history" | "claude-memories" | "db-schema" => anyhow::bail!(
            "connector `{source}` ships with its own change and is not available yet. \
             Available: noop, repo-docs."
        ),
        other => anyhow::bail!("unknown source `{other}`"),
    }
}

fn run(args: &Args) -> Result<()> {
    let connector = connector_for(&args.source, args)?;
    let opts = ScanOptions {
        root: args.path.clone(),
        ..Default::default()
    };
    let report = connector.scan_report(&opts)?;

    if args.dry_run {
        println!(
            "dry run — source={} documents={} units={} bytes={} estimated_tokens≈{}",
            connector.source_kind(),
            report.documents,
            report.units,
            report.bytes,
            report.estimated_tokens(),
        );
        if !report.excluded.is_empty() {
            println!("excluded {} document(s):", report.excluded.len());
            for (path, reason) in &report.excluded {
                println!("  - {path} — {reason}");
            }
        }
        println!("no classification was run and nothing was posted.");
        return Ok(());
    }
    let items = report.items;

    let cli = (!args.no_llm).then(|| ClaudeCli {
        bin: args.claude_bin.clone(),
    });
    let mut budget = Budget {
        max_tokens: args.max_tokens,
        spent: 0,
    };
    let (candidates, summary) =
        build_candidates(connector.as_ref(), &items, cli.as_ref(), &mut budget);

    let Some(api_url) = args.api_url.as_deref() else {
        println!(
            "{} candidate(s) built; no --api-url given, so nothing was posted.",
            candidates.len()
        );
        println!("{summary:?}");
        return Ok(());
    };
    let api_key = args
        .api_key
        .as_deref()
        .context("--api-key (or NEXUSMIND_API_KEY) is required to post candidates")?;

    let http = reqwest::blocking::Client::new();
    let run_body = serde_json::json!({
        "source_kind": connector.source_kind(),
        "client_id": args.client,
        "project_id": args.project,
        "source_ref": args.path,
        "runner_version": cli.as_ref().and_then(|c| c.version()),
    });
    let created: serde_json::Value = http
        .post(format!("{api_url}/v1/migrations"))
        .bearer_auth(api_key)
        .json(&run_body)
        .send()?
        .error_for_status()?
        .json()?;
    let run_id = created
        .get("id")
        .and_then(|v| v.as_str())
        .context("backend did not return a run id")?;

    let staged: serde_json::Value = http
        .post(format!("{api_url}/v1/migrations/{run_id}/candidates"))
        .bearer_auth(api_key)
        .json(&serde_json::json!({ "candidates": candidates }))
        .send()?
        .error_for_status()?
        .json()?;

    println!("run {run_id}: {staged}");
    println!("{summary:?}");
    if summary.aborted_on_budget {
        println!(
            "token budget reached — the candidates already staged are intact and the run is \
             resumable."
        );
    }
    println!("nothing has been committed: every candidate awaits human review.");
    Ok(())
}

fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    if let Err(e) = run(&args) {
        eprintln!("✗ {e}");
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
        let (candidates, summary) = build_candidates(&c, &items, None, &mut budget);

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
        let (candidates, summary) = build_candidates(&c, &items, None, &mut budget);
        assert_eq!(candidates.len(), 1, "--no-llm must still produce candidates");
        assert_eq!(summary.classified, 0);
    }

    #[test]
    fn an_item_with_no_fallback_is_counted_as_failed_not_fatal() {
        let c = NoFallback;
        let items = c.scan(&ScanOptions::default()).unwrap();
        let mut budget = Budget::default();
        let (candidates, summary) = build_candidates(&c, &items, None, &mut budget);
        assert!(candidates.is_empty());
        assert_eq!(summary.failed, 2, "both items are reported, neither aborts the run");
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
            Some(TokenUsage { input: 1200, output: 300 })
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
        assert!(parse_candidate(&e).is_err(), "a candidate with no content is not a candidate");

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
            Some(TokenUsage { input: 0, output: 0 }),
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
        b.record(Some(TokenUsage { input: 600, output: 500 }));
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
        let (candidates, summary) = build_candidates(&c, &items, None, &mut budget);
        assert!(candidates.is_empty());
        assert!(summary.aborted_on_budget, "the abort must be reported, not inferred");
        assert_eq!(summary.scanned, 3);
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

    // ── The core change ships no real connector ──────────────────────────────

    fn args_for(source: &str) -> Args {
        Args {
            source: source.to_string(),
            path: ".".to_string(),
            api_url: None,
            api_key: None,
            client: None,
            project: None,
            dry_run: false,
            no_llm: false,
            max_tokens: None,
            claude_bin: "claude".to_string(),
            include_sdd: false,
        }
    }

    /// `repo-docs` now exists. The other three still do not, and the refusal
    /// still has to say where they live rather than just failing.
    #[test]
    fn repo_docs_is_available_and_the_other_three_are_not() {
        for available in ["noop", "repo-docs"] {
            assert!(
                connector_for(available, &args_for(available)).is_ok(),
                "`{available}` must be available"
            );
        }
        for pending in ["git-history", "claude-memories", "db-schema"] {
            match connector_for(pending, &args_for(pending)) {
                Ok(_) => panic!("`{pending}` has not shipped yet"),
                Err(e) => assert!(
                    e.to_string().contains("its own change"),
                    "the refusal must explain where the connector lives; got: {e}"
                ),
            }
        }
        assert!(connector_for("notion", &args_for("notion")).is_err());
    }

    #[test]
    fn repo_docs_connector_reports_its_source_kind() {
        let c = connector_for("repo-docs", &args_for("repo-docs")).unwrap();
        assert_eq!(c.source_kind(), "repo-docs");
    }
}
