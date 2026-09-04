//! Application state: the screens, the fields, and how a stream of runner
//! events becomes something worth looking at.
//!
//! Nothing here draws. `ui.rs` renders this state and never mutates it, which
//! is what lets every rule in `config.rs` be tested without a terminal.

use crate::api::{
    Candidate, Client, CodeIndexOutcome, CommitResponse, Project, ReviewResponse, Run, Verdict,
};
use crate::config::{Blocker, RunConfig, Source, Warning};
use crate::mascot::{self, Graphics, Mascot, Mood};
use crate::monorepo::{self, Action, Layout, PlanRow};
use crate::protocol::{ParsedLine, RunEvent};
use crate::runner::{resolve_binary, RunHandle, RunnerMsg};
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Connection,
    Source,
    Options,
    /// The monorepo plan: which sub-projects the path contains, and whether each
    /// is created, routed into an existing project, or skipped. Sits after
    /// Options because it needs the path Options collects.
    Projects,
    Running,
    Review,
    Summary,
}

impl Screen {
    /// The stage of the migration pipeline this screen belongs to, used to
    /// light up the diagram in the header.
    pub fn stage(self) -> usize {
        match self {
            Screen::Connection | Screen::Source | Screen::Options | Screen::Projects => 0,
            Screen::Running => 1,
            Screen::Review => 3,
            Screen::Summary => 4,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Screen::Connection => "Connection",
            Screen::Source => "Source",
            Screen::Options => "Options",
            Screen::Projects => "Projects",
            Screen::Running => "Run",
            Screen::Review => "Review",
            Screen::Summary => "Summary",
        }
    }
}

pub const STAGES: [&str; 5] = ["scan", "classify", "stage", "review", "commit"];

// ── Fields ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Secret,
    Toggle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldId {
    ApiUrl,
    ApiKey,
    Client,
    Project,
    Path,
    Includes,
    Excludes,
    IncludeSdd,
    ExtractKnowledge,
    IndexCode,
    HostScope,
    SinceCommit,
    Supabase,
    IncludeData,
    Tables,
    SampleLimit,
    RedactPii,
    Attest,
    NoLlm,
    MaxTokens,
    Parallel,
    Bulk,
    ClaudeBin,
    Model,
}

impl FieldId {
    pub fn kind(self) -> FieldKind {
        use FieldId::*;
        match self {
            ApiKey => FieldKind::Secret,
            IncludeSdd | ExtractKnowledge | IndexCode | HostScope | Supabase | IncludeData
            | RedactPii | NoLlm => FieldKind::Toggle,
            _ => FieldKind::Text,
        }
    }

    pub fn label(self) -> &'static str {
        use FieldId::*;
        match self {
            ApiUrl => "Backend URL",
            ApiKey => "API key",
            Client => "Client id",
            Project => "Project id",
            Path => "Path",
            Includes => "Include (comma separated)",
            Excludes => "Exclude (comma separated)",
            IncludeSdd => "Include SDD artifacts",
            ExtractKnowledge => "Extract knowledge (conventions, decisions)",
            IndexCode => "Index for vector/semantic search",
            HostScope => "Include host-level assets (~/.claude)",
            SinceCommit => "Since commit",
            Supabase => "Supabase conventions",
            IncludeData => "Read row samples (locked)",
            Tables => "Table allowlist",
            SampleLimit => "Rows per table",
            RedactPii => "Redact PII locally",
            Attest => "Authorisation attestation",
            NoLlm => "Skip the LLM (deterministic only)",
            MaxTokens => "Token budget",
            Parallel => "Parallel classifier calls",
            Bulk => "Batch units per call",
            ClaudeBin => "claude binary",
            Model => "Classifier model",
        }
    }

    pub fn help(self) -> &'static str {
        use FieldId::*;
        match self {
            ApiUrl => "Where candidates are staged. Local unless you say otherwise.",
            ApiKey => "Read from NEXUSMIND_API_KEY when set. Never shown in full.",
            Client => "Scopes everything this run stages to one client.",
            Project => "Optional. Narrows the scope further, inside the client.",
            Path => "The repository or directory to scan.",
            Includes => {
                "Restrict the scan to these subpaths. The cheapest way to \
                         make a first run small."
            }
            Excludes => "Skip these subpaths, on top of the connector's own rules.",
            IncludeSdd => "Proposals, designs and task lists under openspec/.",
            ExtractKnowledge => "Reads each code file with Claude and proposes the conventions \
                                 and decisions in it for review. The reason this source exists.",
            IndexCode => "Vectorises the codebase via /v1/code so it is searchable. A separate \
                          backend action; runs on a real run, not a preview.",
            HostScope => "Your personal Claude configuration, not just the repo's.",
            SinceCommit => "Only history after this commit. Empty means the whole history.",
            Supabase => "Read Supabase-specific conventions alongside the schema.",
            IncludeData => "Off by default. Turning it on requires four more answers.",
            Tables => "Exactly which tables may be sampled. There is no 'all'.",
            SampleLimit => "A hard, deterministic row cap per table.",
            RedactPii => "Runs in this process, before any sample leaves the machine.",
            Attest => "Who authorised this, under which agreement. Recorded on the run.",
            NoLlm => "Uses each connector's deterministic fallback. Costs nothing.",
            MaxTokens => "Stops the run cleanly when reached. Staged work survives.",
            Parallel => "How many calls run at once — batches too, when batching is \
                         on. Blank uses the runner's default (4); 1 is serial. It cuts \
                         time, not tokens — and much higher risks rate-limiting.",
            Bulk => "Classify many units in one call instead of one call each. A call \
                     costs ~14k tokens of context before it reads anything, so this is \
                     what decides whether a large source takes minutes or hours. Leave \
                     it on unless you are debugging one unit.",
            ClaudeBin => "The headless classifier invoked as `claude -p`.",
            Model => "Haiku by default — this prompt runs once per unit, so a frontier \
                      model multiplies the bill without improving the answer.",
        }
    }

    pub fn value(self, c: &RunConfig) -> String {
        use FieldId::*;
        match self {
            ApiUrl => c.api_url.clone(),
            ApiKey => c.api_key.clone(),
            Client => c.client.clone(),
            Project => c.project.clone(),
            Path => c.path.clone(),
            Includes => c.includes.clone(),
            Excludes => c.excludes.clone(),
            SinceCommit => c.since_commit.clone(),
            Tables => c.tables.clone(),
            SampleLimit => c.sample_limit.clone(),
            Attest => c.attest.clone(),
            MaxTokens => c.max_tokens.clone(),
            Parallel => c.parallel.clone(),
            Bulk => c.bulk.to_string(),
            ClaudeBin => c.claude_bin.clone(),
            Model => c.model.clone(),
            IncludeSdd => c.include_sdd.to_string(),
            ExtractKnowledge => c.extract_knowledge.to_string(),
            IndexCode => c.index_code.to_string(),
            HostScope => c.host_scope.to_string(),
            Supabase => c.supabase.to_string(),
            IncludeData => c.include_data.to_string(),
            RedactPii => c.redact_pii.to_string(),
            NoLlm => c.no_llm.to_string(),
        }
    }

    pub fn is_on(self, c: &RunConfig) -> bool {
        self.value(c) == "true"
    }

    fn text_mut(self, c: &mut RunConfig) -> Option<&mut String> {
        use FieldId::*;
        Some(match self {
            ApiUrl => &mut c.api_url,
            ApiKey => &mut c.api_key,
            Client => &mut c.client,
            Project => &mut c.project,
            Path => &mut c.path,
            Includes => &mut c.includes,
            Excludes => &mut c.excludes,
            SinceCommit => &mut c.since_commit,
            Tables => &mut c.tables,
            SampleLimit => &mut c.sample_limit,
            Attest => &mut c.attest,
            MaxTokens => &mut c.max_tokens,
            Parallel => &mut c.parallel,
            ClaudeBin => &mut c.claude_bin,
            Model => &mut c.model,
            _ => return None,
        })
    }

    pub fn push_char(self, c: &mut RunConfig, ch: char) {
        if let Some(s) = self.text_mut(c) {
            s.push(ch);
        }
    }

    pub fn pop_char(self, c: &mut RunConfig) {
        if let Some(s) = self.text_mut(c) {
            s.pop();
        }
    }

    pub fn clear(self, c: &mut RunConfig) {
        if let Some(s) = self.text_mut(c) {
            s.clear();
        }
    }

    pub fn toggle(self, c: &mut RunConfig) {
        use FieldId::*;
        match self {
            IncludeSdd => c.include_sdd = !c.include_sdd,
            ExtractKnowledge => c.extract_knowledge = !c.extract_knowledge,
            IndexCode => c.index_code = !c.index_code,
            HostScope => c.host_scope = !c.host_scope,
            Supabase => c.supabase = !c.supabase,
            RedactPii => c.redact_pii = !c.redact_pii,
            NoLlm => c.no_llm = !c.no_llm,
            Bulk => c.bulk = !c.bulk,
            IncludeData => {
                c.include_data = !c.include_data;
                // Turning sampling back off clears the answers that only exist
                // to unlock it. Leaving a stale attestation behind would mean a
                // later run could inherit someone's authorisation for a
                // decision they never made.
                if !c.include_data {
                    c.tables.clear();
                    c.sample_limit.clear();
                    c.redact_pii = false;
                    c.attest.clear();
                }
            }
            _ => {}
        }
    }
}

/// The fields a screen shows, in order.
pub fn fields_for(screen: Screen, source: Source) -> Vec<FieldId> {
    use FieldId::*;
    match screen {
        Screen::Connection => vec![ApiUrl, ApiKey, Client, Project],
        Screen::Options => {
            let mut f = Vec::new();
            if source.takes_path() {
                f.push(Path);
            }
            match source {
                Source::RepoDocs => f.extend([Includes, Excludes, IncludeSdd]),
                Source::ClaudeMemories => f.extend([Includes, Excludes, HostScope]),
                Source::GitHistory => f.extend([SinceCommit, Includes, Excludes]),
                Source::Code => {
                    f.extend([ExtractKnowledge, IndexCode, Includes, Excludes])
                }
                Source::DbSchema => {
                    f.push(Supabase);
                    f.push(IncludeData);
                    f.extend([Tables, SampleLimit, RedactPii, Attest]);
                }
            }
            f.extend([NoLlm, Bulk, MaxTokens, Parallel, ClaudeBin, Model]);
            f
        }
        _ => Vec::new(),
    }
}

/// Whether a field is currently meaningful.
///
/// The four sampling gates stay visible when sampling is off — greyed, not
/// hidden — so an operator can see the price of unlocking it before they do.
pub fn is_active(field: FieldId, c: &RunConfig) -> bool {
    use FieldId::*;
    match field {
        Tables | SampleLimit | RedactPii | Attest => c.include_data,
        MaxTokens | ClaudeBin | Model | Parallel | Bulk => !c.no_llm,
        _ => true,
    }
}

