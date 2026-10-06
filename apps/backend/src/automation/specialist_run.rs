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
//! frontier, defeating the pattern.

use std::path::Path;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::process::Command;

use crate::automation::merge_gate::ChangedFile;
use crate::factory::specialists::{self, CheckReport, Specialist, Step, Verdict};
use crate::store::sqlite::SqliteStore;

/// What a step needs from the run it belongs to.
pub(crate) struct StepEnv<'a> {
    pub store: &'a SqliteStore,
    pub org_id: &'a str,
    pub run_id: &'a str,
    pub attempt_id: &'a str,
    pub workdir: &'a Path,
    pub sandboxed: bool,
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

/// Transcript offsets of the steps within the run's range. The resolver's own
/// invocation and its retries use `seq_base + retry * 100_000` (at most 3).
fn seq_offset(step: Step) -> i64 {
    match step {
        Step::Plan => 700_000,
        Step::Implement => 0,
        Step::Review => 800_000,
    }
}

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
    super::worker::parse_claude_event_stream(&output.stdout)
        .map(|(event, _)| event)
        .map_err(|_| anyhow::anyhow!("specialist_{}_failed", step.as_str()))
}

/// The planning step's outcome.
#[derive(Clone, Debug)]
pub(crate) struct Planned {
    pub plan: String,
    pub model: String,
    pub event: Value,
}

/// Runs the frontier planning step. `Err` is a tenant-visible code.
pub(crate) async fn plan(env: &StepEnv<'_>, spec: &Specialist, config: &Value, model: &str) -> Result<Planned, String> {
    let prompt = specialists::plan_prompt(spec, config);
    let event = run_step(env, Step::Plan, &prompt, model, false, READ_ONLY_MAX_TURNS, READ_ONLY_WALL)
        .await
        .map_err(|error| error.to_string())?;
    let plan = specialists::parse_plan(&super::worker::structured_result(&event))?;
    Ok(Planned { plan, model: model.to_string(), event })
}

/// The deterministic checks and the review of a finished change.
#[derive(Clone, Debug, Default)]
pub(crate) struct Finished {
    pub changed: Vec<ChangedFile>,
    /// `None` when there was nothing to check (no change).
    pub checks: Option<CheckReport>,
    /// `None` when the review did not run (no change, or the checks failed).
    pub review: Option<Verdict>,
    pub review_model: Option<String>,
    pub review_event: Option<Value>,
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
pub(crate) async fn verify_and_review(
    env: &StepEnv<'_>,
    spec: &'static Specialist,
    config: &Value,
    plan: &str,
    choose_review_model: impl FnOnce() -> String,
) -> Result<Finished, String> {
    let changed = checkout_changes(env.workdir).await.map_err(|_| "diff_inspection_failed".to_string())?;
    if changed.iter().all(|file| file.filename == "PENDING.md") {
        return Ok(Finished { changed, ..Default::default() });
    }
    let checks = check_checkout(spec, env.workdir, &changed).await?;
    if !checks.passed {
        return Ok(Finished { changed, checks: Some(checks), ..Default::default() });
    }
    let diff = checkout_diff(env.workdir, &changed).await.map_err(|_| "diff_inspection_failed".to_string())?;
    let model = choose_review_model();
    let prompt = specialists::review_prompt(spec, config, plan, &diff, &checks.advisories());
    let event = run_step(env, Step::Review, &prompt, &model, false, READ_ONLY_MAX_TURNS, READ_ONLY_WALL)
        .await
        .map_err(|error| error.to_string())?;
    let review = specialists::parse_verdict(&super::worker::structured_result(&event));
    Ok(Finished { changed, checks: Some(checks), review: Some(review), review_model: Some(model), review_event: Some(event) })
}

/// Runs the specialist's deterministic checks on a checkout. Indexing reads every
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
    let root = workdir.to_path_buf();
    let changed = changed.to_vec();
    tokio::task::spawn_blocking(move || {
        let view = specialists::CheckoutView::index(&root, base);
        specialists::verify(spec, &changed, &view)
    })
    .await
    .map_err(|_| "specialist_checks_failed_to_run".to_string())
}

/// Applies the specialist's outcome to a resolver outcome: the plan and review
/// costs fold into the result event (so telemetry and economics see every step),
/// and a failed check or a rejecting review blocks publication. Nothing changes
/// for an outcome that did not succeed, beyond the cost folding.
pub(crate) fn apply(
    spec: &Specialist,
    planned: &Planned,
    finished: Option<&Finished>,
    outcome: &mut (String, Value),
) {
    let mut extra = vec![&planned.event];
    if let Some(event) = finished.and_then(|f| f.review_event.as_ref()) {
        extra.push(event);
    }
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
    });
    if let Some(finished) = finished.filter(|f| !f.accepted()) {
        let code = if finished.checks.as_ref().is_some_and(|c| !c.passed) {
            "specialist_checks_failed"
        } else {
            "specialist_review_rejected"
        };
        let result = outcome.1.get("result").cloned();
        *outcome = ("blocked_policy".into(), json!({"code": code, "result": result, "specialist": summary}));
    } else if outcome.1.is_object() {
        outcome.1["specialist"] = summary;
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
        apply(&specialists::DOCS, &planned(), Some(&finished), &mut outcome);
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
        apply(&specialists::DOCS, &planned(), Some(&rejected), &mut outcome);
        assert_eq!((outcome.0.as_str(), outcome.1["code"].as_str()), ("blocked_policy", Some("specialist_review_rejected")));
        assert_eq!(outcome.1["specialist"]["review"]["reasons"][0], "invented field");
        let metrics = crate::factory::telemetry::run_metrics(&outcome.1);
        assert!((metrics.cost_usd.unwrap() - 1.0).abs() < 1e-9, "a rejected run still costs");

        let failing = Finished { checks: Some(CheckReport { passed: false, checks: vec![] }), ..Default::default() };
        let mut outcome = succeeded();
        apply(&specialists::DOCS, &planned(), Some(&failing), &mut outcome);
        assert_eq!(outcome.1["code"], "specialist_checks_failed");
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
}
