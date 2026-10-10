//! The tests specialist's execution checks (factory F4, ADR 94ced31c): the
//! changed tests pass, and they catch most of a set of deterministic mutants of
//! the code they import (and, in the frozen eval, the task's seeded fault).
//!
//! Everything that runs repository code runs in ONE sandbox commands pod, never
//! in the worker. The worker only plans: it finds each test's package, package
//! manager and runner, builds the mutants (`factory::mutation`), and ships the
//! mutated files into the pod with the command list:
//!
//! 1. install, once per lockfile directory (`npm ci`, `pnpm install
//!    --frozen-lockfile`, `yarn install --frozen-lockfile`);
//! 2. run the changed tests, once per package; a pass leaves a marker file;
//! 3. per mutant (and seeded fault): copy the mutated file over the original,
//!    rerun the same tests under a timeout, copy the original back, and exit 0
//!    only if the tests still passed. Without the marker a mutant does not run.
//!
//! Commands get their arguments as `sh -c` positional parameters, never spliced
//! into the script, so no path needs quoting. Results come only from exit codes.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::automation::merge_gate::ChangedFile;
use crate::factory::mutation::{self, Mutant, SourceTree};
use crate::factory::sandbox::WORKSPACE;
use crate::factory::sandbox_exec::CommandRun;
use crate::factory::specialists::CheckOutcome;

/// A fault planted in the code for the frozen eval (`EvalTask::seeded_faults`):
/// `find` must occur exactly once in `path`, and is replaced by `replace`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SeededFault {
    pub path: String,
    pub find: String,
    pub replace: String,
}

impl SeededFault {
    /// Checks the fault on its own, before any checkout exists.
    pub fn validate(&self) -> Result<(), String> {
        let plain_path = mutation::normalize(&self.path).is_some_and(|path| path == self.path);
        if !plain_path || mutation::is_js_test_file(&self.path) {
            return Err("seeded_fault_invalid_path".into());
        }
        if self.find.is_empty() || self.find == self.replace {
            return Err("seeded_fault_invalid_text".into());
        }
        Ok(())
    }

    /// The file with the fault applied. `find` must occur exactly once, so the
    /// fault is the one the task author meant.
    pub fn apply(&self, content: &str) -> Result<String, String> {
        match content.matches(&self.find).count() {
            1 => Ok(content.replacen(&self.find, &self.replace, 1)),
            0 => Err("seeded_fault_not_found".into()),
            _ => Err("seeded_fault_ambiguous".into()),
        }
    }

    pub fn describe(&self) -> String {
        format!("{}: `{}` -> `{}`", self.path, self.find, self.replace)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageManager {
    Npm,
    Pnpm,
    Yarn,
}

/// Lockfiles in the order they win when a directory has several.
const LOCKFILES: &[(&str, PackageManager)] = &[
    ("package-lock.json", PackageManager::Npm),
    ("pnpm-lock.yaml", PackageManager::Pnpm),
    ("yarn.lock", PackageManager::Yarn),
];

impl PackageManager {
    pub(crate) fn install(self) -> &'static [&'static str] {
        match self {
            PackageManager::Npm => &["npm", "ci", "--no-audit", "--no-fund"],
            PackageManager::Pnpm => &["corepack", "pnpm", "install", "--frozen-lockfile"],
            PackageManager::Yarn => &["corepack", "yarn", "install", "--frozen-lockfile"],
        }
    }

    /// Runs a package binary that the install put in `node_modules`. `npx --no`
    /// never downloads one that is missing. The sandbox image has no pnpm or
    /// yarn, only corepack, which fetches them from the npm registry.
    pub(crate) fn exec(self) -> &'static [&'static str] {
        match self {
            PackageManager::Npm => &["npx", "--no"],
            PackageManager::Pnpm => &["corepack", "pnpm", "exec"],
            PackageManager::Yarn => &["corepack", "yarn"],
        }
    }