// ── Progress ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct Progress {
    pub source: String,
    pub dry_run: bool,
    pub documents: usize,
    pub units: usize,
    pub bytes: usize,
    pub estimated_tokens: usize,
    /// Skipped sources grouped by reason: `(reason, count, one sample path)`.
    /// Grouped on the wire because a scan can skip tens of thousands of files.
    pub excluded: Vec<(String, usize, String)>,
    pub current: usize,
    pub total: usize,
    pub current_origin: String,
    /// Sources examined by the scan so far. Distinct from `current`, which
    /// counts classified units: during the scan there are no units yet.
    pub scanning_seen: usize,
    pub by_destination: BTreeMap<String, u64>,
    pub classified: usize,
    pub fallbacks: usize,
    pub failed: usize,
    pub tokens: i64,
    pub run_id: Option<String>,
    /// Every run this session created, one per routed project. In a monorepo run
    /// the runner emits a `RunCreated` per group, so the single `run_id` above —
    /// the last one seen — is not enough to review them all.
    pub created_runs: Vec<CreatedRun>,
    pub staged: Option<(usize, usize, usize)>,
    pub finished: Option<FinishedRun>,
    pub unknown_events: usize,
    pub log: VecDeque<String>,
    /// Recent exchanges with the model. Bounded like the log: a 3000-unit run
    /// would otherwise hold every prompt it ever sent.
    pub agents: VecDeque<AgentExchange>,
}

/// One repository still to migrate in a folder-of-repos plan.
///
/// That layout has no shared repository to host a routing config, so it cannot
/// be one run: each repository is its own scan root with its own `--project`,
/// and they go through the runner one after another.
#[derive(Debug, Clone)]
pub struct QueuedRun {
    pub alias: String,
    pub path: String,
    pub project_id: String,
}

/// A run the runner created for one routed project, collected so the review
/// screen can walk every project's queue, not just the last.
#[derive(Debug, Clone)]
pub struct CreatedRun {
    pub alias: String,
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone)]
pub struct FinishedRun {
    pub ok: bool,
    pub aborted_on_budget: bool,
    pub error: Option<String>,
}

impl Progress {
    /// The share of the token budget already spent, if a budget was set.
    pub fn budget_ratio(&self, max_tokens: Option<i64>) -> Option<f64> {
        let max = max_tokens.filter(|m| *m > 0)?;
        Some((self.tokens as f64 / max as f64).clamp(0.0, 1.0))
    }

    pub fn classify_ratio(&self) -> f64 {
        if self.total == 0 {
            return 0.0;
        }
        (self.current as f64 / self.total as f64).clamp(0.0, 1.0)
    }

    /// How many sources were skipped in total.
    pub fn excluded_total(&self) -> usize {
        self.excluded.iter().map(|(_, n, _)| n).sum()
    }

    /// Exclusions by reason, largest first — the shape a bar chart needs, and
    /// the answer to "why did it only find 249 units?".
    pub fn exclusion_histogram(&self) -> Vec<(String, u64)> {
        let mut v: Vec<(String, u64)> = self
            .excluded
            .iter()
            .map(|(reason, n, _)| (reason.clone(), *n as u64))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    pub fn destination_histogram(&self) -> Vec<(String, u64)> {
        let mut v: Vec<(String, u64)> = self
            .by_destination
            .iter()
            .map(|(k, n)| (k.clone(), *n))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        v
    }

    fn record_agent(&mut self, exchange: AgentExchange) {
        self.agents.push_back(exchange);
        while self.agents.len() > 100 {
            self.agents.pop_front();
        }
    }

    fn note(&mut self, line: String) {
        self.log.push_back(line);
        while self.log.len() > 200 {
            self.log.pop_front();
        }
    }

    pub fn apply(&mut self, msg: RunnerMsg) {
        match msg {
            RunnerMsg::Line(ParsedLine::Event(e)) => self.apply_event(e),
            RunnerMsg::Line(ParsedLine::Unknown(name)) => {
                self.unknown_events += 1;
                self.note(format!("· {name} (newer runner than this TUI)"));
            }
            RunnerMsg::Line(ParsedLine::Noise(text)) => self.note(format!("? {text}")),
            RunnerMsg::Log(text) => self.note(text),
            RunnerMsg::Failed(e) => {
                self.note(format!("✗ {e}"));
                self.finished = Some(FinishedRun {
                    ok: false,
                    aborted_on_budget: false,
                    error: Some(e),
                });
            }
            RunnerMsg::Exited { code } => {
                // A non-zero exit with no `finished` event means the runner died
                // in a way it could not report. Recording it as a failure is the
                // difference between "it stopped" and a screen frozen at 40%.
                if self.finished.is_none() {
                    let mut reason = match code {
                        Some(c) => format!("the runner exited with status {c}"),
                        None => "the runner was terminated".to_string(),
                    };
                    // Not one event, not even `started`: the runner rejected
                    // the invocation before it began. Almost always an older
                    // binary than this TUI. Naming it saves a long hunt.
                    if self.total == 0 && self.source.is_empty() {
                        reason.push_str(
                            " before emitting anything — check that it understands --json                              (cargo build --bin migrate-knowledge), and see Activity for its                              own message",
                        );
                    }
                    self.finished = Some(FinishedRun {
                        ok: false,
                        aborted_on_budget: false,
                        error: Some(reason),
                    });
                }
            }
        }
    }

    fn apply_event(&mut self, e: RunEvent) {
        match e {
            RunEvent::Started {
                source,
                dry_run,
                path,
                ..
            } => {
                self.source = source;
                self.dry_run = dry_run;
                self.note(format!("▶ scanning {path}"));
            }
            RunEvent::Scanned {
                documents,
                units,
                bytes,
                estimated_tokens,
                ..
            } => {
                self.documents = documents;
                self.units = units;
                self.bytes = bytes;
                self.estimated_tokens = estimated_tokens;
                self.total = units;
                self.note(format!(
                    "· scanned {documents} document(s) → {units} unit(s)"
                ));
            }
            RunEvent::ConfigLoaded {
                repository_id,
                path,
                project_count,
                ..
            } => {
                self.note(format!(
                    "· config {path} for {repository_id} ({project_count} project(s))"
                ));
            }
            RunEvent::RoutingGroup {
                alias, item_count, ..
            } => {
                self.note(format!("· routed {item_count} item(s) to {alias}"));
            }
            RunEvent::RoutingIssue {
                kind,
                count,
                sample,
            } => {
                self.note(format!(
                    "! routing {kind}: {count} item(s){}",
                    sample.map(|s| format!("; e.g. {s}")).unwrap_or_default()
                ));
            }
            RunEvent::RoutingReady {
                groups,
                mapped_items,
                unmapped_items,
            } => {
                self.note(format!("· routing ready: {groups} group(s), {mapped_items} mapped, {unmapped_items} unmapped"));
            }
            RunEvent::RunCreated {
                alias,
                run_id,
                project_id,
            } => {
                self.note(format!("· created run {run_id} for {alias}"));
                self.created_runs.push(CreatedRun {
                    alias,
                    project_id,
                    run_id: run_id.clone(),
                });
                self.run_id = Some(run_id);
            }
            RunEvent::Agent {
                index,
                total,
                origin,
                prompt,
                response,
                ok,
                error,
                tokens_spent,
                duration_ms,
            } => self.record_agent(AgentExchange {
                index,
                total,
                origin,
                prompt,
                response,
                ok,
                error,
                tokens_spent,
                duration_ms,
            }),
            RunEvent::Scanning { seen, current } => {
                self.scanning_seen = seen;
                self.current_origin = current;
            }
            RunEvent::Excluded {
                reason,
                count,
                sample,
            } => self.excluded.push((reason, count, sample)),
            // With a pool of workers these events interleave: unit 4 finishes
            // while unit 2 is still starting. Assigning `current` from the
            // event's index made the bar walk backwards and the origin line
            // flicker between workers. `current` is a count of what is done, so
            // it is counted; the origin shows the most recent start, which is
            // the honest answer to "what is it working on" when the answer is
            // several things at once.
            RunEvent::Classifying { total, origin, .. } => {
                self.total = total;
                self.current_origin = origin;
            }
            RunEvent::Classified {
                total,
                destination_kind,
                via,
                tokens_spent,
                ..
            } => {
                self.current += 1;
                self.total = total;
                self.tokens += tokens_spent;
                match via.as_str() {
                    "classified" => self.classified += 1,
                    "fallback" => self.fallbacks += 1,
                    _ => self.failed += 1,
                }
                if !destination_kind.is_empty() {
                    *self.by_destination.entry(destination_kind).or_default() += 1;
                }
            }
            RunEvent::Staged {
                run_id,
                staged,
                skipped,
                rejected,
            } => {
                self.note(format!("· staged {staged} into run {run_id}"));
                self.run_id = Some(run_id);
                self.staged = Some((staged, skipped, rejected));
            }
            RunEvent::Finished {
                ok,
                classified,
                fallbacks,
                failed,
                tokens_spent,
                aborted_on_budget,
                error,
                ..
            } => {
                // The runner's own totals win: they are authoritative, and a
                // dropped line would otherwise leave the summary quietly wrong.
                if ok {
                    self.classified = classified;
                    self.fallbacks = fallbacks;
                    self.failed = failed;
                    self.tokens = tokens_spent;
                }
                self.finished = Some(FinishedRun {
                    ok,
                    aborted_on_budget,
                    error,
                });
            }
        }
    }
}

// ── The app ──────────────────────────────────────────────────────────────────

/// One exchange with the classifier, as shown in the Agents panel.
#[derive(Debug, Clone)]
pub struct AgentExchange {
    pub index: usize,
    pub total: usize,
    pub origin: String,
    pub prompt: String,
    pub response: String,
    pub ok: bool,
    pub error: Option<String>,
    pub tokens_spent: i64,
    pub duration_ms: u64,
}

/// Which half of the activity area is on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityView {
    Both,
    Agents,
    Logs,
}

impl ActivityView {
    pub fn next(self) -> Self {
        match self {
            ActivityView::Both => ActivityView::Agents,
            ActivityView::Agents => ActivityView::Logs,
            ActivityView::Logs => ActivityView::Both,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ActivityView::Both => "both",
            ActivityView::Agents => "agents only",
            ActivityView::Logs => "logs only",
        }
    }
}

/// The reply from a backend call.
///
/// # Why every call goes through a channel
///
/// These were once plain blocking calls on the draw thread. Opening a queue of
/// five hundred candidates — each carrying its full content — froze the whole
/// terminal: no spinner, no keys, no way to cancel, for as long as the request
/// took, and up to the 120-second timeout if the backend was unreachable. It
/// was indistinguishable from a crash, and every approval reloaded the queue,
/// so it happened on every keystroke that mattered.
#[derive(Debug)]
pub enum ApiMsg {
    Runs(Result<Vec<Run>, String>),
    Candidates(Result<Vec<Candidate>, String>),
    Reviewed {
        result: Result<ReviewResponse, String>,
        verdict: Verdict,
    },
    Committed(Result<CommitResponse, String>),
    Probed(Result<String, String>),
    Cancelled(Result<usize, String>),
    /// The backend projects fetched to match the monorepo plan against.
    Projects(Result<Vec<Project>, String>),
    /// The plan executed: projects created, ids resolved, `.nexusmind.yaml`
    /// written. On success the caller starts the routed run.
    PlanExecuted(Result<ExecutedPlan, String>),
    /// The source-code index action finished (or failed).
    Indexed(Result<CodeIndexOutcome, String>),
    /// One candidate rewritten in place. Carries the candidate back so the row
    /// on screen shows the edited text without a queue reload — a reload would
    /// re-sort the queue by confidence and move the cursor off what was just
    /// edited, which is exactly the wrong moment to lose your place.
    Edited(Result<Candidate, String>),
    Failed(String),
}

