//! Golden task replay through the F1 verification gate (ADR 51dc0839).
//!
//! Each golden task is a merged pull request. The replay checks out its accepted
//! answer (`merge_sha`), runs the repository's JS test commands in a sandbox
//! commands pod, and stores the `VerificationReport`. No agent runs and nothing is
//! published. The dataset lives outside the repository; tasks arrive on stdin.

use serde::{Deserialize, Serialize};

/// The fields of a golden task the replay needs (the task text is not one).
#[derive(Clone, Debug, Deserialize)]
pub struct GoldenTask {
    pub id: String,
    pub repository: String,
    pub merge_sha: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
}

/// Run id under which replay reports are stored (one per task and head).
pub const REPLAY_RUN_ID: &str = "golden-v1";

/// The commands that verify a task, chosen by repository and touched paths. The
/// sandbox image has node/npm only, so Rust-only changes have none.
pub fn commands_for(task: &GoldenTask) -> Vec<Vec<String>> {
    let argv = |parts: &[&str]| {
        parts
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
    };
    let touches = |prefix: &str| {
        task.changed_files
            .iter()
            .any(|file| file.starts_with(prefix))
    };
    match task.repository.as_str() {
        "smart-coder-labs/nexus-mind" => {
            let mut commands = Vec::new();
            if touches("apps/admin/") {
                commands.push(argv(&["npm", "--prefix", "apps/admin", "ci"]));
                commands.push(argv(&["npm", "--prefix", "apps/admin", "test"]));
            }
            // The backoffice has no test script: its build type-checks it.
            if touches("apps/backoffice/") {
                commands.push(argv(&["npm", "--prefix", "apps/backoffice", "ci"]));
                commands.push(argv(&[
                    "npm",
                    "--prefix",
                    "apps/backoffice",
                    "run",
                    "build",
                ]));
            }
            commands
        }
        "kasymir/kasymir-app-ui" => vec![
            argv(&["npm", "ci"]),
            argv(&["npm", "test", "--", "--ci", "--passWithNoTests"]),
        ],
        _ => Vec::new(),
    }
}

/// Checks a task before any pod is started: the report needs a canonical UUID,
/// and the checkout a valid repository and a full commit SHA.
pub fn validate_task(task: &GoldenTask) -> anyhow::Result<()> {
    let canonical = uuid::Uuid::parse_str(&task.id)
        .ok()
        .is_some_and(|id| id.hyphenated().to_string() == task.id);
    if !canonical {
        anyhow::bail!("invalid_task_id")
    }
    if super::connectors::validate_repository(&task.repository).is_err() {
        anyhow::bail!("invalid_repository")
    }
    if task.merge_sha.len() != 40 || !task.merge_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_merge_sha")
    }
    Ok(())
}

/// One line of the replay summary.
#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct ReplayOutcome {
    pub id: String,
    pub repository: String,
    pub merge_sha: String,
    pub passed: bool,
    /// No executable evidence in the sandbox image (e.g. Rust-only changes).
    pub skipped: bool,
    pub blocking_failures: Vec<String>,
    /// Set when the replay itself could not run (clone, sandbox).
    pub error: Option<String>,
    /// The end of the first failing command's output, to tell a broken install
    /// from a failing test. The proxy token is already scrubbed.
    pub failure_output: Option<String>,
}

/// Most output kept from a failing command.
const FAILURE_OUTPUT_BYTES: usize = 600;

/// The last bytes of the first failing command's stderr (stdout when stderr is
/// empty).
pub fn failure_output(runs: &[crate::factory::sandbox_exec::CommandRun]) -> Option<String> {
    let failed = runs.iter().find(|run| run.exit_code != Some(0))?;
    let stream = if failed.stderr.iter().any(|b| !b.is_ascii_whitespace()) {
        &failed.stderr
    } else {
        &failed.stdout
    };
    let tail = &stream[stream.len().saturating_sub(FAILURE_OUTPUT_BYTES)..];
    Some(String::from_utf8_lossy(tail).trim().to_string())
}