    /// Runs one of the package's own `package.json` scripts.
    pub(crate) fn run_script(self) -> &'static [&'static str] {
        match self {
            PackageManager::Npm => &["npm", "run"],
            PackageManager::Pnpm => &["corepack", "pnpm", "run"],
            PackageManager::Yarn => &["corepack", "yarn", "run"],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Runner {
    Vitest,
    Jest,
}

impl Runner {
    /// The runner restricted to the given test files (relative to the package).
    fn argv(self, manager: PackageManager, tests: &[String]) -> Vec<String> {
        let mut argv: Vec<String> = manager.exec().iter().map(|s| s.to_string()).collect();
        match self {
            Runner::Vitest => argv.extend(["vitest".to_string(), "run".into()]),
            // Paths, not the regex patterns jest takes by default.
            Runner::Jest => argv.extend(["jest".to_string(), "--runTestsByPath".into()]),
        }
        argv.extend(tests.iter().cloned());
        argv
    }
}

/// The runner a `package.json` uses: its `test` script names it, else a
/// dependency does (vitest first: a package moving off jest keeps both).
pub fn runner_of(package_json: &Value) -> Option<Runner> {
    let script = package_json.pointer("/scripts/test").and_then(Value::as_str).unwrap_or_default();
    if script.contains("vitest") {
        return Some(Runner::Vitest);
    }
    if script.contains("jest") {
        return Some(Runner::Jest);
    }
    let depends = |name: &str| {
        ["dependencies", "devDependencies"]
            .iter()
            .any(|key| package_json.get(key).and_then(|deps| deps.get(name)).is_some())
    };
    if depends("vitest") {
        Some(Runner::Vitest)
    } else if depends("jest") {
        Some(Runner::Jest)
    } else {
        None
    }
}

/// The changed tests of one package, and how to install and run them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestGroup {
    /// The directory of the nearest `package.json` (`""` is the repository root).
    pub package_dir: String,
    /// Where the lockfile is, and so where the install runs (a workspace root).
    pub install_dir: String,
    pub manager: PackageManager,
    pub runner: Runner,
    /// Repository-relative test paths, sorted.
    pub tests: Vec<String>,
}

/// Most packages one change may test: each costs an install and a run.
const MAX_PACKAGES: usize = 3;
/// Most mutant runs in the pod (a mutant shared by packages runs in each).
const MAX_MUTANT_RUNS: usize = 15;

/// The directory and its ancestors up to the root (`""`), nearest first.
pub(crate) fn ancestors(dir: &str) -> Vec<String> {
    let mut out = vec![dir.to_string()];
    let mut current = dir;
    while !current.is_empty() {
        current = mutation::parent(current);
        out.push(current.to_string());
    }
    out
}

pub(crate) fn join(dir: &str, name: &str) -> String {
    if dir.is_empty() {
        name.to_string()
    } else {
        format!("{dir}/{name}")
    }
}

pub(crate) fn read_json(tree: &dyn SourceTree, path: &str) -> Option<Value> {
    serde_json::from_str(&tree.read(path)?).ok()
}

/// Where a package installs: the nearest directory, from the package up to the
/// repository root, that has a lockfile (a workspace package installs from the
/// workspace root), and the package manager that lockfile names.
pub(crate) fn locate_install(tree: &dyn SourceTree, package_dir: &str) -> Option<(String, PackageManager)> {
    ancestors(package_dir).into_iter().find_map(|dir| {
        LOCKFILES
            .iter()
            .find(|(name, _)| tree.exists(&join(&dir, name)))
            .map(|(_, manager)| (dir.clone(), *manager))
    })
}

/// Groups the changed tests by package. `Err` is a reason the tests cannot be
/// run at all, which fails the `tests_pass` check.
pub fn plan_groups(tree: &dyn SourceTree, tests: &[String]) -> Result<Vec<TestGroup>, String> {
    let mut groups: Vec<TestGroup> = Vec::new();
    let mut sorted = tests.to_vec();
    sorted.sort();
    sorted.dedup();
    for test in sorted {
        let package_dir = ancestors(mutation::parent(&test))
            .into_iter()
            .find(|dir| tree.exists(&join(dir, "package.json")))
            .ok_or_else(|| format!("tests_package_not_found: no package.json above {test}"))?;
        if let Some(group) = groups.iter_mut().find(|group| group.package_dir == package_dir) {
            group.tests.push(test);
            continue;
        }
        let (install_dir, manager) = locate_install(tree, &package_dir)
            .ok_or_else(|| format!("tests_lockfile_not_found: no lockfile for {}", display_dir(&package_dir)))?;
        let runner = [&package_dir, &install_dir]
            .into_iter()
            .find_map(|dir| read_json(tree, &join(dir, "package.json")).as_ref().and_then(runner_of))
            .ok_or_else(|| {
                format!("tests_runner_unknown: {} uses neither vitest nor jest", display_dir(&package_dir))
            })?;
        groups.push(TestGroup { package_dir, install_dir, manager, runner, tests: vec![test] });
    }
    if groups.len() > MAX_PACKAGES {
        return Err(format!("tests_too_many_packages: at most {MAX_PACKAGES} packages per change"));
    }
    Ok(groups)
}

pub(crate) fn display_dir(dir: &str) -> &str {
    if dir.is_empty() {
        "the repository root"
    } else {
        dir
    }
}

pub(crate) fn pod_path(path: &str) -> String {
    if path.is_empty() {
        WORKSPACE.to_string()
    } else {
        format!("{WORKSPACE}/{path}")
    }
}

/// `cd` to the first argument, then run the rest.
pub(crate) const IN_DIR: &str = r#"cd "$1" && shift && exec "$@""#;
/// Runs the tests in a package and leaves a marker when they pass.
const RUN_TESTS: &str = r#"marker=$1; cd "$2" || exit 96; shift 2; "$@" && : > "$marker""#;
/// Runs the tests against one mutated file and always restores it. Exits 0 when
/// the tests still pass (the mutant survived), 1 when they fail or time out (it
/// was killed), and 97/98/99 when the mutant could not be judged.
const RUN_MUTANT: &str = r#"marker=$1; target=$2; mutant=$3; original=$4; dir=$5; shift 5
[ -e "$marker" ] || exit 99
cp "$mutant" "$target" || exit 97
cd "$dir" && timeout -k 5 "$MUTANT_TIMEOUT" "$@"
status=$?
cp "$original" "$target" || exit 98
[ "$status" -eq 0 ] && exit 0
exit 1"#;
/// Under the pod's per-command limit, so the restore always runs.
const MUTANT_TIMEOUT_SECS: u64 = 240;
const MUTANT_DIR: &str = "/tmp/nm-mutants";

#[derive(Clone, Debug, PartialEq, Eq)]
enum PodStep {
    Install(String),
    Tests(String),
    /// One run of mutant `n` (a mutant shared by several packages runs once per package).
    Mutant(usize, String),
    Fault(String),
}

/// The commands and files of the checks pod, and what each command is.
#[derive(Debug, Default)]
pub struct PodPlan {
    pub commands: Vec<Vec<String>>,
    pub files: Vec<(String, Vec<u8>)>,
    steps: Vec<PodStep>,
    /// Seeded faults that cannot be run (no changed test in their package).
    unrunnable_faults: Vec<String>,
    /// Whether the change had seeded faults at all (eval only).
    seeded: bool,
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// The group a source path belongs to: the one whose package holds it, the
/// deepest when packages nest.
fn group_of<'a>(groups: &'a [TestGroup], path: &str) -> Option<(usize, &'a TestGroup)> {
    groups
        .iter()
        .enumerate()
        .filter(|(_, group)| group.package_dir.is_empty() || path.starts_with(&format!("{}/", group.package_dir)))
        .max_by_key(|(_, group)| group.package_dir.len())
}

/// A faulted file to run the tests against.
pub struct Faulted {
    pub description: String,
    pub path: String,
    pub original: String,
    pub content: String,
}

/// Builds the pod's commands. Each mutant reruns the tests of every group
/// whose tests import its file, and is killed if any of those runs fails.
pub fn pod_plan(groups: &[TestGroup], mutants: &[(Vec<usize>, Mutant)], faults: &[Faulted], originals: &[(String, String)]) -> PodPlan {
    let mut plan = PodPlan { seeded: !faults.is_empty(), ..Default::default() };
    let mut installed = Vec::new();
    for group in groups {
        if !installed.contains(&group.install_dir) {
            installed.push(group.install_dir.clone());
            let mut argv = strings(&["sh", "-c", IN_DIR, "sh"]);
            argv.push(pod_path(&group.install_dir));
            argv.extend(group.manager.install().iter().map(|s| s.to_string()));
            plan.commands.push(argv);
            plan.steps.push(PodStep::Install(format!(
                "install in {} ({})",
                display_dir(&group.install_dir),
                group.manager.install().join(" ")
            )));
        }
    }
    let marker = |index: usize| format!("/tmp/nm-tests-passed-{index}");
    let runner_argv = |group: &TestGroup| {
        let relative: Vec<String> = group
            .tests
            .iter()
            .map(|test| test.strip_prefix(&format!("{}/", group.package_dir)).unwrap_or(test).to_string())
            .collect();
        group.runner.argv(group.manager, &relative)
    };
    for (index, group) in groups.iter().enumerate() {
        let mut argv = strings(&["sh", "-c", RUN_TESTS, "sh"]);
        argv.extend([marker(index), pod_path(&group.package_dir)]);
        argv.extend(runner_argv(group));
        plan.commands.push(argv);
        plan.steps.push(PodStep::Tests(format!("tests in {}", display_dir(&group.package_dir))));
    }
    let original_file = |path: &str| {
        originals.iter().position(|(original, _)| original == path).map(|k| format!("{MUTANT_DIR}/o{k}"))
    };
    for (k, (_, content)) in originals.iter().enumerate() {
        plan.files.push((format!("{MUTANT_DIR}/o{k}"), content.clone().into_bytes()));
    }
    let script = RUN_MUTANT.replace("\"$MUTANT_TIMEOUT\"", &MUTANT_TIMEOUT_SECS.to_string());
    let push_faulted = |plan: &mut PodPlan, group: usize, name: String, path: &str, content: &str, step: PodStep| {
        let Some(original) = original_file(path) else { return };
        if !plan.files.iter().any(|(path, _)| *path == name) {
            plan.files.push((name.clone(), content.as_bytes().to_vec()));
        }
        let mut argv = strings(&["sh", "-c", &script, "sh"]);
        argv.extend([marker(group), pod_path(path), name, original, pod_path(&groups[group].package_dir)]);
        argv.extend(runner_argv(&groups[group]));
        plan.commands.push(argv);
        plan.steps.push(step);
    };
    for (n, (owners, mutant)) in mutants.iter().enumerate() {
        for group in owners {
            let name = format!("{MUTANT_DIR}/m{n}");
            push_faulted(&mut plan, *group, name, &mutant.path, &mutant.content, PodStep::Mutant(n, mutant.describe()));
        }
    }
    for (n, fault) in faults.iter().enumerate() {
        match group_of(groups, &fault.path) {
            Some((group, _)) => {
                let name = format!("{MUTANT_DIR}/f{n}");
                push_faulted(&mut plan, group, name, &fault.path, &fault.content, PodStep::Fault(fault.description.clone()));
            }
            None => plan
                .unrunnable_faults
                .push(format!("not run (no changed test in its package): {}", fault.description)),
        }
    }
    plan
}

/// How one mutant (or seeded fault) run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fate {
    Killed,
    Survived,
    /// Not judged; counts as survived, so a broken run never helps the score.
    Unjudged(&'static str),
    /// The mutated file may still be in place, so later results are suspect.
    NotRestored,
}

fn fate(exit_code: Option<i32>) -> Fate {
    match exit_code {
        Some(0) => Fate::Survived,
        Some(1) => Fate::Killed,
        Some(97) => Fate::Unjudged("the mutated file could not be put in place"),
        Some(99) => Fate::Unjudged("the tests did not pass first"),
        // 98: the restore failed; 124/137: the pod's own limit stopped the script
        // before its restore (the mutant's tests have a shorter timeout).
        Some(98 | 124 | 137) => Fate::NotRestored,
        _ => Fate::Unjudged("no usable exit status"),
    }
}

/// The end of a command's output, for a failure's details.
pub(crate) fn tail(run: &CommandRun) -> String {
    let mut text = String::from_utf8_lossy(&run.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&run.stderr));
    let text = text.trim();
    let chars: Vec<char> = text.chars().collect();
    let start = chars.len().saturating_sub(600);
    chars[start..].iter().collect::<String>().replace('\n', " | ")
}