/// The result of confirming a monorepo plan: every non-skipped row resolved to
/// a project id, and the path of the config the runner will route from.
#[derive(Debug)]
pub struct ExecutedPlan {
    pub rows: Vec<PlanRow>,
    pub config_path: String,
    pub dry_run: bool,
}

/// What the summary screen is summarising.
#[derive(Debug, Clone)]
pub enum LastCommand {
    Preview,
    Run,
    Review { applied: usize, conflicts: usize },
    Commit(CommitResponse),
}

pub struct App {
    pub screen: Screen,
    pub config: RunConfig,
    pub cursor: usize,
    pub editing: bool,
    /// The focused field still holds the value it had when editing began.
    ///
    /// Exists because of a real failure: `Path` is pre-filled with `.`, typing
    /// appended, and an absolute path became `./Users/cesar/…` — which the
    /// runner then reported as "not a git repository" for a directory that
    /// plainly is one. A pre-filled field that silently concatenates is a trap;
    /// the first character typed now replaces it, as it would in any form.
    pub edit_pristine: bool,
    pub progress: Progress,
    pub handle: Option<RunHandle>,
    /// When the current run began, for the elapsed clock. A scan is one
    /// blocking call, and a clock that keeps moving is what distinguishes slow
    /// from hung when there is nothing else to show.
    pub started_at: Option<std::time::Instant>,
    /// Advances once per draw, so animations do not need wall-clock time.
    pub frame: u64,
    /// Wall clock since start, for animation. A frame counter would speed the
    /// mascot up whenever the draw loop got busy.
    boot: std::time::Instant,
    /// Whether the mascot may be drawn. Decided once at start-up from what the
    /// terminal can do and what the operator asked for, then only ever turned
    /// off — see `mascot.rs`.
    pub mascot_on: bool,
    /// The terminal's own image protocol, when it has one. `None` means the
    /// quadrant fallback, which is the ordinary case.
    pub graphics: Option<Graphics>,
    pub last_command: Option<LastCommand>,
    pub status: String,
    pub candidates: Vec<Candidate>,
    pub review_cursor: usize,
    pub selected: Vec<String>,
    /// Recent runs, for reviewing work staged before this session started.
    pub runs: Vec<Run>,
    pub run_cursor: usize,

    // ── Monorepo plan ─────────────────────────────────────────────────────────
    /// The sub-projects detected under the scan path, each with its decision.
    /// Empty means either "not detected yet" or "not a monorepo"; `detected`
    /// distinguishes them.
    pub plan: Vec<PlanRow>,
    pub plan_cursor: usize,
    /// The path detection last ran for, so re-entering the screen keeps the
    /// operator's edits instead of re-detecting and resetting them.
    pub plan_path: String,
    /// True once detection has run for the current path, so an empty `plan` can
    /// be shown as "single project" rather than "not looked yet".
    pub plan_detected: bool,
    /// What the scan root turned out to be. Decides how the plan executes: one
    /// routed run for a monorepo, one run per repository for a folder of them.
    pub plan_layout: Layout,
    /// A one-line note from detection: how many sub-projects, or why none.
    pub plan_note: String,
    /// The backend projects fetched for the run's client, matched by name.
    pub existing_projects: Vec<Project>,
    /// When set, the plan row whose existing-project target is being picked, and
    /// the cursor into `existing_projects` for that picker.
    pub selecting_for: Option<usize>,
    pub select_cursor: usize,
    /// A `.nexusmind.yaml` already present at the scan root — writing overwrites
    /// it, which the plan screen warns about before the operator confirms.
    pub existing_config: bool,
    /// Repositories still to migrate, for a folder-of-repos plan. Empty for a
    /// monorepo, which is a single routed run.
    pub run_queue: Vec<QueuedRun>,
    /// Which entry of `run_queue` is running now.
    pub queue_pos: usize,
    /// `(path, project)` as they were before the queue took over, restored when
    /// it drains so the operator's own configuration is not left pointing at
    /// the last repository the queue happened to visit.
    queue_restore: Option<(String, String)>,
    /// Every run this session created, across every invocation of the runner.
    /// `start` resets `progress` between queued runs, so the review's list of
    /// runs cannot live inside it.
    pub session_runs: Vec<CreatedRun>,
    /// The reply channel of the API call currently in flight, if any.
    api_rx: Option<std::sync::mpsc::Receiver<ApiMsg>>,
    /// What that call is, for the status line. `None` means idle.
    pub pending: Option<String>,
    /// A run chosen from the picker, which overrides the one this session
    /// produced. `None` falls back to the run in flight.
    pub picked_run: Option<String>,
    /// Candidates whose version moved under us on the last review call.
    pub conflicts: Vec<String>,
    pub activity: ActivityView,
    /// Which exchange the Agents panel is showing. `None` follows the newest,
    /// which is what you want while a run is moving; pressing ↑ pins it, which
    /// is what you want the moment something looks wrong.
    pub agent_cursor: Option<usize>,
    pub show_help: bool,
    /// The (url, key) pair the last probe was fired for.
    ///
    /// Finishing an edit on either connection field probes, so that reaching the
    /// backend — and pulling its run history — is something the operator gets by
    /// typing rather than by knowing a keystroke. This remembers what was
    /// already tried so tabbing back through an unchanged field does not fire a
    /// request every time.
    last_probed: Option<(String, String)>,
    pub should_quit: bool,
    binary: std::path::PathBuf,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    pub fn new() -> Self {
        Self {
            screen: Screen::Connection,
            config: RunConfig::default(),
            cursor: 0,
            editing: false,
            edit_pristine: false,
            progress: Progress::default(),
            handle: None,
            started_at: None,
            frame: 0,
            boot: std::time::Instant::now(),
            mascot_on: Mascot::compiled_in()
                && mascot::terminal_supports()
                && !mascot::disabled_by_operator(),
            // Filled in by `main` before the alternate screen goes up: the
            // query writes to the terminal and reads its reply, which cannot
            // happen once the UI owns the screen.
            graphics: None,
            last_command: None,
            status: "Tab moves between screens · ? for help · q to quit".into(),
            candidates: Vec::new(),
            review_cursor: 0,
            selected: Vec::new(),
            runs: Vec::new(),
            run_cursor: 0,
            plan: Vec::new(),
            plan_cursor: 0,
            plan_path: String::new(),
            plan_detected: false,
            plan_layout: Layout::Monorepo,
            plan_note: String::new(),
            existing_projects: Vec::new(),
            selecting_for: None,
            select_cursor: 0,
            existing_config: false,
            run_queue: Vec::new(),
            queue_pos: 0,
            queue_restore: None,
            session_runs: Vec::new(),
            api_rx: None,
            pending: None,
            picked_run: None,
            conflicts: Vec::new(),
            activity: ActivityView::Both,
            agent_cursor: None,
            show_help: false,
            last_probed: None,
            should_quit: false,
            binary: resolve_binary(),
        }
    }

    /// What the mascot should be doing, derived from state that is already on
    /// screen in words and numbers. It reads that state; it never sets it.
    ///
    /// Three moods, not one per stage. The robot cycles every action animation
    /// while anything is running rather than miming the current step: it is
    /// decoration, so a set that does not match the stage costs nothing, and
    /// tying each stage to one short loop meant most of the sheet was never
    /// seen.
    pub fn mood(&self) -> Mood {
        if matches!(self.last_command, Some(LastCommand::Commit(_))) {
            return Mood::Celebrating;
        }
        if self.in_flight() {
            return Mood::Working;
        }
        match &self.progress.finished {
            Some(f) if f.ok && self.progress.staged.is_some() => Mood::Celebrating,
            _ => Mood::Resting,
        }
    }