/// Replays one golden task: checkout of `merge_sha` (the worker's GitHub token
/// never enters a pod), commands in a sandbox commands pod, report stored under
/// [`REPLAY_RUN_ID`]. Never publishes anything.
pub async fn replay_task(
    store: &crate::store::sqlite::SqliteStore,
    org_id: &str,
    github_token: &str,
    task: &GoldenTask,
) -> ReplayOutcome {
    let mut outcome = ReplayOutcome {
        id: task.id.clone(),
        repository: task.repository.clone(),
        merge_sha: task.merge_sha.clone(),
        passed: false,
        skipped: commands_for(task).is_empty(),
        blocking_failures: Vec::new(),
        error: None,
        failure_output: None,
    };
    match replay(store, org_id, github_token, task).await {
        Ok((report, output)) => {
            outcome.passed = report.passed;
            outcome.blocking_failures = report.blocking_failures;
            outcome.failure_output = output;
        }
        Err(error) => {
            outcome.error = Some(super::sandboxed::failure_code(&task.id, &error));
        }
    }
    outcome
}

async fn replay(
    store: &crate::store::sqlite::SqliteStore,
    org_id: &str,
    github_token: &str,
    task: &GoldenTask,
) -> anyhow::Result<(
    crate::factory::contracts::VerificationReport,
    Option<String>,
)> {
    validate_task(task)?;
    let commands = commands_for(task);
    let (receipts, output) = if commands.is_empty() {
        // Nothing executable in the sandbox image: the report says so.
        (Vec::new(), None)
    } else {
        let checkout = tempfile::tempdir()?;
        let workdir = checkout.path().join("repo");
        tokio::fs::create_dir_all(&workdir).await?;
        let mut init = tokio::process::Command::new("git");
        init.current_dir(&workdir).args(["init", "-q"]);
        super::worker::command_ok(init).await?;
        let mut fetch = super::worker::authenticated_git(github_token);
        fetch.current_dir(&workdir).args([
            "fetch",
            "-q",
            "--depth",
            "1",
            &format!("https://github.com/{}.git", task.repository),
            &task.merge_sha,
        ]);
        super::worker::command_ok(fetch).await?;
        let mut checkout_head = tokio::process::Command::new("git");
        checkout_head
            .current_dir(&workdir)
            .args(["checkout", "-q", "FETCH_HEAD"]);
        super::worker::command_ok(checkout_head).await?;

        let unused_receipts = std::sync::Mutex::new(Vec::new());
        let run_id = format!("golden-{}", task.id);
        let attempt_id = uuid::Uuid::new_v4().to_string();
        let run = super::sandboxed::SandboxedRun {
            store,
            org_id,
            run_id: &run_id,
            attempt_id: &attempt_id,
            retry: 0,
            workdir: &workdir,
            secret_values: &[github_token.to_string()],
            seq_base: 0,
            wall_time: std::time::Duration::ZERO,
            verification: &[],
            receipts: &unused_receipts,
            qa: None,
            slot: "main",
            writes: None,
        };
        let runs = super::sandboxed::run_commands_sandboxed(
            &run,
            &super::sandboxed::CommandsSpec {
                commands: &commands,
                extra_env: &[],
                timeout_secs: COMMAND_TIMEOUT_SECS,
                reproduce_failures: false,
                hosts: &[],
                label: "g",
                max_stdout: crate::factory::sandbox_exec::DEFAULT_COMMAND_STDOUT,
                files: Vec::new(),
                proxy_file: None,
            },
        )
        .await?;
        (
            runs.iter().map(|run| run.receipt()).collect(),
            failure_output(&runs),
        )
    };
    let report =
        crate::factory::verification::build_report(&task.id, &task.merge_sha, &receipts, &[], &[])?;
    let db = store.conn();
    let conn = db.lock().map_err(|_| anyhow::anyhow!("database_lock"))?;
    crate::db::factory_queries::save_verification_report(&conn, org_id, REPLAY_RUN_ID, &report)?;
    Ok((report, output))
}

/// Installs and test suites of real repositories take longer than a verification
/// command's default.
const COMMAND_TIMEOUT_SECS: u64 = 900;

#[cfg(test)]
mod tests {
    use super::*;

    fn task(repository: &str, files: &[&str]) -> GoldenTask {
        GoldenTask {
            id: "24d42cfe-bc93-5820-beac-bec8a4ba20fe".into(),
            repository: repository.into(),
            merge_sha: "3de6eb345ad5b5bf294dc8aa016ba102d9c9be82".into(),
            changed_files: files.iter().map(|f| f.to_string()).collect(),
        }
    }

