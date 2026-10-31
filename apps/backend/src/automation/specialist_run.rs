//! Runs the steps of a specialist (factory F4, see `factory::specialists`) with
//! the worker's existing machinery.
//!
//! Design (the least invasive one that keeps a real tier split): every step is a
//! separate Claude Code invocation with its own `--model`, inside the same run.
//! The implementation step IS the resolver's normal invocation, unchanged except
//! for its prompt (plan and guidance appended) and model, so the WIP checkpoints,
//! sandbox diffs, evaluator and publishing path stay exactly as they are. The
//! plan and review steps are extra read-only invocations (`--permission-mode
//! plan`, `Read,Grep,Glob`), run locally or in their own sandbox pod (slot
//! `<slot>-plan` / `<slot>-review`, so they never collide with the run's agent
//! pod). The alternative, one invocation with a plan/review "sandwich" in the
//! prompt, cannot change model mid-session and would bill the whole run at the
//! frontier, defeating the pattern. The repair round (see `review_with_repair`)
//! is one more writing invocation in its own pod (slot `<slot>-repair`).

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;

use crate::automation::merge_gate::ChangedFile;
use crate::automation::specialist_tests::SeededFault;
use crate::automation::specialist_ui::{ReviewShots, Screenshot};
use crate::factory::specialists::{self, CheckReport, Specialist, Step, Verdict, Verification};
use crate::store::sqlite::SqliteStore;

/// What a step needs from the run it belongs to.
pub(crate) struct StepEnv<'a> {
    pub store: &'a SqliteStore,
    pub org_id: &'a str,
    pub run_id: &'a str,
    pub attempt_id: &'a str,
    pub workdir: &'a Path,
    pub sandboxed: bool,
    pub pi_eval: bool,
    pub claude_bin: &'a str,
    pub secret_values: &'a [String],
    /// The run's pod slot (`main`, `issue-<n>`); a step's pod uses `<slot>-<step>`.
    pub slot: &'a str,
    /// The run's transcript sequence base; steps write above it.
    pub seq_base: i64,
}

/// Plan and review are short: a bounded budget keeps a stuck judgment step from
/// holding the run.
const READ_ONLY_WALL: Duration = Duration::from_secs(900);
const READ_ONLY_MAX_TURNS: u64 = 40;
/// The review reads the change and the code around it (for tests, the code
/// under test too), so it gets more turns than the plan. The eval judge uses
/// the same budget.
pub(crate) const REVIEW_MAX_TURNS: u64 = 60;

/// Transcript offsets of the steps within the run's range. The resolver's own
/// invocation and its retries use `seq_base + retry * 100_000` (at most 3).
/// The repair round's review runs with `REPAIR_ROUND_SEQ_SHIFT` added, so it
/// lands at 900_000, past the first review.
fn seq_offset(step: Step) -> i64 {
    match step {
        Step::Plan => 700_000,
        Step::Implement => 0,
        Step::Repair => 400_000,
        Step::Review => 800_000,
    }
}

const REPAIR_ROUND_SEQ_SHIFT: i64 = 100_000;

/// Tools of a code-writing step: the resolver's.
pub(crate) const WRITE_TOOLS: &str = "Read,Edit,Write,Grep,Glob,Skill,Task,mcp__plugin_nexusmind_nexusmind__*";

/// Runs one Claude Code invocation and returns its `result` event. `writes`
/// selects the resolver's permissions and, in the sandbox, applies the pod's
/// diff to the checkout; otherwise the step is read-only.
pub(crate) async fn run_step(
    env: &StepEnv<'_>,
    step: Step,
    prompt: &str,
    model: &str,
    writes: bool,
    max_turns: u64,
    wall_time: Duration,
) -> anyhow::Result<Value> {
    if env.pi_eval {
        let mut command = super::pi::eval_command(!writes);
        command.current_dir(env.workdir).kill_on_drop(true);
        let output = tokio::time::timeout(
            wall_time,
            super::worker::run_claude_capturing_transcript(
                &mut command,
                Some(prompt),
                env.store,
                env.org_id,
                env.run_id,
                env.secret_values,
                env.seq_base + seq_offset(step),
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("specialist_step_timed_out"))?
        .map_err(|_| anyhow::anyhow!("pi_spawn_failed"))?;
        let (event, _) = super::pi::parse(&output.stdout)
            .map_err(|_| anyhow::anyhow!("pi_provider_error"))?;
        return Ok(event);
    }
    let mut command = Command::new(env.claude_bin);
    super::worker::restrict_claude_environment(&mut command);
    super::worker::ignore_repository_settings(&mut command);
    let turns = max_turns.to_string();
    let (mode, tools) = if writes { ("acceptEdits", WRITE_TOOLS) } else { ("plan", "Read,Grep,Glob") };
    command.args([
        "-p", prompt, "--output-format", "stream-json", "--verbose", "--max-turns", &turns,
        "--permission-mode", mode, "--allowedTools", tools, "--model", model,
    ]);
    command.current_dir(env.workdir).kill_on_drop(true);
    let seq_base = env.seq_base + seq_offset(step);
    let output = if env.sandboxed {
        let slot = if step == Step::Implement { env.slot.to_string() } else { format!("{}-{}", env.slot, step.as_str()) };
        let receipts = std::sync::Mutex::new(Vec::new());
        let (sink, consumer) = if writes {
            let (sink, consumer) = super::worker::sandbox_write_consumer(env.workdir.to_path_buf(), None);
            (Some(sink), Some(consumer))
        } else {
            (None, None)
        };
        let run = super::sandboxed::run_claude_sandboxed(
            super::sandboxed::SandboxedRun {
                store: env.store,
                org_id: env.org_id,
                run_id: env.run_id,
                attempt_id: env.attempt_id,
                retry: 0,
                workdir: env.workdir,
                secret_values: env.secret_values,
                seq_base,
                wall_time,
                verification: &[],
                receipts: &receipts,
                qa: None,
                slot: &slot,
                writes: sink,
            },
            super::sandboxed::sandbox_invocation(&command),
        );
        let outer = super::sandboxed::sandbox_lifetime(wall_time, 0, false) + super::worker::SANDBOX_OUTER_MARGIN;
        let output = tokio::time::timeout(outer, run)
            .await
            .map_err(|_| anyhow::anyhow!("specialist_step_timed_out"))?
            .map_err(|error| anyhow::anyhow!(super::sandboxed::failure_code(env.run_id, &error)))?;
        if let Some(consumer) = consumer {
            consumer.await.map_err(|_| anyhow::anyhow!("sandbox_failed"))??;
        }
        output
    } else {
        tokio::time::timeout(
            wall_time,
            super::worker::run_claude_capturing_transcript(
                &mut command,
                None,
                env.store,
                env.org_id,
                env.run_id,
                env.secret_values,
                seq_base,
            ),
        )
        .await
        .map_err(|_| anyhow::anyhow!("specialist_step_timed_out"))?
        .map_err(|_| anyhow::anyhow!("claude_spawn_failed"))?
    };
    // A non-zero exit (max turns) can still carry a final result, as for the
    // resolver; no result at all fails the step.
    // A failed request still returns its event, so callers record its cost
    // before checking `result_error`.
    super::worker::parse_claude_event_stream(&output.stdout)
        .map(|(event, _)| event)
        .map_err(|_| anyhow::anyhow!("specialist_{}_failed", step.as_str()))
}

/// The error code of a `result` event that reports a failed request rather
/// than finished work: a usage limit or an API error comes back as
/// `is_error: true` with no cost. Without this a limit reads as "no plan" or
/// "no change", indistinguishable from a model that did nothing. Running out
/// of turns is not an error here: its result is judged like any other. Any
/// other failed subtype (`error_during_execution`) is an error.
pub(crate) fn result_error(event: &Value) -> Option<&'static str> {
    let failed = event.get("is_error").and_then(Value::as_bool) == Some(true)
        && event.get("subtype").and_then(Value::as_str) != Some("error_max_turns");
    if !failed {
        return None;
    }
    let text = event.get("result").and_then(Value::as_str).unwrap_or_default().to_ascii_lowercase();
    Some(if text.contains("limit") { "claude_usage_limited" } else { "claude_api_error" })
}