const MAX_DETAILS: usize = 20;

fn outcome(name: &'static str, passed: bool, advisory: bool, details: Vec<String>) -> CheckOutcome {
    CheckOutcome { name, passed, advisory, details: details.into_iter().take(MAX_DETAILS).collect() }
}

/// The checks a run that cannot start reports: its tests did not run.
pub fn not_runnable(reason: String) -> Vec<CheckOutcome> {
    vec![
        outcome("tests_pass", false, false, vec![reason]),
        outcome("mutation_score", false, false, vec!["not run: the changed tests could not run".into()]),
    ]
}

/// Turns the pod's exit codes into `tests_pass`, `mutation_score` and, when the
/// change had seeded faults, `seeded_fault_caught`.
pub fn interpret(plan: &PodPlan, runs: &[CommandRun]) -> Vec<CheckOutcome> {
    let mut failures = Vec::new();
    let mut mutants: Vec<(usize, String, Fate)> = Vec::new();
    let mut faults: Vec<(String, Fate)> = Vec::new();
    for (step, run) in plan.steps.iter().zip(runs) {
        match step {
            PodStep::Install(what) | PodStep::Tests(what) => match run.exit_code {
                Some(0) => {}
                Some(127) => failures.push(format!("{what}: command not found (exit 127)")),
                Some(code) => failures.push(format!("{what} exited {code}: {}", tail(run))),
                None => failures.push(format!("{what}: no exit status")),
            },
            PodStep::Mutant(n, what) => {
                let this = fate(run.exit_code);
                match mutants.iter_mut().find(|(index, _, _)| index == n) {
                    // Several packages ran it: a suspect run taints it, any
                    // failing run kills it, else the worse outcome stands.
                    Some((_, _, fate)) => {
                        *fate = match (*fate, this) {
                            (Fate::NotRestored, _) | (_, Fate::NotRestored) => Fate::NotRestored,
                            (Fate::Killed, _) | (_, Fate::Killed) => Fate::Killed,
                            (Fate::Unjudged(why), _) | (_, Fate::Unjudged(why)) => Fate::Unjudged(why),
                            _ => Fate::Survived,
                        }
                    }
                    None => mutants.push((*n, what.clone(), this)),
                }
            }
            PodStep::Fault(what) => faults.push((what.clone(), fate(run.exit_code))),
        }
    }
    let tests_passed = failures.is_empty() && runs.len() == plan.steps.len();
    let mut checks = vec![outcome("tests_pass", tests_passed, false, failures)];
    if !tests_passed {
        checks.push(outcome("mutation_score", false, false, vec!["not run: the changed tests did not pass".into()]));
        if plan.seeded {
            checks.push(outcome("seeded_fault_caught", false, false, vec!["not run: the changed tests did not pass".into()]));
        }
        return checks;
    }
    let suspect = mutants.iter().any(|(_, _, fate)| *fate == Fate::NotRestored)
        || faults.iter().any(|(_, fate)| *fate == Fate::NotRestored);
    let restore_note = "a mutated file could not be restored, so later results cannot be trusted";
    if mutants.is_empty() {
        // Advisory (ADR 94ced31c): nothing to flip is not the tests' fault; the
        // review reads them instead.
        checks.push(outcome(
            "mutation_score",
            false,
            true,
            vec!["no mutants: the code the changed tests import (through relative imports) has no operator to flip".into()],
        ));
    } else {
        let killed = mutants.iter().filter(|(_, _, fate)| *fate == Fate::Killed).count();
        let total = mutants.len();
        let mut details = vec![format!("killed {killed} of {total} mutants; at least 60% must be killed")];
        if suspect {
            details.push(restore_note.into());
        }
        details.extend(mutants.iter().filter_map(|(_, what, fate)| match fate {
            Fate::Killed => None,
            Fate::Survived => Some(format!("survived: {what}")),
            Fate::Unjudged(why) => Some(format!("not judged ({why}): {what}")),
            Fate::NotRestored => Some(format!("not judged (not restored): {what}")),
        }));
        checks.push(outcome("mutation_score", mutation::score_passes(killed, total) && !suspect, false, details));
    }
    if plan.seeded {
        let mut details = plan.unrunnable_faults.clone();
        if suspect {
            details.push(restore_note.into());
        }
        details.extend(faults.iter().filter(|(_, fate)| *fate != Fate::Killed).map(|(what, fate)| match fate {
            Fate::Unjudged(why) => format!("not judged ({why}): {what}"),
            _ => format!("not caught: {what}"),
        }));
        checks.push(outcome("seeded_fault_caught", details.is_empty(), false, details));
    }
    checks
}