    fn argv(commands: &[Vec<String>]) -> Vec<String> {
        commands.iter().map(|c| c.join(" ")).collect()
    }

    #[test]
    fn nexus_mind_runs_the_js_suites_its_change_touches() {
        let admin = task(
            "smart-coder-labs/nexus-mind",
            &["apps/admin/src/App.tsx", "apps/backend/src/main.rs"],
        );
        assert_eq!(
            argv(&commands_for(&admin)),
            ["npm --prefix apps/admin ci", "npm --prefix apps/admin test"]
        );
        let both = task(
            "smart-coder-labs/nexus-mind",
            &["apps/backoffice/src/a.tsx", "apps/admin/src/b.tsx"],
        );
        assert_eq!(
            argv(&commands_for(&both)),
            [
                "npm --prefix apps/admin ci",
                "npm --prefix apps/admin test",
                "npm --prefix apps/backoffice ci",
                "npm --prefix apps/backoffice run build"
            ]
        );
    }

    #[test]
    fn rust_only_changes_have_no_executable_evidence() {
        let backend = task(
            "smart-coder-labs/nexus-mind",
            &["apps/backend/src/lib.rs", "openspec/x.md"],
        );
        assert!(commands_for(&backend).is_empty());
    }

    #[test]
    fn kasymir_installs_and_runs_jest() {
        let ui = task("kasymir/kasymir-app-ui", &["src/components/Button.tsx"]);
        assert_eq!(
            argv(&commands_for(&ui)),
            ["npm ci", "npm test -- --ci --passWithNoTests"]
        );
    }

    #[test]
    fn tasks_are_validated_before_any_pod_starts() {
        let good = task("kasymir/kasymir-app-ui", &["src/a"]);
        validate_task(&good).unwrap();
        for (mutate, code) in [
            (
                Box::new(|t: &mut GoldenTask| t.id = "not-a-uuid".into())
                    as Box<dyn Fn(&mut GoldenTask)>,
                "invalid_task_id",
            ),
            (
                Box::new(|t: &mut GoldenTask| t.id = t.id.to_uppercase()),
                "invalid_task_id",
            ),
            (
                Box::new(|t: &mut GoldenTask| t.merge_sha = "abc".into()),
                "invalid_merge_sha",
            ),
            (
                Box::new(|t: &mut GoldenTask| t.repository = "../etc".into()),
                "invalid_repository",
            ),
        ] {
            let mut bad = good.clone();
            mutate(&mut bad);
            assert_eq!(validate_task(&bad).unwrap_err().to_string(), code);
        }
    }

    #[test]
    fn a_failure_keeps_the_end_of_the_first_failing_output() {
        let run =
            |code: i32, stdout: &str, stderr: &str| crate::factory::sandbox_exec::CommandRun {
                argv: vec!["npm".into()],
                exit_code: Some(code),
                duration_ms: 1,
                stdout: stdout.as_bytes().to_vec(),
                stdout_truncated: false,
                stderr: stderr.as_bytes().to_vec(),
                reproduction: None,
            };
        assert_eq!(failure_output(&[run(0, "ok", "")]), None);
        let long = format!("{}npm error 404 Not Found", "x".repeat(2000));
        let tail =
            failure_output(&[run(0, "", ""), run(1, "", &long), run(1, "", "later")]).unwrap();
        assert!(tail.ends_with("npm error 404 Not Found"));
        assert_eq!(tail.len(), FAILURE_OUTPUT_BYTES);
        // Jest reports on stdout when stderr is empty.
        assert_eq!(
            failure_output(&[run(1, "Tests: 1 failed\n", "  \n")]).unwrap(),
            "Tests: 1 failed"
        );
    }

    #[test]
    fn unknown_repositories_are_not_guessed() {
        assert!(commands_for(&task("acme/other", &["src/a.ts"])).is_empty());
    }

    #[test]
    fn every_command_passes_the_verification_allowlist() {
        let all = [
            task(
                "smart-coder-labs/nexus-mind",
                &["apps/admin/a", "apps/backoffice/b"],
            ),
            task("kasymir/kasymir-app-ui", &["src/a"]),
        ];
        for t in &all {
            let value = serde_json::to_value(commands_for(t)).unwrap();
            crate::factory::verification::parse_verification_commands(Some(&value)).unwrap();
        }
    }
}