/// The verdict in a review or judge step's result. An unreadable one rejects
/// (fail closed) and says why: the step ran out of turns before answering, or
/// its answer held no verdict object.
pub(crate) fn verdict_of(event: &Value) -> Verdict {
    if event.get("subtype").and_then(Value::as_str) == Some("error_max_turns") {
        return Verdict { accept: false, reasons: vec!["verdict_unreadable:max_turns".into()] };
    }
    let mut verdict = specialists::parse_verdict(&super::worker::structured_result(event));
    if verdict.reasons == ["verdict_unreadable"] {
        verdict.reasons = vec!["verdict_unreadable:no_json".into()];
    }
    verdict
}

/// The planning step's outcome.
#[derive(Clone, Debug)]
pub(crate) struct Planned {
    pub plan: String,
    /// UI only: the app routes the plan says the change shows on.
    pub routes: Vec<String>,
    pub model: String,
    pub event: Value,
}

/// Runs the frontier planning step. `Err` is a tenant-visible code.
pub(crate) async fn plan(env: &StepEnv<'_>, spec: &Specialist, config: &Value, model: &str) -> Result<Planned, String> {
    let prompt = specialists::plan_prompt(spec, config);
    let event = run_step(env, Step::Plan, &prompt, model, false, READ_ONLY_MAX_TURNS, READ_ONLY_WALL)
        .await
        .map_err(|error| error.to_string())?;
    if let Some(code) = result_error(&event) {
        return Err(code.to_string());
    }
    let structured = super::worker::structured_result(&event);
    let plan = specialists::parse_plan(&structured)?;
    let routes = if spec.verification == Verification::Ui { specialists::plan_routes(&structured) } else { Vec::new() };
    Ok(Planned { plan, routes, model: model.to_string(), event })
}

/// The deterministic checks and the review of a finished change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Finished {
    pub changed: Vec<ChangedFile>,
    /// Whether the current diff and proposed issue title pass the automatic
    /// merge path floor; unknown means no repair is eligible.
    pub path_floor_passed: Option<bool>,
    /// `None` when there was nothing to check (no change).
    pub checks: Option<CheckReport>,
    /// `None` when the review did not run (no change, or the checks failed).
    pub review: Option<Verdict>,
    pub review_model: Option<String>,
    pub review_event: Option<Value>,
    /// UI only: the screenshots the review saw (the eval's judge sees them too).
    pub screenshots: Vec<Screenshot>,
}

/// Every deterministic check of a change, and (UI) its screenshots.
#[derive(Clone, Debug, Default)]
pub(crate) struct Checked {
    pub report: CheckReport,
    pub screenshots: Vec<Screenshot>,
}

impl Finished {
    /// Whether the change may be published.
    pub fn accepted(&self) -> bool {
        self.checks.as_ref().is_none_or(|checks| checks.passed) && self.review.as_ref().is_none_or(|review| review.accept)
    }
}

/// Runs the specialist's checks on the checkout, then (only when they pass, so a
/// failing change never spends a frontier call) the review step.
/// `choose_review_model` is called only if the review runs, so its
/// `model.selected` event and frontier slot are only used when needed.
/// `seeded` are the frozen eval's planted faults (empty in production);
/// `routes` are the app routes a UI change is screenshotted on, besides the
/// ones its fixture file hints (the plan's, and in the eval the task's).
#[allow(clippy::too_many_arguments)]
pub(crate) async fn verify_and_review(
    env: &StepEnv<'_>,
    spec: &'static Specialist,
    config: &Value,
    plan: &str,
    routes: &[String],
    seeded: &[SeededFault],
    choose_review_model: impl FnOnce() -> String,
) -> Result<Finished, String> {
    remove_progress_scratchpad(env.workdir).await?;
    let changed = checkout_changes(env.workdir).await.map_err(|_| "diff_inspection_failed".to_string())?;
    if changed.iter().all(|file| file.filename == "PENDING.md") {
        return Ok(Finished { changed, ..Default::default() });
    }
    let title = config.pointer("/issue/title").and_then(Value::as_str).unwrap_or_default();
    let publishable_paths: Vec<ChangedFile> = changed.iter().filter(|file| file.filename != "PENDING.md").cloned().collect();
    let path_floor_passed = super::merge_gate::auto_merge_path_verdict(&publishable_paths, title).is_ok();
    let Checked { report: checks, screenshots } = run_checks(env, spec, &changed, seeded, routes).await?;
    if !checks.passed {
        return Ok(Finished {
            changed,
            path_floor_passed: Some(path_floor_passed),
            checks: Some(checks),
            screenshots,
            ..Default::default()
        });
    }
    // The change's files and diff are taken before the screenshots land in the
    // checkout, and the screenshots leave it when `shots` drops after the step.
    let diff = checkout_diff(env.workdir, &changed).await.map_err(|_| "diff_inspection_failed".to_string())?;
    let shots = ReviewShots::write(env.workdir, &screenshots).map_err(|_| "ui_screenshots_unwritable".to_string())?;
    let model = choose_review_model();
    let prompt = specialists::review_prompt(spec, config, plan, &diff, &checks.advisories(), &shots.descriptions);
    let event = run_step(env, Step::Review, &prompt, &model, false, REVIEW_MAX_TURNS, READ_ONLY_WALL)
        .await
        .map_err(|error| error.to_string())?;
    drop(shots);
    // A review that failed rejects (fail closed) and keeps its event, so its cost
    // still reaches telemetry.
    let review = match result_error(&event) {
        Some(code) => Verdict { accept: false, reasons: vec![code.to_string()] },
        None => verdict_of(&event),
    };
    Ok(Finished {
        changed,
        path_floor_passed: Some(path_floor_passed),
        checks: Some(checks),
        review: Some(review),
        review_model: Some(model),
        review_event: Some(event),
        screenshots,
    })
}

/// PENDING.md is the resolver's handoff scratchpad, not part of a specialist's
/// requested change. A repair step can recreate it, so remove it before every
/// verification/review round rather than only before the first one.
async fn remove_progress_scratchpad(workdir: &Path) -> Result<(), String> {
    let tracked = Command::new("git")
        .current_dir(workdir)
        .args(["ls-files", "--", "PENDING.md"])
        .output()
        .await
        .map_err(|_| "scratchpad_cleanup_failed".to_string())?;
    if !tracked.status.success() || !tracked.stdout.is_empty() {
        return Err("scratchpad_cleanup_failed".into());
    }
    match tokio::fs::remove_file(workdir.join("PENDING.md")).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("scratchpad_cleanup_failed".into()),
    }
}