    /// How the mascot is being drawn, for the help screen.
    pub fn mascot_backend(&self) -> &'static str {
        match (&self.graphics, self.mascot_on) {
            (_, false) => "off",
            (Some(g), _) => g.name(),
            (None, _) => "quadrants",
        }
    }

    /// The frame to draw in a box of `w` x `h` cells, or `None` — which every
    /// caller must treat as "lay out as though this feature did not exist".
    pub fn mascot(&self, w: u16, h: u16) -> Option<Mascot> {
        if !self.mascot_on {
            return None;
        }
        Mascot::for_state(self.mood(), self.boot.elapsed().as_millis() as u64, w, h)
    }

    /// The operator's toggle. It can only ever turn the mascot off and back on
    /// within what the terminal supports; it cannot force it on where drawing
    /// would corrupt the panel.
    pub fn toggle_mascot(&mut self) {
        if !Mascot::compiled_in() || !mascot::terminal_supports() {
            return;
        }
        self.mascot_on = !self.mascot_on;
    }

    pub fn binary(&self) -> &std::path::PathBuf {
        &self.binary
    }

    /// Seconds since the run began, frozen once it ends.
    pub fn elapsed(&self) -> Option<std::time::Duration> {
        self.started_at.map(|t| t.elapsed())
    }

    /// True when the Run screen has nothing to show because nothing was ever
    /// started. Reachable by Tab, so it is a normal state, not an error — and
    /// it must not be dressed up as a scan in progress.
    pub fn never_ran(&self) -> bool {
        self.handle.is_none() && self.progress.finished.is_none() && self.last_command.is_none()
    }

    /// A child process exists and has not reported an end. Governs the stop
    /// key, which only means something when there is something to kill.
    pub fn is_running(&self) -> bool {
        self.handle.is_some() && self.progress.finished.is_none()
    }

    /// A run was started and has not ended.
    ///
    /// Distinct from `is_running`, which asks about the *process*. Everything
    /// the operator sees — the spinner, the clock, whether a quiet Activity
    /// pane is normal — should ask about the *run*, because that is the thing
    /// they are waiting on.
    pub fn in_flight(&self) -> bool {
        self.started_at.is_some() && self.progress.finished.is_none()
    }

    pub fn fields(&self) -> Vec<FieldId> {
        fields_for(self.screen, self.config.source)
    }

    pub fn current_field(&self) -> Option<FieldId> {
        self.fields().get(self.cursor).copied()
    }

    /// Configuration blockers, plus the ones that need the filesystem.
    ///
    /// `config` stays pure so its rules can be tested without a disk; anything
    /// that has to stat a path lives here.
    pub fn blockers(&self, dry_run: bool) -> Vec<Blocker> {
        let mut b = self.config.blockers(dry_run);
        if !self.config.source.takes_path() {
            return b;
        }
        let raw = self.config.path.trim();
        if raw.is_empty() {
            return b;
        }
        let path = std::path::Path::new(raw);
        if !path.is_dir() {
            b.push(Blocker {
                field: "path",
                why: format!("`{raw}` is not a directory on this machine"),
            });
            return b;
        }
        // The runner remains the authority on what a source must contain; this
        // only turns one common mistake into a message on the screen you are
        // already looking at, instead of a failed run.
        if self.config.source == Source::GitHistory && !path.join(".git").exists() {
            b.push(Blocker {
                field: "path",
                why: format!("`{raw}` has no .git — git-history needs a repository root"),
            });
        }
        b
    }

    pub fn warnings(&self, dry_run: bool) -> Vec<Warning> {
        self.config.warnings(dry_run)
    }

    pub fn max_tokens(&self) -> Option<i64> {
        self.config.max_tokens.trim().parse().ok()
    }

    /// Moves within the current screen, skipping fields that are inert.
    /// The exchange the Agents panel is showing, following the newest until
    /// the operator pins one.
    pub fn selected_agent(&self) -> Option<&AgentExchange> {
        if self.progress.agents.is_empty() {
            return None;
        }
        match self.agent_cursor {
            Some(i) => self.progress.agents.get(i),
            None => self.progress.agents.back(),
        }
    }

    pub fn move_agent_cursor(&mut self, delta: isize) {
        let len = self.progress.agents.len();
        if len == 0 {
            return;
        }
        let current = self.agent_cursor.unwrap_or(len - 1) as isize;
        self.agent_cursor = Some((current + delta).clamp(0, len as isize - 1) as usize);
    }

    /// Resumes following the newest exchange.
    pub fn follow_latest_agent(&mut self) {
        self.agent_cursor = None;
    }

    pub fn move_cursor(&mut self, delta: isize) {
        let fields = self.fields();
        if fields.is_empty() {
            return;
        }
        let len = fields.len() as isize;
        let mut next = self.cursor as isize;
        for _ in 0..len {
            next = (next + delta).rem_euclid(len);
            if is_active(fields[next as usize], &self.config) {
                break;
            }
        }
        self.cursor = next as usize;
    }

    pub fn goto(&mut self, screen: Screen) {
        self.screen = screen;
        self.cursor = 0;
        self.editing = false;
        // Arriving at the plan screen is what triggers detection — it needs the
        // path from Options, which the operator has just finished with.
        if screen == Screen::Projects {
            self.enter_projects();
        }
    }

    /// Starts a run. Refuses rather than launching something that will fail.
    pub fn start(&mut self, dry_run: bool) {
        if self.is_running() {
            self.status = "a run is already in flight — press x to stop it".into();
            return;
        }
        // The source-code source has two independent actions. Indexing is a
        // backend call, not a runner pass, and only makes sense on a real run;
        // extraction is the ordinary run below. Fire the index first, then fall
        // through to extraction only when it is enabled.
        if self.config.source == Source::Code {
            if !dry_run && self.config.index_code {
                self.index_code();
            }
            if !self.config.extract_knowledge {
                self.status = if dry_run {
                    "extraction is off — nothing to preview".into()
                } else if self.config.index_code {
                    "indexing started — extraction is off".into()
                } else {
                    "enable Extract knowledge or Index for the code source".into()
                };
                return;
            }
        }
        let blockers = self.blockers(dry_run);
        if let Some(first) = blockers.first() {
            self.status = format!("cannot start: {}", first.why);
            return;
        }
        // A fresh, unqueued run begins a new review set; a queued one continues
        // the plan's, which is why `session_runs` outlives `progress`.
        if self.run_queue.is_empty() {
            self.session_runs.clear();
        }
        self.progress = Progress {
            dry_run,
            ..Default::default()
        };
        self.last_command = Some(if dry_run {
            LastCommand::Preview
        } else {
            LastCommand::Run
        });
        // A run picked from the list would otherwise keep winning in
        // `active_run`, so the review screen would still point at the old queue
        // after this run stages a new one.
        self.picked_run = None;
        self.candidates.clear();
        self.selected.clear();
        self.started_at = Some(std::time::Instant::now());
        self.handle = Some(crate::runner::spawn(&self.binary, &self.config, dry_run));
        self.goto(Screen::Running);
        self.status = if dry_run {
            "previewing — nothing will be posted".into()
        } else {
            "running — candidates are staged for review, never committed".into()
        };
    }

    pub fn stop(&mut self) {
        if let Some(handle) = self.handle.as_mut() {
            handle.cancel();
            self.status = "stopped — everything already staged is intact".into();
        }
    }

    /// Drains whatever the runner produced since the last frame.
    pub fn pump(&mut self) {
        let mut messages = Vec::new();
        if let Some(handle) = self.handle.as_ref() {
            // Bounded so a fast runner cannot starve the draw loop.
            for _ in 0..512 {
                match handle.rx.try_recv() {
                    Ok(msg) => messages.push(msg),
                    Err(_) => break,
                }
            }
        }
        let was_done = self.progress.finished.is_some();
        for msg in messages {
            self.progress.apply(msg);
        }
        // Runs created by this invocation join the session's list. A monorepo
        // run emits several at once; a queued plan adds one per invocation, and
        // `start` clears `progress` between them.
        for created in &self.progress.created_runs {
            if !self.session_runs.iter().any(|r| r.run_id == created.run_id) {
                self.session_runs.push(created.clone());
            }
        }
        if !was_done && self.progress.finished.is_some() {
            // A folder-of-repos plan is a queue: the next repository starts
            // where the last finished, and only the final run lands on Summary.
            if !self.advance_queue() {
                self.goto(Screen::Summary);
            }
        }
        if let Some(reply) = self.take_api_reply() {
            self.handle_api(reply);
        }
    }

    /// Runs a backend call on its own thread.
    ///
    /// One at a time on purpose: two concurrent writes to the same queue would
    /// race on candidate versions, and a second read would only overwrite the
    /// first. Refusing tells the operator what it is waiting on.
    fn spawn_api<F>(&mut self, label: &str, call: F)
    where
        F: FnOnce(&Client) -> ApiMsg + Send + 'static,
    {
        if let Some(busy) = self.pending.as_ref() {
            self.status = format!("busy — {busy}");
            return;
        }
        let (base, key) = (self.config.api_url.clone(), self.config.api_key.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let msg = match Client::new(&base, &key) {
                Ok(client) => call(&client),
                Err(e) => ApiMsg::Failed(e.to_string()),
            };
            let _ = tx.send(msg);
        });
        self.api_rx = Some(rx);
        self.pending = Some(label.to_string());
        self.status = format!("{label}…");
    }

    /// True while a backend call is outstanding.
    pub fn is_waiting(&self) -> bool {
        self.pending.is_some()
    }

    fn take_api_reply(&mut self) -> Option<ApiMsg> {
        let msg = self.api_rx.as_ref()?.try_recv().ok()?;
        self.api_rx = None;
        self.pending = None;
        Some(msg)
    }

    fn handle_api(&mut self, msg: ApiMsg) {
        match msg {
            ApiMsg::Failed(e) => self.status = format!("✗ {e}"),
            ApiMsg::Probed(Ok(m)) => {
                self.status = m;
                // A backend that answers has history, and the operator who just
                // typed a URL and a key is often here to pick up a run they
                // already started rather than to begin a new one. Fetching the
                // list now means it is on the Review screen when they reach it,
                // instead of requiring them to know that `R` reloads it.
                self.load_runs();
            }
            ApiMsg::Probed(Err(e)) => self.status = format!("✗ {e}"),
            ApiMsg::Runs(Ok(runs)) => {
                self.status = if runs.is_empty() {
                    "no migration runs on this backend yet".into()
                } else if self.screen == Screen::Connection {
                    // Said differently on the connection screen: the list is not
                    // in front of them yet, so the message has to say where it
                    // is and how many resumable runs it holds.
                    let resumable = runs.iter().filter(|r| r.is_resumable()).count();
                    format!(
                        "connected · {} existing run(s), {resumable} still open — R to resume one",
                        runs.len()
                    )
                } else {
                    format!("{} run(s) — Enter opens one", runs.len())
                };
                self.runs = runs;
                self.run_cursor = self.run_cursor.min(self.runs.len().saturating_sub(1));
            }
            ApiMsg::Runs(Err(e)) => self.status = format!("could not list runs: {e}"),
            ApiMsg::Candidates(Ok(list)) => {
                self.status = format!("{} candidate(s) awaiting a decision", list.len());
                self.candidates = list;
                self.review_cursor = 0;
                self.selected.clear();
            }
            ApiMsg::Candidates(Err(e)) => self.status = format!("could not load the queue: {e}"),
            ApiMsg::Reviewed { result, verdict } => match result {
                Ok(ReviewResponse {
                    applied,
                    conflicts,
                    results,
                }) => {
                    self.conflicts = results
                        .iter()
                        .filter(|r| r.outcome != "applied")
                        .map(|r| r.candidate_id.clone())
                        .collect();
                    self.last_command = Some(LastCommand::Review { applied, conflicts });
                    self.status = if conflicts > 0 {
                        // Somebody else moved these. Acting on the versions we
                        // hold would overwrite their decision, so the only
                        // honest response is to re-read the queue.
                        format!("{applied} applied, {conflicts} changed under you — reloading")
                    } else {
                        format!("{applied} {}", verdict.past_tense())
                    };
                    self.load_candidates();
                }
                Err(e) => self.status = format!("review failed: {e}"),
            },
            ApiMsg::Committed(Ok(resp)) => {
                self.status = format!(
                    "committed {} · indexed {} · pending {}",
                    resp.committed, resp.indexed, resp.pending_index
                );
                self.last_command = Some(LastCommand::Commit(resp));
                self.goto(Screen::Summary);
            }
            ApiMsg::Committed(Err(e)) => self.status = format!("commit failed: {e}"),
            ApiMsg::Edited(Ok(candidate)) => {
                self.status = format!("edited — {}", candidate.title());
                // Replace in place by id, not by cursor position: the cursor may
                // have moved while the request was in flight.
                if let Some(row) = self.candidates.iter_mut().find(|c| c.id == candidate.id) {
                    *row = candidate;
                }
            }
            ApiMsg::Edited(Err(e)) => {
                self.status = format!("edit failed: {e}");
                // A refused edit means this row is not what we thought it was —
                // stale, or no longer staged. Re-read rather than leave the
                // reviewer deciding on a version that does not exist.
                self.load_candidates();
            }
            ApiMsg::Cancelled(Ok(n)) => {
                self.status = format!("{n} pending candidate(s) cancelled");
                self.candidates.clear();
                self.load_runs();
            }
            ApiMsg::Cancelled(Err(e)) => self.status = format!("cancel failed: {e}"),
            ApiMsg::Projects(Ok(projects)) => {
                self.existing_projects = projects;
                self.rematch_plan();
                let (create, select, _) = self.plan_summary();
                self.status = format!(
                    "{} project(s) on the backend · plan: {create} new, {select} existing",
                    self.existing_projects.len()
                );
            }
            ApiMsg::Projects(Err(e)) => {
                self.status = format!("could not list projects: {e} — matching by name only")
            }
            ApiMsg::PlanExecuted(Ok(exec)) => {
                self.plan = exec.rows;
                match self.plan_layout {
                    Layout::Monorepo => {
                        self.config.config_path = exec.config_path.clone();
                        self.status =
                            format!("wrote {} — starting routed run", exec.config_path);
                        // The routed run reads the config just written; `start`
                        // reuses the ordinary launch path, which emits --config.
                        self.start(exec.dry_run);
                    }
                    Layout::RepoCollection => self.start_repo_queue(),
                }
            }
            ApiMsg::PlanExecuted(Err(e)) => self.status = format!("plan failed: {e}"),
            ApiMsg::Indexed(Ok(out)) => {
                self.status = format!(
                    "code index {}: {} file(s), {} chunk(s)",
                    out.status, out.file_count, out.chunk_count
                );
            }
            ApiMsg::Indexed(Err(e)) => self.status = format!("code index failed: {e}"),
        }
    }

    /// Which run the review screen is acting on.
    ///
    /// A run picked from the list wins over the one this session produced, so
    /// an operator can come back the next morning and finish a queue they left
    /// half-reviewed — the queue lives in the backend, not in this process.
    pub fn active_run(&self) -> Option<String> {
        if let Some(picked) = &self.picked_run {
            return Some(picked.clone());
        }
        // A monorepo run creates one run per project. Pointing the review screen
        // at the last one would silently hide every other project's queue, so
        // with more than one this session the operator must pick which project
        // to review. A single run keeps opening directly, as before.
        if self.session_runs.len() > 1 {
            return None;
        }
        self.progress.run_id.clone()
    }

    /// True when the review screen has nothing to act on and should offer the
    /// list of runs instead of an empty queue.
    pub fn picking_run(&self) -> bool {
        self.active_run().is_none()
    }

    /// Whether the run picker is showing this session's per-project runs rather
    /// than the backend's run history. The session list is offered first after a
    /// monorepo run — it is labelled by project and is exactly the set the
    /// operator just staged — and `R` still loads the full history over it.
    pub fn showing_session_runs(&self) -> bool {
        self.runs.is_empty() && !self.session_runs.is_empty()
    }

    /// The number of rows the run picker is currently showing.
    pub fn run_list_len(&self) -> usize {
        if self.showing_session_runs() {
            self.session_runs.len()
        } else {
            self.runs.len()
        }
    }

    pub fn load_runs(&mut self) {
        self.spawn_api("listing runs", |c| {
            ApiMsg::Runs(c.runs().map_err(|e| e.to_string()))
        });
    }

    pub fn pick_run(&mut self) {
        let id = if self.showing_session_runs() {
            self.session_runs
                .get(self.run_cursor)
                .map(|r| r.run_id.clone())
        } else {
            self.runs.get(self.run_cursor).map(|r| r.id.clone())
        };
        let Some(id) = id else {
            return;
        };
        self.picked_run = Some(id);
        self.load_candidates();
    }

    /// Returns to the run list.
    pub fn unpick_run(&mut self) {
        self.picked_run = None;
        self.progress.run_id = None;
        self.candidates.clear();
        self.selected.clear();
        self.load_runs();
    }

    /// Cancels the run under the cursor.
    ///
    /// Cancel, not delete: `migration_provenance` cascades on a run's deletion,
    /// so removing a run would erase the record of where its committed
    /// knowledge came from. Cancelling drops what is still pending and leaves
    /// that record standing.
    pub fn cancel_run(&mut self) {
        let Some(run) = self.runs.get(self.run_cursor).cloned() else {
            return;
        };
        if self.picked_run.as_deref() == Some(run.id.as_str()) {
            self.picked_run = None;
        }
        let id = run.id.clone();
        self.spawn_api("cancelling run", move |c| {
            ApiMsg::Cancelled(c.cancel(&id).map_err(|e| e.to_string()))
        });
    }

    pub fn load_candidates(&mut self) {
        let Some(run_id) = self.active_run() else {
            self.load_runs();
            return;
        };
        self.spawn_api("loading queue", move |c| {
            ApiMsg::Candidates(c.candidates(&run_id, "staged").map_err(|e| e.to_string()))
        });
    }

    pub fn toggle_selected(&mut self) {
        let Some(c) = self.candidates.get(self.review_cursor) else {
            return;
        };
        if c.needs_individual_review() {
            self.status =
                "this one cannot be batch-approved — press a to approve it on its own".into();
            return;
        }
        match self.selected.iter().position(|id| id == &c.id) {
            Some(i) => {
                self.selected.remove(i);
            }
            None => self.selected.push(c.id.clone()),
        }
    }

    /// Candidates a batch action may touch: everything except the ones that
    /// carry a second gate.
    pub fn batchable(&self) -> Vec<&Candidate> {
        self.candidates
            .iter()
            .filter(|c| !c.needs_individual_review())
            .collect()
    }

    /// How many a batch approval would apply to right now, and whether that is
    /// the selection or the whole queue.
    pub fn batch_target(&self) -> (usize, bool) {
        if self.selected.is_empty() {
            (self.batchable().len(), true)
        } else {
            (self.selected.len(), false)
        }
    }

    pub fn decide(&mut self, verdict: Verdict, batch: bool) {
        let Some(run_id) = self.active_run() else {
            return;
        };
        let actions: Vec<(String, Verdict, i64)> = if batch {
            // With nothing selected, a batch action means the whole queue —
            // minus the candidates that are excluded from batching by design,
            // which are never swept along.
            if self.selected.is_empty() {
                self.batchable()
                    .into_iter()
                    .map(|c| (c.id.clone(), verdict, c.version))
                    .collect()
            } else {
                self.candidates
                    .iter()
                    .filter(|c| self.selected.contains(&c.id))
                    .map(|c| (c.id.clone(), verdict, c.version))
                    .collect()
            }
        } else {
            self.candidates
                .get(self.review_cursor)
                .map(|c| vec![(c.id.clone(), verdict, c.version)])
                .unwrap_or_default()
        };
        if actions.is_empty() {
            self.status = "nothing to act on".into();
            return;
        }
        let label = format!("applying {} decision(s)", actions.len());
        self.spawn_api(&label, move |c| ApiMsg::Reviewed {
            result: c.review(&run_id, &actions).map_err(|e| e.to_string()),
            verdict,
        });
    }

    /// The candidate the review cursor is on, if the queue is showing one.
    pub fn current_candidate(&self) -> Option<&Candidate> {
        if self.picking_run() {
            return None;
        }
        self.candidates.get(self.review_cursor)
    }

    /// Sends a reviewer's rewrite of the candidate under the cursor.
    ///
    /// `title` is folded into the destination hint rather than sent separately:
    /// the hint is what the destination actually reads at commit time, and a
    /// title stored anywhere else would not survive the trip.
    ///
    /// `kind` re-files the candidate. It is here rather than behind its own
    /// keystroke because the classifier's most common mistake is the
    /// destination, not the wording — a rule filed as context — and the
    /// reviewer is already looking at the text when they notice.
    pub fn apply_candidate_edit(&mut self, title: String, kind: String, content: String) {
        let Some(run_id) = self.active_run() else {
            return;
        };
        let Some(candidate) = self.current_candidate() else {
            return;
        };
        let (id, version) = (candidate.id.clone(), candidate.version);
        // A blank or unchanged kind means "leave it where it is"; the backend
        // rejects anything it does not recognise, so a typo is a message rather
        // than a silent no-op.
        let kind = match kind.trim() {
            "" => candidate.destination_kind.clone(),
            k => k.to_string(),
        };
        let mut hint = candidate.destination_hint.clone();
        let trimmed = title.trim();
        if !trimmed.is_empty() {
            match hint.as_object_mut() {
                Some(map) => {
                    map.insert("title".into(), serde_json::Value::String(trimmed.into()));
                }
                // A hint that is not an object (absent, or a bare value from an
                // older run) is replaced rather than dropped on the floor.
                None => hint = serde_json::json!({ "title": trimmed }),
            }
        }
        self.spawn_api("saving edit", move |c| {
            ApiMsg::Edited(
                c.edit_candidate(&run_id, &id, version, &content, &hint, &kind)
                    .map_err(|e| e.to_string()),
            )
        });
    }

    pub fn commit(&mut self) {
        let Some(run_id) = self.active_run() else {
            return;
        };
        self.spawn_api("committing", move |c| {
            ApiMsg::Committed(c.commit(&run_id).map_err(|e| e.to_string()))
        });
    }

    /// Probes if the connection details are complete and have changed.
    ///
    /// Called when an edit on the URL or the key ends. Returns quietly when
    /// either field is blank — half-typed connection details are the normal
    /// state while typing, not an error worth a red line.
    pub fn probe_if_connection_changed(&mut self) {
        let (url, key) = (self.config.api_url.trim(), self.config.api_key.trim());
        if url.is_empty() || key.is_empty() {
            return;
        }
        let pair = (url.to_string(), key.to_string());
        if self.last_probed.as_ref() == Some(&pair) {
            return;
        }
        self.last_probed = Some(pair);
        self.probe();
    }

    pub fn probe(&mut self) {
        self.spawn_api("testing the connection", |c| {
            ApiMsg::Probed(c.probe().map_err(|e| e.to_string()))
        });
    }

    /// Fires the source-code index action against `/v1/code`.
    ///
    /// The project is the one named on Connection, or the repository's own name
    /// when none is set — code search is scoped to a project just like memory.
    /// A local backend indexes the checkout in place (`root_path`); a remote one
    /// must clone, so the repo's `origin` URL is sent instead.
    pub fn index_code(&mut self) {
        let path = self.config.path.trim().to_string();
        let project = if self.config.project.trim().is_empty() {
            repository_name(Path::new(&path))
        } else {
            self.config.project.trim().to_string()
        };
        let local = crate::config::is_local(&self.config.api_url);
        let root_path = local.then(|| path.clone());
        let repo_url = if local { None } else { git_origin_url(&path) };
        self.spawn_api("indexing code for search", move |c| {
            ApiMsg::Indexed(
                c.index_code(&project, root_path.as_deref(), repo_url.as_deref())
                    .map_err(|e| e.to_string()),
            )
        });
    }

    // ── Monorepo plan ─────────────────────────────────────────────────────────

    /// Detects the sub-projects under the current path and loads the backend
    /// projects to match them against.
    ///
    /// Runs at most once per path: re-entering the screen keeps the operator's
    /// decisions. Detection touches only the filesystem and is bounded, so it
    /// runs inline; the project listing it matches against is a backend call and
    /// goes through the async channel like every other.
    pub fn enter_projects(&mut self) {
        if self.plan_detected && self.plan_path == self.config.path {
            return; // already detected for this path — keep the operator's edits
        }
        self.plan.clear();
        self.plan_cursor = 0;
        self.selecting_for = None;
        self.plan_detected = true;
        self.plan_path = self.config.path.clone();
        self.existing_config = false;

        if !self.config.source.takes_path() {
            self.plan_note = "this source scans no path, so it has no sub-projects".into();
            return;
        }
        let raw = self.config.path.trim();
        let root = Path::new(raw);
        if raw.is_empty() || !root.is_dir() {
            self.plan_note = "set a valid path on the Options screen first".into();
            return;
        }
        // Two shapes reach this screen. A checkout holds packages and routes
        // them from one config in one run; a plain folder holds independent
        // repositories, which have no shared repo to host that config and are
        // migrated one run apiece.
        let (layout, mut detected) = monorepo::survey(root);
        self.plan_layout = layout;
        self.existing_config =
            layout == Layout::Monorepo && monorepo::read_existing(root).is_some();

        if detected.is_empty() {
            self.plan_note = match layout {
                Layout::Monorepo =>
                    "no sub-projects found — this scans as a single project (use the Project field on Connection)"
                        .into(),
                Layout::RepoCollection =>
                    "not a Git repository, and no Git repositories directly inside it — nothing to plan"
                        .into(),
            };
            return;
        }

        match layout {
            Layout::Monorepo => {
                // Prepend the repository itself as the catch-all project, so
                // root-level docs and anything outside a package still route
                // somewhere instead of failing the run as unmapped.
                let root_row = monorepo::repository_root_row(root, &detected);
                detected.insert(0, root_row);
                self.plan_note = format!(
                    "{} package(s) + the repository root — one routed run",
                    detected.len().saturating_sub(1)
                );
            }
            Layout::RepoCollection => {
                // No catch-all: each repository is its own scan root, and there
                // is nothing outside them to sweep up.
                self.plan_note = format!(
                    "{} separate repositor{} — one run each",
                    detected.len(),
                    if detected.len() == 1 { "y" } else { "ies" }
                );
            }
        }
        // Build with no matches yet; the async listing rebuilds with them.
        self.plan = monorepo::build_plan(detected, &self.existing_projects);
        self.load_projects();
    }

    /// Fetches the backend projects the plan matches against, scoped to the
    /// run's client when it has one.
    pub fn load_projects(&mut self) {
        let client = self.config.client.trim().to_string();
        self.spawn_api("loading projects", move |c| {
            let cid = (!client.is_empty()).then_some(client.as_str());
            ApiMsg::Projects(c.projects(cid).map_err(|e| e.to_string()))
        });
    }

    fn rematch_plan(&mut self) {
        if self.plan.is_empty() {
            return;
        }
        let detected: Vec<_> = self.plan.iter().map(|r| r.detected.clone()).collect();
        self.plan = monorepo::build_plan(detected, &self.existing_projects);
        self.plan_cursor = self.plan_cursor.min(self.plan.len().saturating_sub(1));
    }

    pub fn plan_move(&mut self, delta: isize) {
        if self.selecting_for.is_some() {
            let n = self.existing_projects.len();
            if n > 0 {
                self.select_cursor =
                    (self.select_cursor as isize + delta).rem_euclid(n as isize) as usize;
            }
            return;
        }
        let n = self.plan.len();
        if n > 0 {
            self.plan_cursor = (self.plan_cursor as isize + delta).rem_euclid(n as isize) as usize;
        }
    }

    /// Cycles the focused row's action: Create → Skip → (Select if it matched)
    /// → Create. A row with no match never offers Select here — that is what the
    /// existing-project picker is for.
    pub fn cycle_action(&mut self) {
        let Some(row) = self.plan.get_mut(self.plan_cursor) else {
            return;
        };
        row.action = match &row.action {
            Action::Create => Action::Skip,
            Action::Skip => match &row.matched {
                Some(p) => Action::Select(p.id.clone()),
                None => Action::Create,
            },
            Action::Select(_) => Action::Create,
        };
    }

    /// Opens the picker of existing backend projects for the focused row.
    pub fn begin_select(&mut self) {
        if self.plan.get(self.plan_cursor).is_none() {
            return;
        }
        if self.existing_projects.is_empty() {
            self.status = "no existing projects to choose from on this backend".into();
            return;
        }
        self.selecting_for = Some(self.plan_cursor);
        // Open on the current selection when there is one, else the top.
        self.select_cursor = self
            .plan
            .get(self.plan_cursor)
            .and_then(|r| r.selected_project_id())
            .and_then(|id| self.existing_projects.iter().position(|p| p.id == id))
            .unwrap_or(0);
    }

    pub fn cancel_select(&mut self) {
        self.selecting_for = None;
    }

    /// Routes the row being picked into the highlighted existing project.
    pub fn confirm_select(&mut self) {
        let Some(row_idx) = self.selecting_for.take() else {
            return;
        };
        let Some(project) = self.existing_projects.get(self.select_cursor).cloned() else {
            return;
        };
        if let Some(row) = self.plan.get_mut(row_idx) {
            row.action = Action::Select(project.id.clone());
            row.matched = Some(project);
        }
    }

    /// How many rows the plan will act on, and how many of those create a new
    /// project — the two numbers the confirmation needs to state plainly.
    pub fn plan_summary(&self) -> (usize, usize, usize) {
        let mut create = 0;
        let mut select = 0;
        let mut skip = 0;
        for row in &self.plan {
            match row.action {
                Action::Create => create += 1,
                Action::Select(_) => select += 1,
                Action::Skip => skip += 1,
            }
        }
        (create, select, skip)
    }

    /// Starts the next repository of a folder-of-repos plan.
    ///
    /// Returns false when the queue is spent, which is what lets the last run
    /// land on the Summary screen instead of looping. Draining also restores
    /// the operator's own path/project: the queue rewrites both per repository,
    /// and leaving them pointed at whichever repository happened to be last
    /// would quietly change what the next manual run scans.
    fn advance_queue(&mut self) -> bool {
        if self.queue_pos + 1 < self.run_queue.len() {
            self.queue_pos += 1;
            self.start_queued_run();
            return true;
        }
        if !self.run_queue.is_empty() {
            if let Some((path, project)) = self.queue_restore.take() {
                self.config.path = path;
                self.config.project = project;
            }
            self.run_queue.clear();
            self.queue_pos = 0;
        }
        false
    }

    /// Points the configuration at one queued repository and launches it.
    ///
    /// No routing config is involved: with `config_path` empty, `to_args` emits
    /// `--project`, which is the single-project flow the runner already has.
    /// Each repository is its own Git checkout, so history and ignore rules
    /// resolve the way they would if it were scanned on its own — which is the
    /// reason this layout is a queue rather than one big scan.
    fn start_queued_run(&mut self) {
        let Some(next) = self.run_queue.get(self.queue_pos).cloned() else {
            return;
        };
        self.config.path = next.path;
        self.config.project = next.project_id;
        self.config.config_path.clear();
        let (pos, total) = (self.queue_pos + 1, self.run_queue.len());
        self.start(false);
        if self.handle.is_some() {
            self.status = format!("repository {pos}/{total} — {}", next.alias);
        }
    }

    /// Turns a resolved folder-of-repos plan into the run queue and starts it.
    fn start_repo_queue(&mut self) {
        // `plan_path` is the folder detection ran against; `config.path` is
        // about to be rewritten per repository, so the parent has to come from
        // the plan, not from the live configuration.
        let root = std::path::PathBuf::from(self.plan_path.trim());
        self.queue_restore = Some((self.config.path.clone(), self.config.project.clone()));
        self.run_queue = self
            .plan
            .iter()
            .filter(|r| r.action != Action::Skip)
            .filter_map(|r| {
                r.resolved_project_id.as_ref().map(|pid| QueuedRun {
                    alias: r.detected.alias.clone(),
                    path: root.join(&r.detected.rel_dir).to_string_lossy().to_string(),
                    project_id: pid.clone(),
                })
            })
            .collect();
        self.queue_pos = 0;
        if self.run_queue.is_empty() {
            self.queue_restore = None;
            self.status = "no repository resolved to a project".into();
            return;
        }
        // The whole queue is one review set, so it is cleared once here rather
        // than by each `start` inside it.
        self.session_runs.clear();
        self.start_queued_run();
    }

    /// Confirms the plan: creates the projects that need creating, writes the
    /// `.nexusmind.yaml`, and — on success — launches the routed run.
    ///
    /// All of it happens on one background thread and reports once, rather than
    /// a create-per-keystroke state machine: a monorepo of twenty packages
    /// should be one confirmation, not twenty.
    pub fn execute_plan(&mut self, dry_run: bool) {
        if self.is_running() {
            self.status = "a run is already in flight — press x to stop it".into();
            return;
        }
        // Confirming a plan creates backend projects and writes a file into the
        // repository — real, hard-to-undo side effects. A "preview" must not do
        // either, so a dry run is refused here rather than quietly creating
        // projects. Preview each project's queue after the run instead.
        if dry_run {
            self.status = match self.plan_layout {
                Layout::Monorepo =>
                    "a plan is applied with r — it creates projects and writes the config, so \
                     there is no side-effect-free preview; review each queue after the run"
                        .into(),
                Layout::RepoCollection =>
                    "a plan is applied with r — it creates projects and runs each repository, \
                     so there is no side-effect-free preview; review each queue after the runs"
                        .into(),
            };
            return;
        }
        let (create, select, _skip) = self.plan_summary();
        if create + select == 0 {
            self.status = "every sub-project is skipped — nothing to migrate".into();
            return;
        }
        // A real run needs credentials before it creates anything on the backend;
        // fail here rather than after creating half the projects.
        if let Some(first) = self.config.blockers(dry_run).into_iter().next() {
            self.status = format!("cannot start: {}", first.why);
            return;
        }
        let rows = self.plan.clone();
        let layout = self.plan_layout;
        let client = self.config.client.trim().to_string();
        let root = Path::new(self.config.path.trim()).to_path_buf();
        let repo_id = repository_name(&root);
        let config_path = monorepo::config_path(&root)
            .to_string_lossy()
            .to_string();

        self.spawn_api("creating projects", move |c| {
            let mut rows = rows;
            let client = (!client.is_empty()).then_some(client.as_str());
            for row in rows.iter_mut() {
                match &row.action {
                    Action::Skip => continue,
                    Action::Select(id) => row.resolved_project_id = Some(id.clone()),
                    Action::Create => match c.create_project(&row.detected.name, client, None) {
                        Ok(p) => row.resolved_project_id = Some(p.id),
                        Err(e) => {
                            return ApiMsg::PlanExecuted(Err(format!(
                                "creating project `{}`: {e}",
                                row.detected.name
                            )))
                        }
                    },
                }
            }
            // A folder of separate repositories has no repository to hold a
            // routing config — each one runs on its own, so there is nothing to
            // write and the queue takes over from here.
            if layout == Layout::RepoCollection {
                return ApiMsg::PlanExecuted(Ok(ExecutedPlan {
                    rows,
                    config_path: String::new(),
                    dry_run,
                }));
            }
            let config = monorepo::build_config(&repo_id, client, &rows);
            if config.projects.is_empty() {
                return ApiMsg::PlanExecuted(Err("no routable sub-projects after execution".into()));
            }
            let yaml = match monorepo::to_yaml(&config) {
                Ok(y) => y,
                Err(e) => return ApiMsg::PlanExecuted(Err(format!("building config: {e}"))),
            };
            if let Err(e) = std::fs::write(&config_path, yaml) {
                return ApiMsg::PlanExecuted(Err(format!("writing {config_path}: {e}")));
            }
            ApiMsg::PlanExecuted(Ok(ExecutedPlan {
                rows,
                config_path,
                dry_run,
            }))
        });
    }
}