/// Why the pod cannot be planned.
#[derive(Debug, PartialEq, Eq)]
pub enum PlanError {
    /// The tests cannot run (no package, lockfile or known runner): a failed check.
    NotRunnable(String),
    /// The eval's seeded fault does not fit the checkout: an error of the task.
    SeededFault(String),
}

/// Everything the worker decides before the pod: groups, mutants, faults.
pub fn prepare(tree: &dyn SourceTree, tests: &[String], seeded: &[SeededFault]) -> Result<PodPlan, PlanError> {
    let groups = plan_groups(tree, tests).map_err(PlanError::NotRunnable)?;
    // Every group whose tests import a file owns it, with the imported names of
    // all of them in scope; a mutant reruns the tests of each owner.
    let mut owner: Vec<(mutation::Target, Vec<usize>)> = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        for target in mutation::mutation_targets(tree, &group.tests) {
            match owner.iter_mut().find(|(owned, _)| owned.path == target.path) {
                Some((owned, groups)) => {
                    owned.names = mutation::merge_names(owned.names.take(), target.names).map(|mut names| {
                        names.sort();
                        names
                    });
                    groups.push(index);
                }
                None => owner.push((target, vec![index])),
            }
        }
    }
    let targets: Vec<mutation::Target> = owner.iter().map(|(target, _)| target.clone()).collect();
    let mut runs = 0;
    let mutants: Vec<(Vec<usize>, Mutant)> = mutation::mutants(tree, &targets, mutation::MAX_MUTANTS)
        .into_iter()
        .map(|mutant| {
            let groups = owner.iter().find(|(target, _)| target.path == mutant.path).map(|(_, g)| g.clone()).unwrap_or_default();
            (groups, mutant)
        })
        // Bounded pod time: a shared file costs one run per package.
        .take_while(|(groups, _)| {
            runs += groups.len();
            runs <= MAX_MUTANT_RUNS
        })
        .collect();
    let mut faults = Vec::new();
    for fault in seeded {
        fault.validate().map_err(PlanError::SeededFault)?;
        let original = tree.read(&fault.path).ok_or_else(|| PlanError::SeededFault("seeded_fault_file_missing".into()))?;
        let content = fault.apply(&original).map_err(PlanError::SeededFault)?;
        faults.push(Faulted { description: fault.describe(), path: fault.path.clone(), original, content });
    }
    let mut originals: Vec<(String, String)> = Vec::new();
    for mutant in mutants.iter().map(|(_, m)| m) {
        if !originals.iter().any(|(path, _)| *path == mutant.path) {
            if let Some(content) = tree.read(&mutant.path) {
                originals.push((mutant.path.clone(), content));
            }
        }
    }
    for fault in &faults {
        if !originals.iter().any(|(path, _)| *path == fault.path) {
            originals.push((fault.path.clone(), fault.original.clone()));
        }
    }
    Ok(pod_plan(&groups, &mutants, &faults, &originals))
}