/// Every deterministic check of a change: the in-worker checks, then, for a
/// specialist whose checks run the repository's code (tests, UI) and only when
/// the in-worker ones pass, the execution checks in a sandbox commands pod.
/// Both land in one report, so the review step and the eval treat them alike.
pub(crate) async fn run_checks(
    env: &StepEnv<'_>,
    spec: &'static Specialist,
    changed: &[ChangedFile],
    seeded: &[SeededFault],
    routes: &[String],
) -> Result<Checked, String> {
    let report = check_checkout(spec, env.workdir, changed).await?;
    if !report.passed || !spec.verification.runs_repository_code() {
        return Ok(Checked { report, screenshots: Vec::new() });
    }
    let mut checks = report.checks;
    let mut screenshots = Vec::new();
    match spec.verification {
        Verification::Tests => checks.extend(super::specialist_tests::run_checks(env, changed, seeded).await?),
        Verification::Ui => {
            let (ui, images) = super::specialist_ui::run_checks(env, changed, routes).await?;
            checks.extend(ui);
            screenshots = images;
        }
        Verification::Docs => {}
    }
    Ok(Checked { report: CheckReport::from_checks(checks), screenshots })
}

/// Runs the specialist's in-worker checks on a checkout. Indexing reads every
/// file, so it runs on the blocking pool; nothing in the repository executes.
pub(crate) async fn check_checkout(
    spec: &'static Specialist,
    workdir: &Path,
    changed: &[ChangedFile],
) -> Result<CheckReport, String> {
    let mut base = std::collections::HashMap::new();
    for file in changed.iter().filter(|file| file.status != "added") {
        let source = file.previous_filename.as_deref().unwrap_or(&file.filename);
        if let Some(content) = base_content(workdir, source).await {
            base.insert(file.filename.clone(), content);
        }
    }
    if matches!(spec.verification, Verification::Tests | Verification::Ui) {
        // The tests and UI checks read only the changed files and their
        // packages: nothing to index.
        let view = specialists::CheckoutView::unindexed(workdir, base);
        return Ok(specialists::verify(spec, changed, &view));
    }
    let root = workdir.to_path_buf();
    let changed = changed.to_vec();
    tokio::task::spawn_blocking(move || {
        let view = specialists::CheckoutView::index(&root, base);
        specialists::verify(spec, &changed, &view)
    })
    .await
    .map_err(|_| "specialist_checks_failed_to_run".to_string())
}

/// Codes of a review whose request failed (see `result_error`): the review
/// found nothing, so a repair has nothing to fix.
const REVIEW_REQUEST_FAILURES: &[&str] = &["claude_usage_limited", "claude_api_error"];

/// Only checks a change can fix without changing the factory's scope or risk
/// decision may request one repair. Seeded-fault feedback is deliberately never
/// exposed to the agent; when tests did not run, that check is only a derivative
/// failure and is omitted from the repair prompt.
const REPAIRABLE_CHECKS: &[&str] = &[
    "links_resolve",
    "identifiers_exist",
    "tests_pass",
    "mutation_score",
    "ui_build",
    "ui_lint",
    "ui_tests",
    "ui_screenshots",
];

fn check_repair_reasons(checks: &CheckReport) -> Option<Vec<String>> {
    let mut reasons = Vec::new();
    for check in checks.checks.iter().filter(|check| !check.passed && !check.advisory) {
        if check.name == "seeded_fault_caught" {
            if !check.details.is_empty() && check.details.iter().all(|detail| detail.starts_with("not run:")) {
                continue;
            }
            // The task's seeded faults are hidden evaluation data. Never pass
            // their result or location to a model, even as an untrusted hint.
            return None;
        }
        if !REPAIRABLE_CHECKS.contains(&check.name) {
            return None;
        }
        if check.name == "mutation_score" {
            // Don't reveal which generated mutation survived. The threshold
            // failure is enough to direct stronger tests without target leakage.
            reasons.push("mutation_score: mutation checks did not meet the required threshold".to_string());
        } else {
            reasons.push(format!("{}: {}", check.name, check.details.join("; ")));
        }
    }
    (!reasons.is_empty()).then_some(reasons)
}

/// Reasons for one bounded repair: either repairable deterministic findings,
/// or a substantive review rejection after all checks pass. Infrastructure,
/// policy-floor, hidden seeded-fault, and unreadable-review failures stay final.
pub(crate) fn repair_reasons(finished: &Finished) -> Option<Vec<String>> {
    if finished.path_floor_passed != Some(true) {
        return None;
    }
    if let Some(checks) = finished.checks.as_ref().filter(|checks| !checks.passed) {
        return check_repair_reasons(checks);
    }
    if !finished.checks.as_ref().is_some_and(|checks| checks.passed) {
        return None;
    }
    let review = finished.review.as_ref().filter(|review| !review.accept)?;
    let review_failed = review
        .reasons
        .iter()
        .any(|reason| reason.starts_with("verdict_unreadable") || REVIEW_REQUEST_FAILURES.contains(&reason.as_str()));
    (!review_failed && !review.reasons.is_empty()).then(|| review.reasons.clone())
}

/// The repair round's implementation step.
#[derive(Clone, Debug)]
pub(crate) struct RepairStep {
    pub model: String,
    /// `None` when the step returned no result at all.
    pub event: Option<Value>,
    /// The step could not finish (a code); the run stops with it.
    pub error: Option<String>,
}

/// The checks and review of a change, with at most one repair round between
/// two of them.
#[derive(Clone, Debug)]
pub(crate) struct Reviewed {
    /// One round, or two when the first review rejected and the repair ran.
    /// The last round's verdict is final.
    pub rounds: Vec<Finished>,
    pub repair: Option<RepairStep>,
    /// The repair round could not finish (a code): the run stops with it, and
    /// every step that ran still bills.
    pub error: Option<String>,
}

impl Reviewed {
    pub fn single(finished: Finished) -> Self {
        Self { rounds: vec![finished], repair: None, error: None }
    }

    pub fn last(&self) -> &Finished {
        self.rounds.last().expect("a review has at least one round")
    }

    /// What `apply` decides; only the tests ask it directly.
    #[cfg(test)]
    pub fn accepted(&self) -> bool {
        self.error.is_none() && self.last().accepted()
    }

    /// The model steps after the plan and the first implementation, in order:
    /// `(step, model, result event)`.
    pub fn billed(&self) -> Vec<(Step, &str, &Value)> {
        fn review(finished: &Finished) -> Option<(Step, &str, &Value)> {
            let model = finished.review_model.as_deref().unwrap_or_default();
            finished.review_event.as_ref().map(|event| (Step::Review, model, event))
        }
        let mut billed: Vec<(Step, &str, &Value)> = review(&self.rounds[0]).into_iter().collect();
        if let Some(repair) = &self.repair {
            billed.extend(repair.event.as_ref().map(|event| (Step::Repair, repair.model.as_str(), event)));
        }
        billed.extend(self.rounds.iter().skip(1).filter_map(review));
        billed
    }
}

/// The two halves of a specialist's finish, so the repair round is driven the
/// same way in production, in the eval and in tests.
#[allow(async_fn_in_trait)]
pub(crate) trait Rounds {
    /// The checks and review of the change in the checkout; `repaired` is the
    /// round after the repair.
    async fn verify_and_review(&mut self, repaired: bool) -> Result<Finished, String>;
    /// The implementation step once more, told the review's reasons.
    async fn repair(&mut self, reasons: &[String]) -> RepairStep;
}

