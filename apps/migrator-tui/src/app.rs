//! Application state: the screens, the fields, and how a stream of runner
//! events becomes something worth looking at.
//!
//! Nothing here draws. `ui.rs` renders this state and never mutates it, which
//! is what lets every rule in `config.rs` be tested without a terminal.

use crate::api::{Candidate, Client, CommitResponse, ReviewResponse, Run, Verdict};
use crate::config::{Blocker, RunConfig, Source, Warning};
use crate::mascot::{self, Graphics, Mascot, Mood};
use crate::protocol::{ParsedLine, RunEvent};
use crate::runner::{resolve_binary, RunHandle, RunnerMsg};
use std::collections::{BTreeMap, VecDeque};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Connection,
    Source,
    Options,
    Running,
    Review,
    Summary,
}

impl Screen {
    /// The stage of the migration pipeline this screen belongs to, used to
    /// light up the diagram in the header.
    pub fn stage(self) -> usize {
        match self {
            Screen::Connection | Screen::Source | Screen::Options => 0,
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
    ClaudeBin,
}

impl FieldId {
    pub fn kind(self) -> FieldKind {
        use FieldId::*;
        match self {
            ApiKey => FieldKind::Secret,
            IncludeSdd | HostScope | Supabase | IncludeData | RedactPii | NoLlm => {
                FieldKind::Toggle
            }
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
            ClaudeBin => "claude binary",
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
            ClaudeBin => "The headless classifier invoked as `claude -p`.",
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
            ClaudeBin => c.claude_bin.clone(),
            IncludeSdd => c.include_sdd.to_string(),
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
            ClaudeBin => &mut c.claude_bin,
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
            HostScope => c.host_scope = !c.host_scope,
            Supabase => c.supabase = !c.supabase,
            RedactPii => c.redact_pii = !c.redact_pii,
            NoLlm => c.no_llm = !c.no_llm,
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
                Source::DbSchema => {
                    f.push(Supabase);
                    f.push(IncludeData);
                    f.extend([Tables, SampleLimit, RedactPii, Attest]);
                }
            }
            f.extend([NoLlm, MaxTokens, ClaudeBin]);
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
        MaxTokens | ClaudeBin => !c.no_llm,
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
    pub staged: Option<(usize, usize, usize)>,
    pub finished: Option<FinishedRun>,
    pub unknown_events: usize,
    pub log: VecDeque<String>,
    /// Recent exchanges with the model. Bounded like the log: a 3000-unit run
    /// would otherwise hold every prompt it ever sent.
    pub agents: VecDeque<AgentExchange>,
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
            RunEvent::RunCreated { alias, run_id, .. } => {
                self.note(format!("· created run {run_id} for {alias}"));
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
            RunEvent::Classifying {
                index,
                total,
                origin,
            } => {
                self.current = index.saturating_sub(1);
                self.total = total;
                self.current_origin = origin;
            }
            RunEvent::Classified {
                index,
                total,
                destination_kind,
                via,
                tokens_spent,
                ..
            } => {
                self.current = index;
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
    Failed(String),
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
            api_rx: None,
            pending: None,
            picked_run: None,
            conflicts: Vec::new(),
            activity: ActivityView::Both,
            agent_cursor: None,
            show_help: false,
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
    }

    /// Starts a run. Refuses rather than launching something that will fail.
    pub fn start(&mut self, dry_run: bool) {
        if self.is_running() {
            self.status = "a run is already in flight — press x to stop it".into();
            return;
        }
        let blockers = self.blockers(dry_run);
        if let Some(first) = blockers.first() {
            self.status = format!("cannot start: {}", first.why);
            return;
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
        if !was_done && self.progress.finished.is_some() {
            self.goto(Screen::Summary);
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
            ApiMsg::Probed(Ok(m)) => self.status = m,
            ApiMsg::Probed(Err(e)) => self.status = format!("✗ {e}"),
            ApiMsg::Runs(Ok(runs)) => {
                self.status = if runs.is_empty() {
                    "no migration runs on this backend yet".into()
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
            ApiMsg::Cancelled(Ok(n)) => {
                self.status = format!("{n} pending candidate(s) cancelled");
                self.candidates.clear();
                self.load_runs();
            }
            ApiMsg::Cancelled(Err(e)) => self.status = format!("cancel failed: {e}"),
        }
    }

    /// Which run the review screen is acting on.
    ///
    /// A run picked from the list wins over the one this session produced, so
    /// an operator can come back the next morning and finish a queue they left
    /// half-reviewed — the queue lives in the backend, not in this process.
    pub fn active_run(&self) -> Option<String> {
        self.picked_run
            .clone()
            .or_else(|| self.progress.run_id.clone())
    }

    /// True when the review screen has nothing to act on and should offer the
    /// list of runs instead of an empty queue.
    pub fn picking_run(&self) -> bool {
        self.active_run().is_none()
    }

    pub fn load_runs(&mut self) {
        self.spawn_api("listing runs", |c| {
            ApiMsg::Runs(c.runs().map_err(|e| e.to_string()))
        });
    }

    pub fn pick_run(&mut self) {
        let Some(run) = self.runs.get(self.run_cursor) else {
            return;
        };
        self.picked_run = Some(run.id.clone());
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

    pub fn commit(&mut self) {
        let Some(run_id) = self.active_run() else {
            return;
        };
        self.spawn_api("committing", move |c| {
            ApiMsg::Committed(c.commit(&run_id).map_err(|e| e.to_string()))
        });
    }

    pub fn probe(&mut self) {
        self.spawn_api("testing the connection", |c| {
            ApiMsg::Probed(c.probe().map_err(|e| e.to_string()))
        });
    }
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