/// Largest file the planner reads (a source file or a `package.json`).
const MAX_READ_BYTES: u64 = 2 * 1_048_576;

/// A checkout on disk. Every path is resolved and must stay inside the
/// checkout once symlinks are followed: a repository's symlink must never make
/// the worker read (and ship to the pod) a file of its own.
pub struct DiskTree {
    root: PathBuf,
}

impl DiskTree {
    pub fn new(root: &Path) -> std::io::Result<Self> {
        Ok(Self { root: root.canonicalize()? })
    }

    fn inside(&self, path: &str) -> Option<PathBuf> {
        let relative = mutation::normalize(path)?;
        let full = self.root.join(relative).canonicalize().ok()?;
        full.starts_with(&self.root).then_some(full)
    }
}

impl SourceTree for DiskTree {
    fn read(&self, path: &str) -> Option<String> {
        let full = self.inside(path)?;
        let meta = std::fs::metadata(&full).ok()?;
        if !meta.is_file() || meta.len() > MAX_READ_BYTES {
            return None;
        }
        std::fs::read_to_string(full).ok()
    }
    fn exists(&self, path: &str) -> bool {
        self.inside(path).is_some_and(|full| full.is_file())
    }
}

/// Runs the execution checks for a change that passed the path checks. `Err`
/// is a tenant-visible code: the checks could not run (no sandbox, a pod
/// failure, an eval task whose seeded fault does not fit).
pub(crate) async fn run_checks(
    env: &super::specialist_run::StepEnv<'_>,
    changed: &[ChangedFile],
    seeded: &[SeededFault],
) -> Result<Vec<CheckOutcome>, String> {
    // Fail closed: the worker never runs repository code (`select` already
    // keeps the tests specialist off unsandboxed runs).
    if !env.sandboxed {
        return Err("tests_specialist_requires_sandbox".into());
    }
    let tests: Vec<String> = changed
        .iter()
        .filter(|file| file.status != "removed" && mutation::is_js_test_file(&file.filename))
        .map(|file| file.filename.clone())
        .collect();
    let root = env.workdir.to_path_buf();
    let seeded = seeded.to_vec();
    let planned = tokio::task::spawn_blocking(move || {
        let tree = DiskTree::new(&root).map_err(|_| PlanError::NotRunnable("checkout_unreadable".into()))?;
        prepare(&tree, &tests, &seeded)
    })
    .await
    .map_err(|_| "specialist_checks_failed_to_run".to_string())?;
    let plan = match planned {
        Ok(plan) => plan,
        Err(PlanError::NotRunnable(reason)) => return Ok(not_runnable(reason)),
        Err(PlanError::SeededFault(code)) => return Err(code),
    };
    let receipts = std::sync::Mutex::new(Vec::new());
    let slot = format!("{}-tests", env.slot);
    let run = super::sandboxed::SandboxedRun {
        store: env.store,
        org_id: env.org_id,
        run_id: env.run_id,
        attempt_id: env.attempt_id,
        retry: 0,
        workdir: env.workdir,
        secret_values: env.secret_values,
        seq_base: 0,
        wall_time: Duration::ZERO,
        verification: &[],
        receipts: &receipts,
        qa: None,
        slot: &slot,
        writes: None,
    };
    // corepack must never wait on a download prompt.
    let corepack = [("COREPACK_ENABLE_DOWNLOAD_PROMPT".to_string(), "0".to_string())];
    let spec = super::sandboxed::CommandsSpec {
        commands: &plan.commands,
        extra_env: &corepack,
        timeout_secs: crate::factory::verification::COMMAND_TIMEOUT_SECS,
        reproduce_failures: false,
        hosts: &[],
        label: "m",
        max_stdout: 64_000,
        files: plan.files.clone(),
        proxy_file: None,
    };
    let runs = super::sandboxed::run_commands_sandboxed(&run, &spec)
        .await
        .map_err(|error| super::sandboxed::failure_code(env.run_id, &error))?;
    if runs.len() != plan.commands.len() {
        return Err("tests_checks_incomplete".into());
    }
    Ok(interpret(&plan, &runs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Tree(HashMap<String, String>);

    impl Tree {
        fn with(mut self, path: &str, content: &str) -> Self {
            self.0.insert(path.into(), content.into());
            self
        }
    }

    impl SourceTree for Tree {
        fn read(&self, path: &str) -> Option<String> {
            self.0.get(path).cloned()
        }
        fn exists(&self, path: &str) -> bool {
            self.0.contains_key(path)
        }
    }

    fn paths(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn the_runner_comes_from_the_test_script_then_the_dependencies() {
        assert_eq!(runner_of(&json!({"scripts": {"test": "vitest run"}, "devDependencies": {"jest": "1"}})), Some(Runner::Vitest));
        assert_eq!(runner_of(&json!({"scripts": {"test": "jest --ci"}, "devDependencies": {"vitest": "1"}})), Some(Runner::Jest));
        assert_eq!(runner_of(&json!({"devDependencies": {"jest": "1", "vitest": "2"}})), Some(Runner::Vitest));
        assert_eq!(runner_of(&json!({"dependencies": {"jest": "1"}})), Some(Runner::Jest));
        assert_eq!(runner_of(&json!({"scripts": {"test": "mocha"}, "devDependencies": {"mocha": "1"}})), None);
    }

    #[test]
    fn tests_group_by_package_with_the_lockfile_manager_and_runner() {
        let tree = Tree::default()
            .with("apps/admin/package.json", r#"{"scripts":{"test":"vitest run"}}"#)
            .with("apps/admin/package-lock.json", "{}")
            .with("apps/admin/pnpm-lock.yaml", "")
            .with("packages/ui/package.json", r#"{"devDependencies":{"jest":"29"}}"#)
            .with("package.json", r#"{"private":true}"#)
            .with("pnpm-lock.yaml", "");
        let groups = plan_groups(
            &tree,
            &paths(&["packages/ui/src/__tests__/a.tsx", "apps/admin/src/lib/b.test.ts", "apps/admin/src/a.test.ts"]),
        )
        .unwrap();
        assert_eq!(
            groups,
            [
                TestGroup {
                    package_dir: "apps/admin".into(),
                    install_dir: "apps/admin".into(),
                    // package-lock.json wins over pnpm-lock.yaml in the same directory.
                    manager: PackageManager::Npm,
                    runner: Runner::Vitest,
                    tests: paths(&["apps/admin/src/a.test.ts", "apps/admin/src/lib/b.test.ts"]),
                },
                TestGroup {
                    package_dir: "packages/ui".into(),
                    // A workspace package installs from the workspace root.
                    install_dir: "".into(),
                    manager: PackageManager::Pnpm,
                    runner: Runner::Jest,
                    tests: paths(&["packages/ui/src/__tests__/a.tsx"]),
                },
            ]
        );
        let yarn = Tree::default().with("package.json", r#"{"devDependencies":{"vitest":"2"}}"#).with("yarn.lock", "");
        assert_eq!(plan_groups(&yarn, &paths(&["a.test.ts"])).unwrap()[0].manager, PackageManager::Yarn);
    }

    #[test]
    fn untestable_packages_fail_with_a_clear_code() {
        let none = Tree::default();
        assert!(plan_groups(&none, &paths(&["src/a.test.ts"])).unwrap_err().starts_with("tests_package_not_found"));
        let unlocked = Tree::default().with("package.json", r#"{"devDependencies":{"vitest":"2"}}"#);
        assert!(plan_groups(&unlocked, &paths(&["a.test.ts"])).unwrap_err().starts_with("tests_lockfile_not_found"));
        let mocha = Tree::default().with("package.json", r#"{"devDependencies":{"mocha":"1"}}"#).with("yarn.lock", "");
        assert_eq!(
            plan_groups(&mocha, &paths(&["a.test.ts"])).unwrap_err(),
            "tests_runner_unknown: the repository root uses neither vitest nor jest"
        );
        let mut many = Tree::default();
        for n in 0..4 {
            many = many
                .with(&format!("p{n}/package.json"), r#"{"devDependencies":{"vitest":"2"}}"#)
                .with(&format!("p{n}/yarn.lock"), "");
        }
        let tests: Vec<String> = (0..4).map(|n| format!("p{n}/a.test.ts")).collect();
        assert!(plan_groups(&many, &tests).unwrap_err().starts_with("tests_too_many_packages"));
    }

    fn admin() -> Tree {
        Tree::default()
            .with("apps/admin/package.json", r#"{"scripts":{"test":"vitest run"}}"#)
            .with("apps/admin/package-lock.json", "{}")
            .with("apps/admin/src/lib/range.test.ts", "import { inRange } from './range'\n")
            .with("apps/admin/src/lib/range.ts", "export const inRange = (n: number) => n >= 0 && n < 10\n")
    }

    #[test]
    fn the_pod_installs_runs_the_tests_then_each_mutant_with_a_restore() {
        let plan = prepare(&admin(), &paths(&["apps/admin/src/lib/range.test.ts"]), &[]).unwrap();
        // install, tests, 3 mutants (`>=`, `&&`, `<`).
        assert_eq!(plan.commands.len(), 5);
        assert_eq!(&plan.commands[0][4..], ["/workspace/apps/admin", "npm", "ci", "--no-audit", "--no-fund"]);
        assert_eq!(plan.commands[0][..3], ["sh", "-c", IN_DIR]);
        assert_eq!(
            &plan.commands[1][4..],
            ["/tmp/nm-tests-passed-0", "/workspace/apps/admin", "npx", "--no", "vitest", "run", "src/lib/range.test.ts"]
        );
        let mutant = &plan.commands[2];
        assert!(mutant[2].contains("timeout -k 5 240 \"$@\"") && mutant[2].contains("cp \"$original\" \"$target\""));
        assert_eq!(
            &mutant[4..9],
            ["/tmp/nm-tests-passed-0", "/workspace/apps/admin/src/lib/range.ts", "/tmp/nm-mutants/m0", "/tmp/nm-mutants/o0", "/workspace/apps/admin"]
        );
        assert_eq!(&mutant[9..], ["npx", "--no", "vitest", "run", "src/lib/range.test.ts"]);
        let file = |name: &str| plan.files.iter().find(|(path, _)| path == name).map(|(_, c)| String::from_utf8_lossy(c).into_owned());
        assert_eq!(file("/tmp/nm-mutants/o0").unwrap(), "export const inRange = (n: number) => n >= 0 && n < 10\n");
        assert_eq!(file("/tmp/nm-mutants/m0").unwrap(), "export const inRange = (n: number) => n < 0 && n < 10\n");
        // Nothing from the repository is spliced into a script.
        assert!(plan.commands.iter().all(|argv| !argv[2].contains("apps/admin")));
    }

    fn run(exit_code: Option<i32>) -> CommandRun {
        CommandRun { exit_code, stdout: b"FAIL src/lib/range.test.ts\nexpected 1".to_vec(), ..Default::default() }
    }

    fn by_name<'a>(checks: &'a [CheckOutcome], name: &str) -> &'a CheckOutcome {
        checks.iter().find(|c| c.name == name).unwrap()
    }

    #[test]
    fn a_source_shared_by_packages_runs_against_each_and_merges_scopes() {
        let both = r#"{"scripts":{"test":"vitest run"}}"#;
        let tree = Tree::default()
            .with("a/package.json", both)
            .with("a/yarn.lock", "")
            .with("b/package.json", both)
            .with("b/pnpm-lock.yaml", "")
            .with("shared/lib.ts", "export const one = (x) => x === 1\nexport const two = (x) => x === 2\nexport const three = (x) => x === 3\n")
            .with("a/x.test.ts", "import { one } from '../shared/lib'\n")
            .with("b/y.test.ts", "import { two } from '../shared/lib'\n");
        let plan = prepare(&tree, &paths(&["a/x.test.ts", "b/y.test.ts"]), &[]).unwrap();
        // 2 installs (corepack), 2 test runs, 2 mutants (`one`, `two`; not `three`) x 2 packages.
        assert_eq!(plan.commands.len(), 8);
        assert_eq!(&plan.commands[0][4..], ["/workspace/a", "corepack", "yarn", "install", "--frozen-lockfile"]);
        assert_eq!(&plan.commands[1][4..], ["/workspace/b", "corepack", "pnpm", "install", "--frozen-lockfile"]);
        assert_eq!(&plan.commands[3][6..9], ["corepack", "pnpm", "exec"]);
        let mutant_files = plan.files.iter().filter(|(path, _)| path.contains("/m")).count();
        assert_eq!(mutant_files, 2, "each mutant ships once");
        // Mutant 0 survives in a but is killed in b: killed. Mutant 1 survives both.
        let exits = [0, 0, 0, 0, 0, 1, 0, 0];
        let checks = interpret(&plan, &exits.map(|code| run(Some(code))));
        let score = by_name(&checks, "mutation_score");
        assert_eq!(score.details[0], "killed 1 of 2 mutants; at least 60% must be killed");
        assert_eq!(score.details.len(), 2, "{score:?}");
    }

    #[test]
    fn the_mutation_score_needs_sixty_percent_killed() {
        let plan = prepare(&admin(), &paths(&["apps/admin/src/lib/range.test.ts"]), &[]).unwrap();
        let checks = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(1)), run(Some(0))]);
        assert!(by_name(&checks, "tests_pass").passed);
        let score = by_name(&checks, "mutation_score");
        assert!(score.passed && !score.advisory, "{score:?}");
        assert_eq!(score.details[0], "killed 2 of 3 mutants; at least 60% must be killed");
        assert_eq!(score.details[1], "survived: apps/admin/src/lib/range.ts:1 `<` -> `>=`");
        let weak = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(0)), run(Some(0))]);
        assert!(!by_name(&weak, "mutation_score").passed);
        // A mutant that could not be judged never counts as killed.
        let unjudged = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(97)), run(None)]);
        assert!(!by_name(&unjudged, "mutation_score").passed);
        // A file left mutated makes every later result suspect.
        let suspect = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(98)), run(Some(1)), run(Some(1))]);
        let score = by_name(&suspect, "mutation_score");
        assert!(!score.passed && score.details.iter().any(|d| d.contains("could not be restored")), "{score:?}");
    }

    #[test]
    fn failing_tests_fail_the_report_and_skip_the_score() {
        let plan = prepare(&admin(), &paths(&["apps/admin/src/lib/range.test.ts"]), &[]).unwrap();
        let checks = interpret(&plan, &[run(Some(0)), run(Some(1)), run(Some(99)), run(Some(99)), run(Some(99))]);
        let pass = by_name(&checks, "tests_pass");
        assert!(!pass.passed);
        assert_eq!(pass.details, ["tests in apps/admin exited 1: FAIL src/lib/range.test.ts | expected 1"]);
        assert_eq!(by_name(&checks, "mutation_score").details, ["not run: the changed tests did not pass"]);
        let missing = interpret(&plan, &[run(Some(127)), run(Some(1))]);
        assert_eq!(by_name(&missing, "tests_pass").details[0], "install in apps/admin (npm ci --no-audit --no-fund): command not found (exit 127)");
        let unrunnable = not_runnable("tests_runner_unknown: x".into());
        assert!(unrunnable.iter().all(|check| !check.passed && !check.advisory));
    }

    #[test]
    fn no_mutants_is_advisory_for_the_review() {
        let tree = admin().with("apps/admin/src/lib/range.ts", "export const inRange = (n: number) => n\n");
        let plan = prepare(&tree, &paths(&["apps/admin/src/lib/range.test.ts"]), &[]).unwrap();
        assert_eq!(plan.commands.len(), 2, "install and tests only");
        let checks = interpret(&plan, &[run(Some(0)), run(Some(0))]);
        let score = by_name(&checks, "mutation_score");
        assert!(!score.passed && score.advisory);
        let report = crate::factory::specialists::CheckReport::from_checks(checks);
        assert!(report.passed, "an advisory never fails the report");
        assert_eq!(report.advisories().len(), 1);
    }

    #[test]
    fn seeded_faults_are_validated_and_must_be_caught() {
        let fault = |path: &str, find: &str, replace: &str| SeededFault { path: path.into(), find: find.into(), replace: replace.into() };
        assert_eq!(fault("../x.ts", "a", "b").validate().unwrap_err(), "seeded_fault_invalid_path");
        assert_eq!(fault("/x.ts", "a", "b").validate().unwrap_err(), "seeded_fault_invalid_path");
        assert_eq!(fault("src/a.test.ts", "a", "b").validate().unwrap_err(), "seeded_fault_invalid_path");
        assert_eq!(fault("src/a.ts", "", "b").validate().unwrap_err(), "seeded_fault_invalid_text");
        assert_eq!(fault("src/a.ts", "a", "a").validate().unwrap_err(), "seeded_fault_invalid_text");
        let ok = fault("apps/admin/src/lib/range.ts", "n < 10", "n <= 10");
        ok.validate().unwrap();
        assert_eq!(ok.apply("n < 1; n < 10").unwrap(), "n < 1; n <= 10");
        assert_eq!(ok.apply("n < 10 || n < 10").unwrap_err(), "seeded_fault_ambiguous");
        assert_eq!(ok.apply("n").unwrap_err(), "seeded_fault_not_found");

        let tests = paths(&["apps/admin/src/lib/range.test.ts"]);
        let plan = prepare(&admin(), &tests, std::slice::from_ref(&ok)).unwrap();
        // install, tests, 3 mutants, 1 fault.
        assert_eq!(plan.commands.len(), 6);
        let faulted = plan.files.iter().find(|(path, _)| path == "/tmp/nm-mutants/f0").unwrap();
        assert_eq!(faulted.1, b"export const inRange = (n: number) => n >= 0 && n <= 10\n");
        let caught = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(1)), run(Some(1)), run(Some(1))]);
        assert!(by_name(&caught, "seeded_fault_caught").passed);
        let missed = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(1)), run(Some(1)), run(Some(0))]);
        assert_eq!(
            by_name(&missed, "seeded_fault_caught").details,
            ["not caught: apps/admin/src/lib/range.ts: `n < 10` -> `n <= 10`"]
        );
        // A fault outside every tested package cannot be caught.
        let outside = admin().with("libs/x.ts", "const a = 1\n");
        let plan = prepare(&outside, &tests, &[fault("libs/x.ts", "1", "2")]).unwrap();
        let checks = interpret(&plan, &[run(Some(0)), run(Some(0)), run(Some(1)), run(Some(1)), run(Some(1))]);
        assert!(!by_name(&checks, "seeded_fault_caught").passed);
        // A fault that does not fit the checkout is the task's error, not a check.
        let wrong = fault("apps/admin/src/lib/range.ts", "missing", "x");
        assert_eq!(prepare(&admin(), &tests, &[wrong]).unwrap_err(), PlanError::SeededFault("seeded_fault_not_found".into()));
        // Without seeded faults (production) there is no such check.
        let plan = prepare(&admin(), &tests, &[]).unwrap();
        let checks = interpret(&plan, &vec![run(Some(0)); 5]);
        assert!(checks.iter().all(|check| check.name != "seeded_fault_caught"));
    }

    #[test]
    fn a_disk_tree_never_leaves_the_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.ts"), "export const key = 'x' === 'y'").unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/a.ts"), "ok").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path().join("secret.ts"), dir.path().join("src/link.ts")).unwrap();
        let tree = DiskTree::new(dir.path()).unwrap();
        assert_eq!(tree.read("src/a.ts").as_deref(), Some("ok"));
        assert!(tree.exists("src/a.ts") && !tree.exists("src"));
        assert_eq!(tree.read("src/link.ts"), None, "a symlink out of the checkout is not followed");
        assert!(!tree.exists("src/link.ts"));
        assert_eq!(tree.read("../secret.ts"), None);
    }
}