/// Verifies and reviews the change; when it has repairable check failures or a
/// substantive rejecting review (see `repair_reasons`), implements once more
/// with bounded feedback and repeats all checks and the review. One round only;
/// policy, infrastructure and hidden-eval failures remain final.
pub(crate) async fn review_with_repair(rounds: &mut impl Rounds) -> Result<Reviewed, String> {
    let first = rounds.verify_and_review(false).await?;
    let Some(reasons) = repair_reasons(&first) else {
        return Ok(Reviewed::single(first));
    };
    let repair = rounds.repair(&reasons).await;
    if let Some(code) = repair.error.clone() {
        return Ok(Reviewed { rounds: vec![first], repair: Some(repair), error: Some(code) });
    }
    match rounds.verify_and_review(true).await {
        Ok(mut second) => {
            // A repair that undid the whole change leaves nothing to publish.
            if second.checks.is_none() {
                second.review = Some(Verdict { accept: false, reasons: vec!["repair_left_no_change".into()] });
            }
            Ok(Reviewed { rounds: vec![first, second], repair: Some(repair), error: None })
        }
        Err(code) => Ok(Reviewed { rounds: vec![first], repair: Some(repair), error: Some(code) }),
    }
}

/// The real rounds: the checks and review in this checkout, and the repair as
/// a writing step in its own pod (slot `<slot>-repair`).
pub(crate) struct LiveRounds<'a, 'e, R: Fn() -> String, M: Fn() -> String> {
    pub env: &'a StepEnv<'e>,
    pub spec: &'static Specialist,
    pub config: &'a Value,
    pub plan: &'a str,
    pub routes: &'a [String],
    pub seeded: &'a [SeededFault],
    /// The first implementation's prompt; the repair's extends it.
    pub implementation_prompt: &'a str,
    pub max_turns: u64,
    pub wall_time: Duration,
    /// Called only when a review runs (it records `model.selected` in production).
    pub review_model: R,
    /// Called only when the repair runs.
    pub repair_model: M,
}

impl<R: Fn() -> String, M: Fn() -> String> Rounds for LiveRounds<'_, '_, R, M> {
    async fn verify_and_review(&mut self, repaired: bool) -> Result<Finished, String> {
        let shifted = StepEnv { seq_base: self.env.seq_base + REPAIR_ROUND_SEQ_SHIFT, ..*self.env };
        let env = if repaired { &shifted } else { self.env };
        verify_and_review(env, self.spec, self.config, self.plan, self.routes, self.seeded, || (self.review_model)()).await
    }

    async fn repair(&mut self, reasons: &[String]) -> RepairStep {
        let model = (self.repair_model)();
        let prompt = specialists::repair_prompt(self.implementation_prompt, reasons);
        let failed = |model: String, code: &str| RepairStep { model, event: None, error: Some(code.to_string()) };
        let Ok(baseline) = RepairBaseline::commit(self.env.workdir) else {
            return failed(model, "repair_baseline_failed");
        };
        let result = run_step(self.env, Step::Repair, &prompt, &model, true, self.max_turns, self.wall_time).await;
        let restored = baseline.restore();
        let event = match result {
            Ok(event) => event,
            Err(error) => return failed(model, &super::sandboxed::failure_code(self.env.run_id, &error)),
        };
        let error = result_error(&event)
            .map(str::to_string)
            .or_else(|| restored.is_err().then(|| "repair_baseline_failed".to_string()));
        RepairStep { model, event: Some(event), error }
    }
}

/// The first round's change, committed for the length of the repair step. A
/// sandboxed writing step sends its diff against a baseline the pod built from
/// the checkout as it was (the change included), and the worker applies it
/// after resetting the checkout to `HEAD`; without this commit the first
/// round's change would be wiped and only the repair's edits kept. Restoring
/// moves `HEAD` and the index back and leaves every file as the repair left it,
/// so the change is again the uncommitted diff the publishing path expects.
///
/// Cancellation: every git command here runs synchronously, with no `.await`
/// between spawning it and reaping it, so dropping the run's future can never
/// leave a `git commit` running that lands after the restore. A guard dropped
/// without a restore (the run was cancelled during the repair step) restores
/// synchronously. The restore resets to the recorded original sha (never
/// `HEAD~1`) and then checks that `HEAD` is that sha; when it is not, the
/// repair fails with `repair_baseline_failed` and the run never publishes.
struct RepairBaseline {
    workdir: std::path::PathBuf,
    head: Option<String>,
}

/// Runs git to completion in `workdir`, with no hooks, signing or identity
/// from the repository or the host.
fn git_sync(workdir: &Path, args: &[&str]) -> anyhow::Result<std::process::Output> {
    let output = std::process::Command::new("git")
        .current_dir(workdir)
        .args([
            "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false", "-c", "user.name=nexusmind",
            "-c", "user.email=factory@nexusmind.invalid",
        ])
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()?;
    if !output.status.success() {
        anyhow::bail!("git_{}_failed", args.first().copied().unwrap_or("command"))
    }
    Ok(output)
}

fn git_head(workdir: &Path) -> anyhow::Result<String> {
    Ok(String::from_utf8_lossy(&git_sync(workdir, &["rev-parse", "HEAD"])?.stdout).trim().to_string())
}

/// Puts `HEAD` and the index back on `head`, keeping the files, and proves it.
fn reset_to(workdir: &Path, head: &str) -> anyhow::Result<()> {
    git_sync(workdir, &["reset", "-q", head])?;
    if git_head(workdir)? != head {
        anyhow::bail!("repair_baseline_not_restored")
    }
    Ok(())
}

impl RepairBaseline {
    // Synchronous on purpose (see the type's doc): a git add and commit of a
    // checkout are short, and an await here would make them cancellable.
    fn commit(workdir: &Path) -> anyhow::Result<Self> {
        let head = git_head(workdir)?;
        let baseline = Self { workdir: workdir.to_path_buf(), head: Some(head) };
        // On an error `baseline` drops and puts `HEAD` back.
        git_sync(workdir, &["add", "-A"])?;
        git_sync(workdir, &["commit", "-q", "--no-verify", "--allow-empty", "-m", "specialist repair baseline"])?;
        Ok(baseline)
    }

    fn restore(mut self) -> anyhow::Result<()> {
        let Some(head) = self.head.take() else { return Ok(()) };
        reset_to(&self.workdir, &head)
    }
}

impl Drop for RepairBaseline {
    fn drop(&mut self) {
        if let Some(head) = self.head.take() {
            let _ = reset_to(&self.workdir, &head);
        }
    }
}