/// The repository's `origin` remote URL, for indexing a remote checkout the
/// backend cannot see on disk. `None` when the path is not a repo or has no
/// origin — the caller turns that into a clear "set your git remote" message.
fn git_origin_url(path: &str) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(["-C", path, "remote", "get-url", "origin"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let url = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!url.is_empty()).then_some(url)
}

/// A human name for the repository at `root`, used as `repository.id` (slugged
/// downstream). The directory's own name, or `repository` when the path has
/// none (e.g. `.` at a filesystem root, which does not happen in practice).
fn repository_name(root: &Path) -> String {
    root.canonicalize()
        .ok()
        .as_deref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(id: &str, kind: &str, attestation: serde_json::Value) -> Candidate {
        Candidate {
            id: id.into(),
            source_identity: format!("repo-docs:x:{id}.md"),
            destination_kind: kind.into(),
            content: "body".into(),
            destination_hint: serde_json::json!({ "title": id }),
            source_excerpt: None,
            confidence: Some(0.9),
            attestation,
            provenance_kind: "migrated".into(),
            status: "staged".into(),
            version: 1,
        }
    }

    #[test]
    fn turning_row_sampling_off_clears_the_answers_that_unlocked_it() {
        let mut c = RunConfig {
            source: Source::DbSchema,
            include_data: true,
            tables: "invoices".into(),
            sample_limit: "50".into(),
            redact_pii: true,
            attest: "MSA-2026-014".into(),
            ..Default::default()
        };
        FieldId::IncludeData.toggle(&mut c);
        assert!(!c.include_data);
        assert_eq!(c.tables, "");
        assert_eq!(c.sample_limit, "");
        assert!(!c.redact_pii);
        assert_eq!(c.attest, "", "a stale attestation must never survive");
    }

    #[test]
    fn the_sampling_gates_are_inert_until_sampling_is_unlocked() {
        let mut c = RunConfig {
            source: Source::DbSchema,
            ..Default::default()
        };
        assert!(!is_active(FieldId::Tables, &c));
        assert!(!is_active(FieldId::Attest, &c));
        c.include_data = true;
        assert!(is_active(FieldId::Tables, &c));
        assert!(is_active(FieldId::Attest, &c));
    }

    #[test]
    fn the_cursor_skips_inert_fields() {
        let mut app = App::new();
        app.config.source = Source::DbSchema;
        app.goto(Screen::Options);
        // Path is absent for db-schema, so the list opens on Supabase.
        let fields = app.fields();
        assert_eq!(fields[0], FieldId::Supabase);
        assert_eq!(fields[1], FieldId::IncludeData);
        app.cursor = 1;
        app.move_cursor(1);
        assert_eq!(
            app.current_field(),
            Some(FieldId::NoLlm),
            "the four locked gates are stepped over, not typed into"
        );
    }

    #[test]
    fn a_run_that_dies_without_a_finished_event_is_recorded_as_failed() {
        let mut p = Progress::default();
        p.apply(RunnerMsg::Exited { code: Some(101) });
        let f = p.finished.expect("an exit must be terminal");
        assert!(!f.ok);
        assert!(f.error.unwrap().contains("101"));
    }

    /// A clean run must not be overwritten by the exit that follows it.
    #[test]
    fn a_successful_finish_survives_the_process_exit() {
        let mut p = Progress::default();
        p.apply(RunnerMsg::Line(ParsedLine::Event(RunEvent::Finished {
            ok: true,
            scanned: 69,
            classified: 60,
            fallbacks: 9,
            failed: 0,
            tokens_spent: 14716,
            aborted_on_budget: false,
            error: None,
        })));
        p.apply(RunnerMsg::Exited { code: Some(0) });
        assert!(p.finished.unwrap().ok);
        assert_eq!(p.classified, 60);
    }

    #[test]
    fn exclusions_are_ordered_largest_first_and_total_correctly() {
        let mut p = Progress::default();
        for (reason, count, sample) in [
            (
                "third-party asset",
                21_526usize,
                "node_modules/x/LICENSE.md",
            ),
            ("not engineering knowledge", 3, "docs/marketing/a.md"),
        ] {
            p.apply(RunnerMsg::Line(ParsedLine::Event(RunEvent::Excluded {
                reason: reason.into(),
                count,
                sample: sample.into(),
            })));
        }
        assert_eq!(
            p.exclusion_histogram(),
            vec![
                ("third-party asset".to_string(), 21_526),
                ("not engineering knowledge".to_string(), 3),
            ]
        );
        assert_eq!(
            p.excluded_total(),
            21_529,
            "the total counts files, not reasons — a single event can stand for              twenty thousand of them"
        );
    }

    #[test]
    fn the_budget_gauge_saturates_instead_of_overflowing() {
        let p = Progress {
            tokens: 60_000,
            ..Default::default()
        };
        assert_eq!(p.budget_ratio(Some(50_000)), Some(1.0));
        assert_eq!(p.budget_ratio(None), None);
        assert_eq!(p.budget_ratio(Some(0)), None, "a zero budget is no budget");
    }

    #[test]
    fn progress_ratio_is_zero_before_anything_is_known() {
        assert_eq!(Progress::default().classify_ratio(), 0.0);
    }

    #[test]
    fn a_scan_heartbeat_advances_the_counter_without_inventing_units() {
        let mut p = Progress::default();
        p.apply(RunnerMsg::Line(ParsedLine::Event(RunEvent::Scanning {
            seen: 137,
            current: "docs/API_SPEC.md".into(),
        })));
        assert_eq!(p.scanning_seen, 137);
        assert_eq!(p.current_origin, "docs/API_SPEC.md");
        assert_eq!(p.total, 0, "the unit count is unknown until the walk ends");
        assert_eq!(p.classify_ratio(), 0.0);
    }

    /// `never_ran` decides whether the Run screen is honest. It must stop being
    /// true the moment a run is launched, and stay false afterwards.
    #[test]
    fn never_ran_is_true_only_before_the_first_command() {
        let mut app = App::new();
        assert!(app.never_ran());

        app.config.api_key.clear();
        app.config.api_url.clear();
        app.start(false);
        assert!(app.never_ran(), "a refused start has still run nothing");

        app.last_command = Some(LastCommand::Preview);
        assert!(!app.never_ran());
    }

    #[test]
    fn an_unknown_event_is_counted_and_logged_not_dropped() {
        let mut p = Progress::default();
        p.apply(RunnerMsg::Line(ParsedLine::Unknown("embedded".into())));
        assert_eq!(p.unknown_events, 1);
        assert!(p.log.back().unwrap().contains("embedded"));
    }

    /// The log is what an operator scrolls after something goes wrong; it must
    /// not grow without bound over a run of thousands of units.
    fn exchange(i: usize, ok: bool) -> RunEvent {
        RunEvent::Agent {
            index: i,
            total: 9,
            origin: format!("docs/{i}.md"),
            prompt: "Classify this file…".into(),
            response: if ok {
                r#"{"destination_kind":"memory"}"#.into()
            } else {
                "No store_memory call: this turn's job was to propose…".into()
            },
            ok,
            error: (!ok).then(|| "carried no parseable candidate JSON".to_string()),
            tokens_spent: 1081,
            duration_ms: 4200,
        }
    }

    /// The panel exists to answer "what did we actually ask, and what came
    /// back" — the question a run of 249 fallbacks raises and nothing else
    /// could answer.
    #[test]
    fn exchanges_are_recorded_with_both_sides_and_the_reason_it_failed() {
        let mut p = Progress::default();
        p.apply(RunnerMsg::Line(ParsedLine::Event(exchange(1, false))));
        let a = p.agents.back().unwrap();
        assert!(a.prompt.contains("Classify this file"));
        assert!(a.response.contains("No store_memory call"));
        assert_eq!(
            a.error.as_deref(),
            Some("carried no parseable candidate JSON")
        );
        assert_eq!(a.tokens_spent, 1081, "a failed answer still cost money");
    }

    #[test]
    fn the_exchange_list_is_bounded_like_the_log() {
        let mut p = Progress::default();
        for i in 0..300 {
            p.apply(RunnerMsg::Line(ParsedLine::Event(exchange(i, true))));
        }
        assert_eq!(p.agents.len(), 100);
        assert_eq!(p.agents.back().unwrap().index, 299);
    }

    /// Following the newest is right while a run moves; pinning is right the
    /// moment something looks wrong.
    #[test]
    fn the_agent_panel_follows_the_newest_until_it_is_pinned() {
        let mut app = App::new();
        for i in 0..5 {
            app.progress
                .apply(RunnerMsg::Line(ParsedLine::Event(exchange(i, true))));
        }
        assert_eq!(app.selected_agent().unwrap().index, 4, "follows the newest");

        app.move_agent_cursor(-1);
        assert_eq!(app.selected_agent().unwrap().index, 3);
        app.progress
            .apply(RunnerMsg::Line(ParsedLine::Event(exchange(99, true))));
        assert_eq!(
            app.selected_agent().unwrap().index,
            3,
            "a pinned exchange does not move under the operator"
        );

        app.follow_latest_agent();
        assert_eq!(app.selected_agent().unwrap().index, 99);
    }

    #[test]
    fn the_agent_cursor_cannot_leave_the_list() {
        let mut app = App::new();
        app.progress
            .apply(RunnerMsg::Line(ParsedLine::Event(exchange(0, true))));
        app.move_agent_cursor(-50);
        assert_eq!(app.selected_agent().unwrap().index, 0);
        app.move_agent_cursor(50);
        assert_eq!(app.selected_agent().unwrap().index, 0);
    }

    #[test]
    fn moving_the_cursor_with_no_exchanges_is_harmless() {
        let mut app = App::new();
        app.move_agent_cursor(1);
        assert!(app.selected_agent().is_none());
    }

    /// The mood is read off state the operator can already see. These are the
    /// mappings, so a future change to them is a deliberate one.
    #[test]
    fn the_mood_follows_the_run() {
        let mut app = App::new();
        assert_eq!(app.mood(), Mood::Resting, "nothing has run");

        app.started_at = Some(std::time::Instant::now());
        assert_eq!(app.mood(), Mood::Working, "something is running");

        app.progress.total = 69;
        app.progress.staged = Some((69, 0, 0));
        assert_eq!(
            app.mood(),
            Mood::Working,
            "still one mood: something is running"
        );

        app.progress.finished = Some(FinishedRun {
            ok: true,
            aborted_on_budget: false,
            error: None,
        });
        assert_eq!(app.mood(), Mood::Celebrating, "staged and finished cleanly");
    }

    /// A failed run does not celebrate.
    #[test]
    fn a_failure_rests_rather_than_celebrating() {
        let mut app = App::new();
        app.progress.staged = Some((3, 0, 0));
        app.progress.finished = Some(FinishedRun {
            ok: false,
            aborted_on_budget: false,
            error: Some("the runner exited with status 101".into()),
        });
        assert_eq!(app.mood(), Mood::Resting);
    }

    /// The toggle can turn it off anywhere, but cannot force it on where the
    /// terminal would render it as garbage.
    #[test]
    fn the_toggle_cannot_force_the_mascot_onto_a_terminal_that_cannot_draw_it() {
        let mut app = App::new();
        let possible = Mascot::compiled_in() && mascot::terminal_supports();
        app.mascot_on = false;
        app.toggle_mascot();
        assert_eq!(
            app.mascot_on, possible,
            "toggling on is only allowed where drawing is"
        );
        app.toggle_mascot();
        assert!(!app.mascot_on, "and off always works");
    }

    /// With the mascot off there is nothing to draw, whatever the state.
    #[test]
    fn a_disabled_mascot_yields_no_frame() {
        let mut app = App::new();
        app.mascot_on = false;
        app.started_at = Some(std::time::Instant::now());
        assert!(app.mascot(80, 40).is_none());
    }

    #[test]
    fn the_activity_panel_cycles_back_to_both() {
        let v = ActivityView::Both;
        assert_eq!(v.next(), ActivityView::Agents);
        assert_eq!(v.next().next(), ActivityView::Logs);
        assert_eq!(v.next().next().next(), ActivityView::Both);
    }

    #[test]
    fn the_log_is_bounded() {
        let mut p = Progress::default();
        for i in 0..500 {
            p.apply(RunnerMsg::Log(format!("line {i}")));
        }
        assert_eq!(p.log.len(), 200);
        assert_eq!(p.log.back().unwrap(), "line 499");
    }

    /// "Aprobar todo": with nothing selected a batch action means the whole
    /// queue — except the candidates that are excluded from batching by design,
    /// which must never be swept along by a convenience key.
    #[test]
    fn approve_all_covers_the_queue_but_never_the_gated_candidates() {
        let mut app = App::new();
        app.candidates = vec![
            candidate("c1", "memory", serde_json::json!({})),
            candidate("c2", "convention", serde_json::json!({})),
            candidate("c3", "harness", serde_json::json!({})),
            candidate(
                "c4",
                "memory",
                serde_json::json!({ "client_attested": "MSA-2026-014" }),
            ),
        ];
        let ids: Vec<&str> = app.batchable().iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["c1", "c2"]);
        assert_eq!(
            app.batch_target(),
            (2, true),
            "no selection means the whole eligible queue"
        );

        app.selected = vec!["c1".into()];
        assert_eq!(
            app.batch_target(),
            (1, false),
            "an explicit selection wins over the whole queue"
        );
    }

    /// Selecting a gated candidate must be refused at the point of selection,
    /// not silently dropped later.
    #[test]
    fn a_gated_candidate_cannot_even_be_selected() {
        let mut app = App::new();
        app.candidates = vec![candidate("c3", "harness", serde_json::json!({}))];
        app.review_cursor = 0;
        app.toggle_selected();
        assert!(app.selected.is_empty());
        assert!(
            app.status.contains("cannot be batch-approved"),
            "{}",
            app.status
        );
    }

    /// A backend call must leave the draw loop free. The observable contract is
    /// that the call returns immediately with a pending marker rather than a
    /// result.
    #[test]
    fn a_backend_call_returns_immediately_and_reports_that_it_is_waiting() {
        let mut app = App::new();
        app.config.api_url = "http://127.0.0.1:1".into(); // nothing listens here
        app.config.api_key = "k".into();
        app.probe();
        assert!(app.is_waiting(), "the call is out, the UI is not");
        assert!(
            app.status.contains("testing the connection"),
            "{}",
            app.status
        );

        // A second call while one is outstanding is refused, not queued: two
        // concurrent writes would race on candidate versions.
        app.load_runs();
        assert!(app.status.starts_with("busy"), "{}", app.status);
    }

    /// A run started from the Run screen must take over the review target;
    /// otherwise the queue still shows the run opened from the picker.
    #[test]
    fn starting_a_run_releases_a_run_opened_from_the_picker() {
        let mut app = App::new();
        app.picked_run = Some("older".into());
        app.candidates = vec![candidate("c1", "memory", serde_json::json!({}))];
        app.config.path = ".".into();
        app.start(true);
        assert_eq!(app.picked_run, None);
        assert!(app.candidates.is_empty());
    }

    /// git-history against a directory that is not a repository is the failure
    /// the operator hit. It is now caught before the run, by name.
    #[test]
    fn a_path_that_cannot_work_is_reported_before_the_run_not_after() {
        let mut app = App::new();
        app.config.source = Source::RepoDocs;
        app.config.path = "/definitely/not/here".into();
        let why: Vec<String> = app.blockers(true).iter().map(|b| b.why.clone()).collect();
        assert!(why.iter().any(|w| w.contains("not a directory")), "{why:?}");

        app.config.source = Source::GitHistory;
        app.config.path = "/tmp".into();
        let why: Vec<String> = app.blockers(true).iter().map(|b| b.why.clone()).collect();
        assert!(why.iter().any(|w| w.contains("no .git")), "{why:?}");

        // The repository this crate lives in must pass both.
        app.config.path = "../../".into();
        assert!(app.blockers(true).is_empty(), "{:?}", app.blockers(true));
    }

    // ── Monorepo plan ─────────────────────────────────────────────────────────

    fn created(alias: &str, run: &str) -> CreatedRun {
        CreatedRun {
            alias: alias.into(),
            project_id: format!("p_{alias}"),
            run_id: run.into(),
        }
    }

    fn plan_row(rel: &str, action: Action) -> PlanRow {
        let name = rel.rsplit('/').next().unwrap().to_string();
        PlanRow {
            detected: crate::monorepo::Detected {
                alias: name.clone(),
                name,
                rel_dir: rel.into(),
                via: "test",
            },
            matched: None,
            action,
            resolved_project_id: None,
        }
    }

    /// The multi-run failure this prevents: after a monorepo run the review
    /// screen must not silently open only the last project's queue.
    #[test]
    fn several_session_runs_force_a_pick_but_one_opens_directly() {
        let mut app = App::new();
        app.progress.run_id = Some("last".into());
        app.session_runs = vec![created("web", "run-web")];
        assert_eq!(
            app.active_run().as_deref(),
            Some("last"),
            "a single project opens without a pick"
        );

        app.session_runs.push(created("api", "run-api"));
        assert!(
            app.picking_run(),
            "two projects means the operator chooses which to review"
        );
        assert!(app.showing_session_runs());
        assert_eq!(app.run_list_len(), 2);
    }

    #[test]
    fn picking_a_session_run_targets_that_projects_queue() {
        let mut app = App::new();
        app.config.api_url = "http://127.0.0.1:1".into();
        app.config.api_key = "k".into();
        app.session_runs = vec![created("web", "run-web"), created("api", "run-api")];
        app.run_cursor = 1;
        app.pick_run();
        assert_eq!(app.picked_run.as_deref(), Some("run-api"));
        assert_eq!(app.active_run().as_deref(), Some("run-api"));
        assert!(!app.picking_run(), "a pick settles the target");
    }

    #[test]
    fn cycling_a_matched_row_offers_select_but_an_unmatched_one_does_not() {
        let mut app = App::new();
        app.plan = vec![plan_row("apps/web", Action::Create)];
        app.plan[0].matched = Some(Project {
            id: "p_web".into(),
            name: "web".into(),
            client_id: None,
            archived_at: None,
        });
        app.cycle_action(); // Create → Skip
        assert_eq!(app.plan[0].action, Action::Skip);
        app.cycle_action(); // Skip → Select (it matched)
        assert_eq!(app.plan[0].action, Action::Select("p_web".into()));
        app.cycle_action(); // Select → Create
        assert_eq!(app.plan[0].action, Action::Create);

        // With no match, Skip goes straight back to Create.
        app.plan[0].matched = None;
        app.cycle_action(); // Create → Skip
        app.cycle_action(); // Skip → Create
        assert_eq!(app.plan[0].action, Action::Create);
    }

    fn resolved_row(rel: &str, action: Action, pid: Option<&str>) -> PlanRow {
        let mut row = plan_row(rel, action);
        row.resolved_project_id = pid.map(str::to_string);
        row
    }

    /// A folder of separate repositories cannot be one routed run — there is no
    /// repository to hold the config — so it becomes one run per repository.
    #[test]
    fn a_folder_of_repositories_becomes_one_queued_run_per_repository() {
        let mut app = App::new();
        app.plan_layout = Layout::RepoCollection;
        app.plan_path = "/estate".into();
        app.config.path = "/estate".into();
        // No credentials: `start` refuses, so this asserts the queue that was
        // built rather than a launched process.
        app.config.api_key.clear();
        app.plan = vec![
            resolved_row("svc-a", Action::Create, Some("p_a")),
            resolved_row("svc-b", Action::Select("p_b".into()), Some("p_b")),
            resolved_row("svc-c", Action::Skip, None),
            resolved_row("svc-d", Action::Create, None), // never resolved
        ];

        app.start_repo_queue();

        let queued: Vec<(String, String)> = app
            .run_queue
            .iter()
            .map(|q| (q.path.clone(), q.project_id.clone()))
            .collect();
        assert_eq!(
            queued,
            vec![
                ("/estate/svc-a".to_string(), "p_a".to_string()),
                ("/estate/svc-b".to_string(), "p_b".to_string()),
            ],
            "skipped and unresolved repositories never enter the queue"
        );
        assert_eq!(app.queue_pos, 0);
        assert!(app.handle.is_none(), "no credentials, so nothing launched");
    }

    /// The queue rewrites path and project per repository; draining it must put
    /// the operator's own configuration back, or the next manual run silently
    /// scans whichever repository happened to be last.
    #[test]
    fn draining_the_queue_restores_the_operators_path_and_project() {
        let mut app = App::new();
        app.config.path = "/estate/svc-b".into();
        app.config.project = "p_b".into();
        app.queue_restore = Some(("/estate".into(), String::new()));
        app.run_queue = vec![QueuedRun {
            alias: "svc-b".into(),
            path: "/estate/svc-b".into(),
            project_id: "p_b".into(),
        }];
        app.queue_pos = 0;

        assert!(!app.advance_queue(), "one entry means the queue is spent");
        assert_eq!(app.config.path, "/estate");
        assert_eq!(app.config.project, "");
        assert!(app.run_queue.is_empty(), "a spent queue frees the next manual run");
    }

    #[test]
    fn a_plan_where_everything_is_skipped_refuses_to_execute() {
        let mut app = App::new();
        app.config.api_url = "http://localhost:8080".into();
        app.config.api_key = "nm_x".into();
        app.config.path = ".".into();
        app.plan = vec![plan_row("apps/web", Action::Skip)];
        app.execute_plan(false);
        assert!(app.handle.is_none(), "nothing was launched");
        assert!(app.status.contains("skipped"), "{}", app.status);
    }

    #[test]
    fn plan_summary_counts_each_action() {
        let mut app = App::new();
        app.plan = vec![
            plan_row("a", Action::Create),
            plan_row("b", Action::Select("p".into())),
            plan_row("c", Action::Skip),
            plan_row("d", Action::Create),
        ];
        assert_eq!(app.plan_summary(), (2, 1, 1));
    }

    /// Detection reports a non-monorepo path as "single project" rather than an
    /// empty screen with no explanation.
    #[test]
    fn a_path_that_is_not_a_git_repo_is_explained_not_left_blank() {
        let mut app = App::new();
        app.config.path = "/".into(); // a directory, but not a Git repository
        app.enter_projects();
        assert!(app.plan.is_empty());
        assert!(app.plan_detected);
        assert!(
            app.plan_note.contains("Git repository"),
            "note: {}",
            app.plan_note
        );
    }

    // ── Source-code source ──────────────────────────────────────────────────────

    #[test]
    fn a_code_source_with_both_actions_off_runs_nothing() {
        let mut app = App::new();
        app.config.source = Source::Code;
        app.config.extract_knowledge = false;
        app.config.index_code = false;
        app.config.path = ".".into();
        app.config.api_url = "http://localhost:8080".into();
        app.config.api_key = "nm_x".into();
        app.start(false);
        assert!(app.handle.is_none(), "the extractor did not run");
        assert!(!app.is_waiting(), "and nothing was indexed");
        assert!(app.status.contains("enable"), "{}", app.status);
    }

    #[test]
    fn a_code_source_with_only_index_indexes_without_running_the_extractor() {
        let mut app = App::new();
        app.config.source = Source::Code;
        app.config.extract_knowledge = false;
        app.config.index_code = true;
        app.config.path = ".".into();
        app.config.api_url = "http://localhost:8080".into();
        app.config.api_key = "nm_x".into();
        app.start(false);
        assert!(app.handle.is_none(), "extraction is off, so no runner");
        assert!(app.is_waiting(), "the index call is in flight");
    }

    /// A preview must never index — indexing is a real, remote write.
    #[test]
    fn a_code_preview_does_not_index() {
        let mut app = App::new();
        app.config.source = Source::Code;
        app.config.extract_knowledge = false;
        app.config.index_code = true;
        app.config.path = ".".into();
        app.start(true);
        assert!(!app.is_waiting(), "a dry run indexes nothing");
        assert!(app.handle.is_none());
    }

    #[test]
    fn starting_a_run_that_cannot_succeed_reports_instead_of_launching() {
        let mut app = App::new();
        app.config.api_key.clear();
        app.config.api_url.clear();
        app.start(false);
        assert!(app.handle.is_none());
        assert!(app.status.contains("cannot start"), "{}", app.status);
        assert_eq!(app.screen, Screen::Connection, "it does not navigate away");
    }
}