/// Applies the specialist's outcome to a resolver outcome: the cost of every
/// other step (plan, reviews, repair) folds into the result event (so telemetry
/// and economics see every step), and a failed check or a rejecting final
/// review blocks publication, as does a repair round that could not finish.
/// Nothing changes for an outcome that did not succeed, beyond the cost folding.
pub(crate) fn apply(
    spec: &Specialist,
    planned: &Planned,
    reviewed: Option<&Reviewed>,
    outcome: &mut (String, Value),
) {
    let mut extra = vec![&planned.event];
    if let Some(reviewed) = reviewed {
        extra.extend(reviewed.billed().into_iter().map(|(_, _, event)| event));
    }
    let finished = reviewed.map(Reviewed::last);
    if let Some(event) = outcome.1.get("result").filter(|r| crate::factory::telemetry::run_result_event(r).is_some()) {
        let combined = specialists::combine_result_events(event, &extra);
        outcome.1["result"] = combined;
    } else if outcome.1.is_object() {
        // No implementation result (the step failed): the plan (and review)
        // still billed, so they become the run's result event, which is the
        // one place telemetry and economics read cost from.
        let (first, rest) = extra.split_first().expect("the plan event is always present");
        outcome.1["result"] = specialists::combine_result_events(first, rest);
    }
    let summary = json!({
        "id": spec.id,
        "plan": {"model": planned.model},
        "checks": finished.and_then(|f| f.checks.clone()),
        "review": finished.and_then(|f| f.review.clone()),
        "review_model": finished.and_then(|f| f.review_model.clone()),
        "screenshots": finished.map(|f| f.screenshots.iter().map(|s| s.file.clone()).collect::<Vec<_>>()).unwrap_or_default(),
        "repair": reviewed.and_then(|r| r.repair.as_ref().map(|repair| json!({
            "model": repair.model,
            "first_review": r.rounds[0].review,
        }))),
    });
    if let Some(code) = reviewed.and_then(|r| r.error.as_deref()) {
        let result = outcome.1.get("result").cloned();
        *outcome = ("blocked_runtime".into(), json!({"code": code, "result": result, "specialist": summary}));
    } else if let Some(finished) = finished.filter(|f| !f.accepted()) {
        let code = if finished.checks.as_ref().is_some_and(|c| !c.passed) {
            "specialist_checks_failed"
        } else {
            "specialist_review_rejected"
        };
        let result = outcome.1.get("result").cloned();
        *outcome = ("blocked_policy".into(), json!({"code": code, "result": result, "specialist": summary}));
    } else if outcome.1.is_object() {
        if let Some(repair) = reviewed.and_then(|r| r.repair.as_ref()).and_then(|r| r.event.as_ref()) {
            publish_repair_summary(outcome, repair);
        }
        outcome.1["specialist"] = summary;
    }
}

/// After an accepted repair, the PR describes the change the repair left: the
/// repair step's `title` and `summary`, each where its structured result has a
/// non-empty one, over the first implementation's. Every other field of the
/// first result (its contract) stays.
fn publish_repair_summary(outcome: &mut (String, Value), repair: &Value) {
    let repaired = super::worker::structured_result(repair);
    let mut published = super::worker::structured_result(&outcome.1);
    if !published.is_object() {
        published = json!({});
    }
    let mut changed = false;
    for field in ["title", "summary"] {
        if let Some(text) = repaired.get(field).and_then(Value::as_str).filter(|t| !t.trim().is_empty()) {
            published[field] = json!(text);
            changed = true;
        }
    }
    if changed && outcome.1.get("result").is_some_and(Value::is_object) {
        outcome.1["result"]["result"] = json!(published.to_string());
    }
}

/// Whether a resolver result is an explicit no-op (an issue comment, no change).
pub(crate) fn is_no_op(outcome: &Value) -> bool {
    super::worker::structured_result(outcome).get("no_op").and_then(Value::as_bool) == Some(true)
}

/// The changed files of a checkout against its `HEAD`, untracked included, in the
/// shape of GitHub's pull files (`added`, `modified`, `removed`, `renamed`).
pub(crate) async fn checkout_changes(workdir: &Path) -> anyhow::Result<Vec<ChangedFile>> {
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["status", "--porcelain=v1", "-z", "--untracked-files=all"])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("git_status_failed")
    }
    Ok(parse_porcelain(&output.stdout))
}

/// Parses `git status --porcelain=v1 -z`: `XY path\0`, and for renames and
/// copies `XY new\0old\0`.
pub(crate) fn parse_porcelain(raw: &[u8]) -> Vec<ChangedFile> {
    let mut entries = raw.split(|b| *b == 0).filter(|e| !e.is_empty());
    let mut files = Vec::new();
    while let Some(entry) = entries.next() {
        if entry.len() < 4 {
            continue;
        }
        let (x, y) = (entry[0], entry[1]);
        let path = String::from_utf8_lossy(&entry[3..]).into_owned();
        let status = match (x, y) {
            (b'?', _) | (b'A', _) => "added",
            (b'D', _) | (_, b'D') => "removed",
            (b'R', _) | (b'C', _) => "renamed",
            _ => "modified",
        };
        let previous_filename = (status == "renamed")
            .then(|| entries.next().map(|old| String::from_utf8_lossy(old).into_owned()))
            .flatten();
        files.push(ChangedFile { filename: path, status: status.into(), previous_filename });
    }
    files
}

async fn base_content(workdir: &Path, path: &str) -> Option<String> {
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["show", &format!("HEAD:{path}")])
        .output()
        .await
        .ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Most bytes of one new file shown in a diff.
const MAX_NEW_FILE_BYTES: usize = 200_000;

/// The change as a unified diff: tracked files from `git diff HEAD`, new files
/// rendered as additions (git does not diff untracked files).
pub(crate) async fn checkout_diff(workdir: &Path, changed: &[ChangedFile]) -> anyhow::Result<String> {
    let output = Command::new("git")
        .current_dir(workdir)
        .args(["diff", "--no-color", "--no-ext-diff", "HEAD"])
        .output()
        .await?;
    if !output.status.success() {
        anyhow::bail!("git_diff_failed")
    }
    let mut diff = String::from_utf8_lossy(&output.stdout).into_owned();
    let tracked: std::collections::HashSet<String> = String::from_utf8_lossy(
        &Command::new("git").current_dir(workdir).args(["diff", "--name-only", "HEAD"]).output().await?.stdout,
    )
    .lines()
    .map(str::to_string)
    .collect();
    for file in changed.iter().filter(|f| f.status == "added" && !tracked.contains(&f.filename)) {
        let Ok(bytes) = tokio::fs::read(workdir.join(&file.filename)).await else { continue };
        let text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_NEW_FILE_BYTES)]).into_owned();
        diff.push_str(&format!("diff --git a/{0} b/{0}\nnew file\n--- /dev/null\n+++ b/{0}\n", file.filename));
        for line in text.lines() {
            diff.push('+');
            diff.push_str(line);
            diff.push('\n');
        }
    }
    Ok(diff)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init_repo(workdir: &Path) {
        assert!(
            std::process::Command::new("git")
                .current_dir(workdir)
                .args(["init", "-q"])
                .status()
                .unwrap()
                .success()
        );
    }

    #[tokio::test]
    async fn verification_removes_recreated_progress_scratchpad() {
        let workdir = tempfile::tempdir().unwrap();
        init_repo(workdir.path());
        let scratchpad = workdir.path().join("PENDING.md");
        tokio::fs::write(&scratchpad, "unfinished plan").await.unwrap();

        remove_progress_scratchpad(workdir.path()).await.unwrap();
        assert!(!scratchpad.exists());
        remove_progress_scratchpad(workdir.path()).await.unwrap();
    }

    #[tokio::test]
    async fn verification_fails_closed_if_progress_scratchpad_is_a_directory() {
        let workdir = tempfile::tempdir().unwrap();
        init_repo(workdir.path());
        tokio::fs::create_dir(workdir.path().join("PENDING.md")).await.unwrap();

        assert_eq!(remove_progress_scratchpad(workdir.path()).await, Err("scratchpad_cleanup_failed".into()));
    }

    #[tokio::test]
    async fn verification_preserves_a_tracked_pending_file() {
        let workdir = tempfile::tempdir().unwrap();
        init_repo(workdir.path());
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .current_dir(workdir.path())
                .args(args)
                .output()
                .unwrap()
        };
        let scratchpad = workdir.path().join("PENDING.md");
        tokio::fs::write(&scratchpad, "tracked user content").await.unwrap();
        assert!(git(&["add", "PENDING.md"]).status.success());

        assert_eq!(remove_progress_scratchpad(workdir.path()).await, Err("scratchpad_cleanup_failed".into()));
        assert_eq!(tokio::fs::read_to_string(scratchpad).await.unwrap(), "tracked user content");
    }

    #[test]
    fn porcelain_entries_become_pull_file_statuses() {
        let raw = b"?? docs/new.md\0 M README.md\0D  old.md\0R  docs/b.md\0docs/a.md\0A  docs/c.md\0";
        let files = parse_porcelain(raw);
        let summary: Vec<(&str, &str, Option<&str>)> = files
            .iter()
            .map(|f| (f.filename.as_str(), f.status.as_str(), f.previous_filename.as_deref()))
            .collect();
        assert_eq!(
            summary,
            [
                ("docs/new.md", "added", None),
                ("README.md", "modified", None),
                ("old.md", "removed", None),
                ("docs/b.md", "renamed", Some("docs/a.md")),
                ("docs/c.md", "added", None),
            ]
        );
    }

    fn planned() -> Planned {
        Planned {
            plan: "p".into(),
            routes: Vec::new(),
            model: "opus".into(),
            event: json!({"type": "result", "result": "{\"plan\":\"p\"}", "total_cost_usd": 0.5}),
        }
    }

    fn succeeded() -> (String, Value) {
        (
            "succeeded".into(),
            json!({"code": "completed", "result": {"type": "result", "result": "{\"title\":\"t\",\"summary\":\"s\"}", "total_cost_usd": 0.1}}),
        )
    }

    #[test]
    fn an_accepted_change_keeps_its_outcome_and_carries_every_steps_cost() {
        let finished = Finished {
            checks: Some(CheckReport { passed: true, checks: vec![] }),
            review: Some(Verdict { accept: true, reasons: vec![] }),
            review_model: Some("opus".into()),
            review_event: Some(json!({"type": "result", "total_cost_usd": 0.4})),
            ..Default::default()
        };
        let mut outcome = succeeded();
        apply(&specialists::DOCS, &planned(), Some(&Reviewed::single(finished)), &mut outcome);
        assert_eq!(outcome.0, "succeeded");
        assert!((outcome.1["result"]["total_cost_usd"].as_f64().unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(outcome.1["specialist"]["id"], "docs");
        // The implementation's output is still what gets evaluated and published.
        assert!(!is_no_op(&outcome.1));
        assert_eq!(super::super::worker::structured_result(&outcome.1)["title"], "t");
    }

    #[test]
    fn failed_checks_or_a_rejecting_review_block_publication_but_keep_the_cost() {
        let rejected = Finished {
            checks: Some(CheckReport { passed: true, checks: vec![] }),
            review: Some(Verdict { accept: false, reasons: vec!["invented field".into()] }),
            review_event: Some(json!({"type": "result", "total_cost_usd": 0.4})),
            ..Default::default()
        };
        let mut outcome = succeeded();
        apply(&specialists::DOCS, &planned(), Some(&Reviewed::single(rejected)), &mut outcome);
        assert_eq!((outcome.0.as_str(), outcome.1["code"].as_str()), ("blocked_policy", Some("specialist_review_rejected")));
        assert_eq!(outcome.1["specialist"]["review"]["reasons"][0], "invented field");
        let metrics = crate::factory::telemetry::run_metrics(&outcome.1);
        assert!((metrics.cost_usd.unwrap() - 1.0).abs() < 1e-9, "a rejected run still costs");

        let failing = Finished { checks: Some(CheckReport { passed: false, checks: vec![] }), ..Default::default() };
        let mut outcome = succeeded();
        apply(&specialists::DOCS, &planned(), Some(&Reviewed::single(failing)), &mut outcome);
        assert_eq!(outcome.1["code"], "specialist_checks_failed");
    }

    /// Rounds scripted in order; records what ran and what the repair was told.
    struct FakeRounds {
        rounds: Vec<Result<Finished, String>>,
        repair: RepairStep,
        calls: Vec<String>,
    }

    impl Rounds for FakeRounds {
        async fn verify_and_review(&mut self, repaired: bool) -> Result<Finished, String> {
            self.calls.push(format!("review(repaired={repaired})"));
            self.rounds.remove(0)
        }
        async fn repair(&mut self, reasons: &[String]) -> RepairStep {
            self.calls.push(format!("repair({})", reasons.join("; ")));
            self.repair.clone()
        }
    }

    fn round(checks_pass: bool, verdict: Option<(bool, &str)>, review_cost: f64) -> Finished {
        Finished {
            changed: vec![ChangedFile { filename: "docs/a.md".into(), status: "added".into(), previous_filename: None }],
            path_floor_passed: Some(true),
            checks: Some(CheckReport { passed: checks_pass, checks: vec![] }),
            review: verdict.map(|(accept, reason)| Verdict {
                accept,
                reasons: if reason.is_empty() { vec![] } else { vec![reason.into()] },
            }),
            review_model: verdict.map(|_| "opus".into()),
            review_event: verdict.map(|_| json!({"type": "result", "total_cost_usd": review_cost})),
            ..Default::default()
        }
    }

    fn failed_check(name: &'static str, details: &[&str]) -> specialists::CheckOutcome {
        specialists::CheckOutcome {
            name,
            passed: false,
            advisory: false,
            details: details.iter().map(|detail| (*detail).to_string()).collect(),
        }
    }

    fn repair_step(error: Option<&str>) -> RepairStep {
        RepairStep {
            model: "sonnet".into(),
            event: Some(json!({"type": "result", "result": "{\"title\":\"t2\"}", "total_cost_usd": 0.2})),
            error: error.map(str::to_string),
        }
    }

    async fn drive(rounds: Vec<Result<Finished, String>>, repair: RepairStep) -> (Result<Reviewed, String>, Vec<String>) {
        let mut fake = FakeRounds { rounds, repair, calls: Vec::new() };
        let reviewed = review_with_repair(&mut fake).await;
        (reviewed, fake.calls)
    }

    fn applied(reviewed: &Reviewed) -> (String, Value) {
        let mut outcome = succeeded();
        apply(&specialists::DOCS, &planned(), Some(reviewed), &mut outcome);
        outcome
    }

    fn cost(outcome: &(String, Value)) -> f64 {
        crate::factory::telemetry::run_metrics(&outcome.1).cost_usd.unwrap()
    }

    #[tokio::test]
    async fn an_accepted_first_round_needs_no_repair() {
        let (reviewed, calls) = drive(vec![Ok(round(true, Some((true, "")), 0.4))], repair_step(None)).await;
        let reviewed = reviewed.unwrap();
        assert_eq!(calls, ["review(repaired=false)"]);
        assert!(reviewed.accepted() && reviewed.repair.is_none());
        let outcome = applied(&reviewed);
        assert_eq!(outcome.0, "succeeded");
        assert!(outcome.1["specialist"]["repair"].is_null());
        assert!((cost(&outcome) - 1.0).abs() < 1e-9, "implement 0.1 + plan 0.5 + review 0.4");
    }

    #[tokio::test]
    async fn a_rejected_change_is_repaired_once_and_the_second_verdict_accepts_it() {
        let (reviewed, calls) = drive(
            vec![Ok(round(true, Some((false, "docs/a.md:3 invents `run_fast`")), 0.4)), Ok(round(true, Some((true, "")), 0.3))],
            repair_step(None),
        )
        .await;
        let reviewed = reviewed.unwrap();
        assert_eq!(calls, ["review(repaired=false)", "repair(docs/a.md:3 invents `run_fast`)", "review(repaired=true)"]);
        assert!(reviewed.accepted());
        let steps: Vec<(Step, &str)> = reviewed.billed().iter().map(|(step, model, _)| (*step, *model)).collect();
        assert_eq!(steps, [(Step::Review, "opus"), (Step::Repair, "sonnet"), (Step::Review, "opus")]);
        let outcome = applied(&reviewed);
        assert_eq!(outcome.0, "succeeded");
        assert!((cost(&outcome) - 1.5).abs() < 1e-9, "implement, plan, both reviews and the repair all bill");
        assert_eq!(outcome.1["specialist"]["repair"]["model"], "sonnet");
        assert_eq!(outcome.1["specialist"]["repair"]["first_review"]["reasons"][0], "docs/a.md:3 invents `run_fast`");
        assert_eq!(outcome.1["specialist"]["review"]["accept"], true);
        // The PR describes the repaired change: the repair's title, and round 1's
        // summary where the repair gave none.
        let published = crate::automation::worker::structured_result(&outcome.1);
        assert_eq!((published["title"].as_str(), published["summary"].as_str()), (Some("t2"), Some("s")));
    }

    #[tokio::test]
    async fn a_second_rejection_is_final() {
        let (reviewed, calls) = drive(
            vec![Ok(round(true, Some((false, "wrong field")), 0.4)), Ok(round(true, Some((false, "still wrong")), 0.3))],
            repair_step(None),
        )
        .await;
        let reviewed = reviewed.unwrap();
        assert_eq!(calls.len(), 3, "one repair round only: {calls:?}");
        assert!(!reviewed.accepted());
        let outcome = applied(&reviewed);
        assert_eq!((outcome.0.as_str(), outcome.1["code"].as_str()), ("blocked_policy", Some("specialist_review_rejected")));
        assert_eq!(outcome.1["specialist"]["review"]["reasons"][0], "still wrong");
        assert_eq!(crate::automation::worker::structured_result(&outcome.1["result"])["title"], "t");
        assert!((cost(&outcome) - 1.5).abs() < 1e-9);
    }

    #[tokio::test]
    async fn checks_failing_after_the_repair_block_without_a_second_review() {
        let (reviewed, calls) =
            drive(vec![Ok(round(true, Some((false, "wrong field")), 0.4)), Ok(round(false, None, 0.0))], repair_step(None)).await;
        let reviewed = reviewed.unwrap();
        assert_eq!(calls.len(), 3);
        let outcome = applied(&reviewed);
        assert_eq!(outcome.1["code"], "specialist_checks_failed");
        assert!((cost(&outcome) - 1.2).abs() < 1e-9, "the first review and the repair still bill");
    }

    #[tokio::test]
    async fn failing_checks_or_a_review_that_said_nothing_get_no_repair() {
        for first in [
            round(false, None, 0.0),
            round(true, Some((false, "verdict_unreadable:max_turns")), 0.4),
            round(true, Some((false, "verdict_unreadable:no_json")), 0.4),
            round(true, Some((false, "claude_usage_limited")), 0.0),
            round(true, Some((false, "claude_api_error")), 0.0),
            round(true, Some((false, "")), 0.4),
        ] {
            let (reviewed, calls) = drive(vec![Ok(first.clone())], repair_step(None)).await;
            assert_eq!(calls, ["review(repaired=false)"], "{first:?}");
            assert!(!reviewed.unwrap().accepted());
        }
        // A first round that could not run is an error, as before.
        let (reviewed, calls) = drive(vec![Err("checks_install_failed".into())], repair_step(None)).await;
        assert_eq!((reviewed.unwrap_err().as_str(), calls.len()), ("checks_install_failed", 1));
    }

    #[tokio::test]
    async fn repairable_check_failures_get_one_safe_feedback_round() {
        let first = Finished {
            path_floor_passed: Some(true),
            checks: Some(CheckReport::from_checks(vec![
                failed_check("tests_pass", &["vitest failed: expected 2 to be 3"]),
                failed_check("mutation_score", &["killed 1 of 3 mutants", "survived: src/secret.ts:1 `<` -> `>=`"]),
                failed_check("seeded_fault_caught", &["not run: the changed tests did not pass"]),
            ])),
            ..Default::default()
        };
        let findings = repair_reasons(&first).expect("repairable checks produce findings");
        assert!(findings.iter().any(|finding| finding.contains("vitest failed")));
        assert!(findings.iter().any(|finding| finding == "mutation_score: mutation checks did not meet the required threshold"));
        assert!(findings.iter().all(|finding| !finding.contains("secret.ts") && !finding.contains("seeded_fault_caught")));

        let (reviewed, calls) = drive(vec![Ok(first), Ok(round(true, Some((true, "")), 0.3))], repair_step(None)).await;
        let reviewed = reviewed.unwrap();
        assert!(reviewed.accepted());
        assert_eq!(calls.len(), 3);
        assert!(calls[1].starts_with("repair("));
        assert!(!calls[1].contains("seeded_fault_caught") && !calls[1].contains("secret.ts"), "{calls:?}");
        assert_eq!(
            reviewed.billed().iter().map(|(step, _, _)| *step).collect::<Vec<_>>(),
            vec![Step::Repair, Step::Review]
        );
    }

    #[tokio::test]
    async fn seeded_faults_and_scope_floor_findings_never_trigger_repair() {
        for check in [
            failed_check("seeded_fault_caught", &["not caught: apps/admin/src/hidden.ts"]),
            failed_check("ui_paths_only", &["changed a file outside src/"]),
        ] {
            let first = Finished {
                path_floor_passed: Some(true),
                checks: Some(CheckReport::from_checks(vec![check])),
                ..Default::default()
            };
            assert!(repair_reasons(&first).is_none());
            let (reviewed, calls) = drive(vec![Ok(first)], repair_step(None)).await;
            assert!(!reviewed.unwrap().accepted());
            assert_eq!(calls, ["review(repaired=false)"]);
        }
        let risky = Finished {
            path_floor_passed: Some(false),
            checks: Some(CheckReport::from_checks(vec![failed_check("tests_pass", &["test failed"])])),
            ..Default::default()
        };
        assert!(repair_reasons(&risky).is_none());
    }

    #[tokio::test]
    async fn a_repair_that_cannot_finish_stops_the_run_but_bills_what_ran() {
        let (reviewed, calls) =
            drive(vec![Ok(round(true, Some((false, "wrong field")), 0.4))], repair_step(Some("claude_usage_limited"))).await;
        let reviewed = reviewed.unwrap();
        assert_eq!(calls.len(), 2, "no second review after a failed repair");
        let outcome = applied(&reviewed);
        assert_eq!((outcome.0.as_str(), outcome.1["code"].as_str()), ("blocked_runtime", Some("claude_usage_limited")));
        assert!((cost(&outcome) - 1.2).abs() < 1e-9);
        // The second round's checks could not run (an infrastructure failure).
        let (reviewed, _) = drive(
            vec![Ok(round(true, Some((false, "wrong field")), 0.4)), Err("checks_install_failed".into())],
            repair_step(None),
        )
        .await;
        assert_eq!(applied(&reviewed.unwrap()).1["code"], "checks_install_failed");
        // A repair that removed the whole change is not accepted.
        let empty = Finished { changed: vec![], ..Default::default() };
        let (reviewed, _) = drive(vec![Ok(round(true, Some((false, "wrong field")), 0.4)), Ok(empty)], repair_step(None)).await;
        assert_eq!(applied(&reviewed.unwrap()).1["specialist"]["review"]["reasons"][0], "repair_left_no_change");
    }

    #[tokio::test]
    async fn the_repair_baseline_keeps_the_first_rounds_change_under_a_reset_to_head() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git").current_dir(dir.path()).args(args).status().unwrap();
            assert!(status.success(), "{args:?}");
        };
        git(&["init", "-q"]);
        std::fs::write(dir.path().join("README.md"), "# Old\n").unwrap();
        git(&["add", "README.md"]);
        git(&["-c", "user.email=a@b", "-c", "user.name=a", "commit", "-q", "-m", "base"]);
        let head = |dir: &Path| {
            String::from_utf8(std::process::Command::new("git").current_dir(dir).args(["rev-parse", "HEAD"]).output().unwrap().stdout)
                .unwrap()
        };
        let base = head(dir.path());
        // The first round's change: a modified and a new file.
        std::fs::write(dir.path().join("README.md"), "# New\n").unwrap();
        std::fs::write(dir.path().join("guide.md"), "first\n").unwrap();
        let baseline = RepairBaseline::commit(dir.path()).unwrap();
        // What the sandbox write consumer does with the repair's diff.
        crate::factory::workspace::reset_and_apply(
            dir.path(),
            b"diff --git a/guide.md b/guide.md\n--- a/guide.md\n+++ b/guide.md\n@@ -1 +1 @@\n-first\n+repaired\n",
        )
        .await
        .unwrap();
        baseline.restore().unwrap();
        assert_eq!(head(dir.path()), base, "HEAD is back at the base");
        assert_eq!(std::fs::read_to_string(dir.path().join("README.md")).unwrap(), "# New\n");
        assert_eq!(std::fs::read_to_string(dir.path().join("guide.md")).unwrap(), "repaired\n");
        let changed: Vec<(String, String)> =
            checkout_changes(dir.path()).await.unwrap().into_iter().map(|f| (f.filename, f.status)).collect();
        assert_eq!(changed, [("README.md".into(), "modified".into()), ("guide.md".into(), "added".into())]);
        // Dropped without a restore (a cancelled run), it still puts HEAD back.
        let dropped = RepairBaseline::commit(dir.path()).unwrap();
        drop(dropped);
        assert_eq!(head(dir.path()), base);
        // A commit that lands after the baseline's (a late writer) does not
        // survive the restore: it resets to the recorded sha, not `HEAD~1`.
        let late = RepairBaseline::commit(dir.path()).unwrap();
        git(&["-c", "user.email=a@b", "-c", "user.name=a", "commit", "-q", "--allow-empty", "-m", "late"]);
        late.restore().unwrap();
        assert_eq!(head(dir.path()), base);
        assert_eq!(std::fs::read_to_string(dir.path().join("guide.md")).unwrap(), "repaired\n");
        // A restore that cannot put HEAD back is an error, so the run fails closed.
        let lost = RepairBaseline { workdir: dir.path().to_path_buf(), head: Some("0".repeat(40)) };
        assert!(lost.restore().is_err());
        assert_eq!(head(dir.path()), base, "a failed restore leaves HEAD where it was");
    }

    #[test]
    fn a_failed_implementation_still_reports_the_plan_cost() {
        let mut outcome = ("failed".to_string(), json!({"code": "claude_failed"}));
        apply(&specialists::DOCS, &planned(), None, &mut outcome);
        assert_eq!(outcome.0, "failed");
        // The plan's cost becomes the run's result, which telemetry reads.
        assert_eq!(crate::factory::telemetry::run_metrics(&outcome.1).cost_usd, Some(0.5));
    }

    #[tokio::test]
    async fn checkout_changes_and_diff_cover_new_and_modified_files() {
        let dir = tempfile::tempdir().unwrap();
        let git = |args: &[&str]| {
            let status = std::process::Command::new("git").current_dir(dir.path()).args(args).status().unwrap();
            assert!(status.success(), "{args:?}");
        };
        git(&["init", "-q"]);
        git(&["-c", "user.email=a@b", "-c", "user.name=a", "commit", "-q", "--allow-empty", "-m", "base"]);
        std::fs::write(dir.path().join("README.md"), "# Old\n").unwrap();
        git(&["add", "README.md"]);
        git(&["-c", "user.email=a@b", "-c", "user.name=a", "commit", "-q", "-m", "readme"]);
        std::fs::write(dir.path().join("README.md"), "# New\n").unwrap();
        std::fs::create_dir_all(dir.path().join("docs")).unwrap();
        std::fs::write(dir.path().join("docs/guide.md"), "# Guide\nhello\n").unwrap();
        let changed = checkout_changes(dir.path()).await.unwrap();
        assert_eq!(changed.len(), 2);
        let diff = checkout_diff(dir.path(), &changed).await.unwrap();
        assert!(diff.contains("-# Old") && diff.contains("+# New"), "{diff}");
        assert!(diff.contains("+++ b/docs/guide.md\n+# Guide\n+hello"), "{diff}");
        assert_eq!(base_content(dir.path(), "README.md").await.as_deref(), Some("# Old\n"));
        let report = check_checkout(&specialists::DOCS, dir.path(), &changed).await.unwrap();
        assert!(report.passed, "{:?}", report.failures());
    }

    #[test]
    fn failed_requests_become_error_codes_but_running_out_of_turns_does_not() {
        let limited = json!({"type": "result", "subtype": "success", "is_error": true, "result": "You've hit your limit · resets 3am"});
        assert_eq!(result_error(&limited), Some("claude_usage_limited"));
        let api = json!({"type": "result", "subtype": "success", "is_error": true, "result": "API Error: 500"});
        assert_eq!(result_error(&api), Some("claude_api_error"));
        let turns = json!({"type": "result", "subtype": "error_max_turns", "is_error": true});
        assert_eq!(result_error(&turns), None);
        let ok = json!({"type": "result", "subtype": "success", "is_error": false, "result": "rate limit docs updated"});
        assert_eq!(result_error(&ok), None);
    }

    #[test]
    fn an_unreadable_verdict_says_why() {
        let out_of_turns = json!({"type": "result", "subtype": "error_max_turns", "is_error": true});
        assert_eq!(verdict_of(&out_of_turns).reasons, ["verdict_unreadable:max_turns"]);
        let prose = json!({"type": "result", "subtype": "success", "result": "I looked at the change and it is fine."});
        assert_eq!(verdict_of(&prose).reasons, ["verdict_unreadable:no_json"]);
        let wrapped = json!({"type": "result", "subtype": "success",
            "result": "The test uses `{}` as a fixture.\n{\"verdict\":\"reject\",\"reasons\":[\"a.test.ts:3 asserts nothing\"]}\nDone."});
        let verdict = verdict_of(&wrapped);
        assert!(!verdict.accept && verdict.reasons == ["a.test.ts:3 asserts nothing"], "{verdict:?}");
    }
}
