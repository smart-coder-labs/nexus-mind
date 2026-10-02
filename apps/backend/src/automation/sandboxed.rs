//! Runs a Claude invocation in an ephemeral task pod instead of the worker
//! container (factory F1). It returns the same `std::process::Output` as the
//! local runner, so the worker classifies both paths with the same code.
//!
//! Configuration (worker environment):
//! - `FACTORY_SANDBOX_IMAGE`: task pod image (v1: the backend image).
//! - `FACTORY_PROXY_SIGNING_KEY`: shared with the egress proxy, at least 32 bytes.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use tokio::process::Command;

use crate::factory::{
    egress::sign_run_token,
    sandbox::{task_pod_manifest, PodProfile, TaskPodRequest},
    sandbox_exec::{run_in_sandbox, KubeRuntime, SandboxJob},
    workspace::pack_workspace,
};
use crate::store::sqlite::SqliteStore;

/// WIP checkpoint interval, as on the local path.
const CHECKPOINT_EVERY: Duration = Duration::from_secs(180);

/// How long a pod may take to be scheduled and pull its image.
const READY_TIMEOUT: Duration = Duration::from_secs(180);
/// Extra lifetime for the pod and token beyond the run's wall time (unpack, diff).
const SLACK_SECS: u64 = 120;

pub(crate) struct SandboxedRun<'a> {
    pub store: &'a SqliteStore,
    pub org_id: &'a str,
    pub run_id: &'a str,
    /// Lease attempt: a run reclaimed after a worker crash gets a new one, so its
    /// pod never collides with the orphan the crashed worker left.
    pub attempt_id: &'a str,
    /// Output-format retry within one attempt.
    pub retry: u32,
    pub workdir: &'a Path,
    pub secret_values: &'a [String],
    pub seq_base: i64,
    pub wall_time: Duration,
    /// Allowlisted verification commands to run in the pod.
    pub verification: &'a [Vec<String>],
    /// Receives the verification receipts when the run completes.
    pub receipts: &'a std::sync::Mutex<Vec<crate::factory::verification::VerificationReceipt>>,
    /// Browser-driven templates (QA, judge): target hosts and where screenshots go.
    pub qa: Option<QaSandbox<'a>>,
    /// The pod slot within the run (`main`, or `issue-<n>` in a resolver fanout).
    pub slot: &'a str,
    /// Code-writing templates: every diff of the agent's work, periodic and
    /// final, goes to this channel in order. The caller applies them to its
    /// checkout (and pushes WIP checkpoints); when the run returns, the channel is
    /// closed and the last diff received is the result.
    pub writes: Option<tokio::sync::mpsc::UnboundedSender<SandboxWrite>>,
}

/// A diff of the agent's work, in the order it was taken.
#[derive(Debug, PartialEq)]
pub(crate) enum SandboxWrite {
    /// Work in progress (periodic, or the last state of a timed-out run).
    Checkpoint(Vec<u8>),
    /// The finished run's change.
    Final(Vec<u8>),
}

pub(crate) struct QaSandbox<'a> {
    pub hosts: &'a [String],
    pub artifacts_dir: &'a Path,
}

/// The pod-side invocation for a locally prepared Claude command: the same
/// arguments with the binary resolved from the pod's `PATH`, and the `-p` prompt
/// moved to stdin so it never travels in the exec request.
pub(crate) fn sandbox_invocation(command: &Command) -> (Vec<String>, Option<Vec<u8>>) {
    let mut argv = vec!["claude".to_string()];
    let mut prompt = None;
    let mut args = command
        .as_std()
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned());
    while let Some(arg) = args.next() {
        if arg == "-p" && prompt.is_none() {
            argv.push(arg);
            prompt = args.next().map(String::into_bytes);
        } else {
            argv.push(arg);
        }
    }
    (argv, prompt)
}

/// Lifetime of the agent pod and its token: scheduling and image pull, the run
/// with its full `wall_time`, then cleanup slack.
fn agent_lifetime(wall_time: Duration) -> Duration {
    wall_time + READY_TIMEOUT + Duration::from_secs(SLACK_SECS)
}

/// Lifetime of a commands pod: every command at its timeout plus the kill
/// grace, twice when failures are reproduced.
fn commands_lifetime(commands: usize, reproduce_failures: bool, timeout_secs: u64) -> Duration {
    let runs = commands as u64 * if reproduce_failures { 2 } else { 1 };
    READY_TIMEOUT + Duration::from_secs(SLACK_SECS + (timeout_secs + 10) * runs)
}

/// Total budget of a sandboxed run: the agent pod, then the commands pod if any.
pub(crate) fn sandbox_lifetime(
    wall_time: Duration,
    commands: usize,
    reproduce_failures: bool,
) -> Duration {
    let commands = if commands == 0 {
        Duration::ZERO
    } else {
        commands_lifetime(
            commands,
            reproduce_failures,
            crate::factory::verification::COMMAND_TIMEOUT_SECS,
        )
    };
    agent_lifetime(wall_time) + commands
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    // A wait status carries the exit code in its second byte.
    std::process::ExitStatus::from_raw((code & 0xff) << 8)
}

/// The error as a tenant-visible code: our own codes pass through, anything else
/// (kube or I/O text with cluster details) is logged here and reported generically.
pub(crate) fn failure_code(run_id: &str, error: &anyhow::Error) -> String {
    let message = error.to_string();
    if !message.is_empty() && message.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        message
    } else {
        tracing::warn!(run_id, "sandbox run failed: {error:#}");
        "sandbox_internal_error".to_string()
    }
}

/// What every sandboxed pod of a run shares.
struct Prepared {
    image: String,
    signing_key: String,
    workspace_tar: Vec<u8>,
    runtime: Arc<KubeRuntime>,
    attempt: String,
    now: i64,
}

async fn prepare(run: &SandboxedRun<'_>) -> anyhow::Result<Prepared> {
    let image = std::env::var("FACTORY_SANDBOX_IMAGE")
        .map_err(|_| anyhow::anyhow!("sandbox_unconfigured"))?;
    let signing_key = std::env::var("FACTORY_PROXY_SIGNING_KEY")
        .ok()
        .filter(|key| key.len() >= 32)
        .ok_or_else(|| anyhow::anyhow!("sandbox_unconfigured"))?;
    // Kubernetes label values are at most 63 characters (RUN_LABEL).
    if run.run_id.len() > 63 {
        anyhow::bail!("run_id_too_long")
    }
    Ok(Prepared {
        image,
        signing_key,
        workspace_tar: pack_workspace(run.workdir, run.secret_values).await?,
        runtime: Arc::new(KubeRuntime::in_cluster().await?),
        attempt: run.attempt_id.chars().take(8).collect(),
        now: chrono::Utc::now().timestamp(),
    })
}

impl Prepared {
    /// A run token living `lifetime`, carrying `hosts` (v3) when there are any.
    fn token(
        &self,
        org_id: &str,
        run_id: &str,
        lifetime: Duration,
        hosts: &[String],
    ) -> anyhow::Result<String> {
        let expires = self.now + lifetime.as_secs() as i64;
        let key = self.signing_key.as_bytes();
        if hosts.is_empty() {
            sign_run_token(key, org_id, run_id, expires)
        } else {
            crate::factory::egress::sign_run_token_with_hosts(key, org_id, run_id, expires, hosts)
        }
        .map_err(|error| anyhow::anyhow!("run_token_failed: {error:?}"))
    }

    fn job(
        &self,
        run: &SandboxedRun<'_>,
        profile: PodProfile,
        token: &str,
        suffix: String,
        lifetime: Duration,
    ) -> SandboxJob {
        SandboxJob {
            manifest: task_pod_manifest(&TaskPodRequest {
                org_id: run.org_id.to_string(),
                run_id: run.run_id.to_string(),
                pod_suffix: format!("{}-{}-{suffix}", self.attempt, run.slot),
                profile,
                slot: run.slot.to_string(),
                image: self.image.clone(),
                run_token: token.to_string(),
                wall_time_secs: lifetime.as_secs() as i64,
            }),
            workspace_tar: self.workspace_tar.clone(),
            command: None,
            command_stdin: None,
            ready_timeout: READY_TIMEOUT,
            wall_time: lifetime,
            collect_diff: false,
            verification: Vec::new(),
            verification_env: Vec::new(),
            command_timeout_secs: crate::factory::verification::COMMAND_TIMEOUT_SECS,
            reproduce_failures: false,
            command_stdout_cap: crate::factory::sandbox_exec::DEFAULT_COMMAND_STDOUT,
            files: Vec::new(),
            collect_dir: None,
            checkpoints: None,
        }
    }
}

/// Repository commands in a commands pod: only a registry-only token (plus the
/// run's target hosts, for end-to-end tests) and `extra_env` on stdin.
pub(crate) struct CommandsSpec<'a> {
    pub commands: &'a [Vec<String>],
    pub extra_env: &'a [(String, String)],
    pub timeout_secs: u64,
    pub reproduce_failures: bool,
    pub hosts: &'a [String],
    /// Distinguishes this commands pod from the run's others (e.g. "t", "v").
    pub label: &'a str,
    /// Most stdout kept per command.
    pub max_stdout: usize,
    /// Files written into the pod before the commands run (path, content).
    pub files: Vec<(String, Vec<u8>)>,
    /// Write `http://run:<pod token>@<proxy>` to this path, for tools that take
    /// their proxy from a file rather than the environment (nuclei).
    pub proxy_file: Option<&'a str>,
}

pub(crate) async fn run_commands_sandboxed(
    run: &SandboxedRun<'_>,
    spec: &CommandsSpec<'_>,
) -> anyhow::Result<Vec<crate::factory::sandbox_exec::CommandRun>> {
    let prepared = prepare(run).await?;
    run_commands(&prepared, run, spec).await
}

async fn run_commands(
    prepared: &Prepared,
    run: &SandboxedRun<'_>,
    spec: &CommandsSpec<'_>,
) -> anyhow::Result<Vec<crate::factory::sandbox_exec::CommandRun>> {
    let lifetime = commands_lifetime(
        spec.commands.len(),
        spec.reproduce_failures,
        spec.timeout_secs,
    );
    let token = prepared.token(
        run.org_id,
        &crate::factory::egress::registry_only_run_id(run.run_id),
        lifetime,
        spec.hosts,
    )?;
    let mut job = prepared.job(
        run,
        PodProfile::Commands,
        &token,
        format!("{}-{}", run.retry, spec.label),
        lifetime,
    );
    job.verification = spec.commands.to_vec();
    job.verification_env = crate::factory::sandbox::verification_env(&token);
    job.verification_env.extend(spec.extra_env.iter().cloned());
    job.command_timeout_secs = spec.timeout_secs;
    job.reproduce_failures = spec.reproduce_failures;
    job.command_stdout_cap = spec.max_stdout;
    job.files = spec.files.clone();
    // Private npm scopes resolve through the proxy's read-only GitHub Packages
    // route. The pod's HOME is /tmp in a commands pod (verification_env).
    let scopes = workspace_npm_scopes(run.workdir);
    if !scopes.is_empty() {
        let registry = format!(
            "http://{}/r/{token}/ghpkg/",
            crate::factory::sandbox::PROXY_AUTHORITY
        );
        job.files.push((
            "/tmp/.npmrc".to_string(),
            github_packages_npmrc(&scopes, &registry).into_bytes(),
        ));
    }
    if let Some(path) = spec.proxy_file {
        let proxy = format!(
            "http://run:{token}@{}\n",
            crate::factory::sandbox::PROXY_AUTHORITY
        );
        job.files.push((path.to_string(), proxy.into_bytes()));
    }
    let mut runs = run_in_sandbox(prepared.runtime.clone(), &job, &mut |_: &[u8]| {})
        .await?
        .verification;
    // Test output may echo the environment; the pod's token is live until expiry.
    for run in &mut runs {
        scrub(run, token.as_bytes());
    }
    Ok(runs)
}

fn scrub(run: &mut crate::factory::sandbox_exec::CommandRun, secret: &[u8]) {
    fn replace(haystack: &mut Vec<u8>, secret: &[u8]) {
        if secret.is_empty() || memchr::memmem::find(haystack, secret).is_none() {
            return;
        }
        let mut out = Vec::with_capacity(haystack.len());
        let mut rest = haystack.as_slice();
        while let Some(at) = memchr::memmem::find(rest, secret) {
            out.extend_from_slice(&rest[..at]);
            out.extend_from_slice(b"[REDACTED]");
            rest = &rest[at + secret.len()..];
        }
        out.extend_from_slice(rest);
        *haystack = out;
    }
    replace(&mut run.stdout, secret);
    replace(&mut run.stderr, secret);
    if let Some(again) = run.reproduction.as_deref_mut() {
        scrub(again, secret);
    }
}

/// The npm scopes a lockfile resolves from GitHub Packages (lockfile v2/v3
/// `packages`, or v1 `dependencies`).
pub(crate) fn github_packages_scopes(
    lockfile: &serde_json::Value,
) -> std::collections::BTreeSet<String> {
    fn from_github(entry: &serde_json::Value) -> bool {
        entry
            .get("resolved")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|url| url.starts_with("https://npm.pkg.github.com/"))
    }
    fn scope_of(name: &str) -> Option<String> {
        // The last `node_modules/` segment is the package; scoped names start with @.
        let package = name.rsplit("node_modules/").next().unwrap_or(name);
        package
            .strip_prefix('@')
            .and_then(|rest| rest.split('/').next())
            .map(|scope| format!("@{scope}"))
    }
    fn walk_v1(deps: &serde_json::Value, scopes: &mut std::collections::BTreeSet<String>) {
        for (name, entry) in deps.as_object().into_iter().flatten() {
            if from_github(entry) {
                scopes.extend(scope_of(name));
            }
            if let Some(nested) = entry.get("dependencies") {
                walk_v1(nested, scopes);
            }
        }
    }
    let mut scopes = std::collections::BTreeSet::new();
    for (name, entry) in lockfile
        .get("packages")
        .and_then(|p| p.as_object())
        .into_iter()
        .flatten()
    {
        if from_github(entry) {
            scopes.extend(scope_of(name));
        }
    }
    if let Some(deps) = lockfile.get("dependencies") {
        walk_v1(deps, &mut scopes);
    }
    scopes
}

/// GitHub Packages scopes across the workspace's `package-lock.json` files (up to
/// three levels deep, never inside `node_modules` or `.git`).
fn workspace_npm_scopes(workdir: &Path) -> std::collections::BTreeSet<String> {
    fn walk(dir: &Path, depth: usize, scopes: &mut std::collections::BTreeSet<String>) {
        let lock = dir.join("package-lock.json");
        if let Ok(raw) = std::fs::read_to_string(&lock) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                scopes.extend(github_packages_scopes(&value));
            }
        }
        if depth == 0 {
            return;
        }
        for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let skip = name == "node_modules" || name == ".git";
            if !skip && entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                walk(&entry.path(), depth - 1, scopes);
            }
        }
    }
    let mut scopes = std::collections::BTreeSet::new();
    walk(workdir, 3, &mut scopes);
    scopes
}

/// A token-free `.npmrc` that sends those scopes to the proxy's GitHub Packages
/// route; `replace-registry-host` rewrites the lockfile's tarball URLs too.
pub(crate) fn github_packages_npmrc(
    scopes: &std::collections::BTreeSet<String>,
    registry_url: &str,
) -> String {
    let mut npmrc = String::new();
    for scope in scopes {
        npmrc.push_str(&format!("{scope}:registry={registry_url}\n"));
    }
    npmrc.push_str("replace-registry-host=npm.pkg.github.com\n");
    npmrc
}

/// A scanner's report from a commands pod, or why there is none. The worker only
/// parses this output; it never runs the scanner itself for a sandboxed run.
pub(crate) fn scanner_output(
    run: &crate::factory::sandbox_exec::CommandRun,
) -> anyhow::Result<Vec<u8>> {
    match run.exit_code {
        None => anyhow::bail!("scanner_failed"),
        // `timeout` exits 124, or 137 when it had to kill.
        Some(124 | 137) => anyhow::bail!("scanner_timeout"),
        Some(126 | 127) => anyhow::bail!("scanner_unavailable"),
        Some(_) if run.stdout_truncated => anyhow::bail!("scanner_output_too_large"),
        Some(_) => Ok(run.stdout.clone()),
    }
}

/// A reachability probe for a scan target, run through the proxy before the
/// scanner: a target the sandbox cannot reach must fail the scan, not produce an
/// empty, clean-looking report.
pub(crate) fn reachability_probe(host: &str) -> Vec<String> {
    // curl takes the proxy from HTTPS_PROXY; it fails on transport errors only.
    ["curl", "-sS", "-o", "/dev/null", "--max-time", "20"]
        .iter()
        .map(|part| part.to_string())
        .chain(std::iter::once(format!("https://{host}/")))
        .collect()
}

/// Every probe must have connected (any HTTP status is fine).
pub(crate) fn check_probes(
    runs: &[crate::factory::sandbox_exec::CommandRun],
) -> anyhow::Result<()> {
    if runs.iter().all(|run| run.exit_code == Some(0)) {
        Ok(())
    } else {
        anyhow::bail!("dast_target_unreachable")
    }
}

pub(crate) async fn run_claude_sandboxed(
    run: SandboxedRun<'_>,
    (mut argv, prompt): (Vec<String>, Option<Vec<u8>>),
) -> anyhow::Result<std::process::Output> {
    let prepared = prepare(&run).await?;
    let agent_life = agent_lifetime(run.wall_time);
    let hosts: &[String] = run.qa.as_ref().map_or(&[], |qa| qa.hosts);
    let run_token = prepared.token(run.org_id, run.run_id, agent_life, hosts)?;
    // Everything a transcript or error could echo that is still live: redact it.
    let secrets: Vec<String> = run
        .secret_values
        .iter()
        .cloned()
        .chain([run_token.clone()])
        .collect();

    // 1. The agent, in a pod that runs no repository code.
    let mut agent_job = prepared.job(
        &run,
        PodProfile::Agent,
        &run_token,
        run.retry.to_string(),
        agent_life,
    );
    if run.qa.is_some() {
        // The browser reaches the targets through the proxy with the run token,
        // which goes into a file in the pod, never into the exec request.
        let (mcp, config) = crate::factory::sandbox::playwright_mcp(&run_token);
        argv.extend(["--mcp-config".to_string(), mcp]);
        agent_job.files = vec![(
            crate::factory::sandbox::PLAYWRIGHT_CONFIG_PATH.to_string(),
            config,
        )];
        agent_job.collect_dir = Some(crate::factory::sandbox::QA_OUTPUT_DIR.to_string());
    }
    // Checkpoint diffs from the executor are forwarded in order; the forwarder
    // ends (its channel closes with the job) before the final diff is sent.
    let forwarder = run.writes.as_ref().map(|sink| {
        argv.extend([
            "--mcp-config".to_string(),
            crate::factory::sandbox::nexusmind_mcp(),
        ]);
        agent_job.collect_diff = true;
        let (inner, mut received) = tokio::sync::mpsc::unbounded_channel();
        agent_job.checkpoints = Some(crate::factory::sandbox_exec::Checkpoints {
            every: CHECKPOINT_EVERY,
            sink: inner,
        });
        let sink = sink.clone();
        tokio::spawn(async move {
            while let Some(diff) = received.recv().await {
                let _ = sink.send(SandboxWrite::Checkpoint(diff));
            }
        })
    });
    agent_job.command = Some(argv);
    agent_job.command_stdin = prompt;
    let mut sequence = run.seq_base;
    let (store, org_id, run_id) = (run.store, run.org_id, run.run_id);
    let secrets_ref = secrets.as_slice();
    let agent = run_in_sandbox(
        prepared.runtime.clone(),
        &agent_job,
        &mut |line: &[u8]| {
            super::worker::record_transcript_line(
                store,
                org_id,
                run_id,
                secrets_ref,
                &mut sequence,
                line,
            )
        },
    )
    .await?;
    if let Some(qa) = &run.qa {
        tokio::fs::create_dir_all(qa.artifacts_dir).await?;
        for (name, content) in &agent.artifacts {
            // Names were validated as plain file names by the executor.
            tokio::fs::write(qa.artifacts_dir.join(name), content).await?;
        }
    }
    drop(agent_job);
    if let (Some(sink), Some(forwarder)) = (&run.writes, forwarder) {
        let _ = forwarder.await;
        // The final state, after every checkpoint already sent.
        let _ = sink.send(SandboxWrite::Final(agent.diff.clone()));
    }
    let output = agent
        .output
        .ok_or_else(|| anyhow::anyhow!("sandbox_agent_missing"))?;

    // 2. Verification commands, in their own pod holding only the registry token.
    // Output-format retries pass no commands: the head is the same, so the first
    // run's receipts stand.
    if !run.verification.is_empty() {
        let spec = CommandsSpec {
            commands: run.verification,
            extra_env: &[],
            timeout_secs: crate::factory::verification::COMMAND_TIMEOUT_SECS,
            reproduce_failures: false,
            hosts: &[],
            label: "v",
            max_stdout: crate::factory::sandbox_exec::DEFAULT_COMMAND_STDOUT,
            files: Vec::new(),
            proxy_file: None,
        };
        let runs = run_commands(&prepared, &run, &spec).await?;
        if let Ok(mut slot) = run.receipts.lock() {
            *slot = runs.iter().map(|run| run.receipt()).collect();
        }
    }
    Ok(std::process::Output {
        status: exit_status(output.exit_code),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

/// Worker tick: deletes task pods a crashed or cancelled run left behind. A no-op
/// until the sandbox is configured.
pub(crate) async fn gc_task_pods() {
    if std::env::var_os("FACTORY_SANDBOX_IMAGE").is_none() {
        return;
    }
    let sweep = async {
        let runtime = KubeRuntime::in_cluster().await?;
        crate::factory::sandbox_exec::gc_expired_task_pods(&runtime, chrono::Utc::now().timestamp())
            .await
    };
    // A hung API server must not stall the worker loop.
    let result = tokio::time::timeout(Duration::from_secs(30), sweep)
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("gc_timeout")));
    match result {
        Ok(0) => {}
        Ok(deleted) => tracing::info!(deleted, "deleted expired task pods"),
        Err(error) => tracing::warn!("task pod gc failed: {error:#}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prompt_moves_to_stdin_and_claude_resolves_from_path() {
        let mut command = Command::new("/usr/local/bin/claude");
        command.args(["-p", "review this", "--permission-mode", "plan"]);
        let (argv, prompt) = sandbox_invocation(&command);
        assert_eq!(argv, ["claude", "-p", "--permission-mode", "plan"]);
        assert_eq!(prompt.as_deref(), Some(&b"review this"[..]));
    }

    #[test]
    fn only_our_own_codes_reach_the_tenant() {
        assert_eq!(
            failure_code("r", &anyhow::anyhow!("pod_not_ready")),
            "pod_not_ready"
        );
        assert_eq!(
            failure_code(
                "r",
                &anyhow::anyhow!("ApiError: pods \"task-x\" already exists at https://10.43.0.1")
            ),
            "sandbox_internal_error"
        );
    }

    #[test]
    fn command_output_never_carries_the_pod_token() {
        let mut run = crate::factory::sandbox_exec::CommandRun {
            stdout: b"HTTPS_PROXY=http://run:v2.o.r-registry.1.sig@p and v2.o.r-registry.1.sig"
                .to_vec(),
            stderr: b"clean".to_vec(),
            reproduction: Some(Box::new(crate::factory::sandbox_exec::CommandRun {
                stderr: b"v2.o.r-registry.1.sig".to_vec(),
                ..Default::default()
            })),
            ..Default::default()
        };
        scrub(&mut run, b"v2.o.r-registry.1.sig");
        assert_eq!(
            run.stdout,
            b"HTTPS_PROXY=http://run:[REDACTED]@p and [REDACTED]"
        );
        assert_eq!(run.stderr, b"clean");
        assert_eq!(run.reproduction.unwrap().stderr, b"[REDACTED]");
    }

    #[test]
    fn scanner_runs_are_classified_without_guessing() {
        use crate::factory::sandbox_exec::CommandRun;
        let run = |exit_code: Option<i32>, truncated: bool| CommandRun {
            exit_code,
            stdout: b"{}".to_vec(),
            stdout_truncated: truncated,
            ..Default::default()
        };
        // Scanners exit non-zero when they find something: the report still counts.
        assert_eq!(scanner_output(&run(Some(0), false)).unwrap(), b"{}");
        assert_eq!(scanner_output(&run(Some(1), false)).unwrap(), b"{}");
        for (exit_code, truncated, code) in [
            (Some(124), false, "scanner_timeout"),
            (Some(137), false, "scanner_timeout"),
            (Some(127), false, "scanner_unavailable"),
            (Some(126), false, "scanner_unavailable"),
            (None, false, "scanner_failed"),
            (Some(0), true, "scanner_output_too_large"),
        ] {
            assert_eq!(
                scanner_output(&run(exit_code, truncated))
                    .unwrap_err()
                    .to_string(),
                code
            );
        }
    }

    #[test]
    fn unreachable_scan_targets_fail_the_scan() {
        use crate::factory::sandbox_exec::CommandRun;
        let probe = reachability_probe("app.acme.test");
        assert_eq!(probe.first().map(String::as_str), Some("curl"));
        assert_eq!(
            probe.last().map(String::as_str),
            Some("https://app.acme.test/")
        );
        let run = |exit_code| CommandRun {
            exit_code,
            ..Default::default()
        };
        assert!(check_probes(&[run(Some(0)), run(Some(0))]).is_ok());
        for bad in [Some(56), Some(7), None] {
            assert_eq!(
                check_probes(&[run(Some(0)), run(bad)])
                    .unwrap_err()
                    .to_string(),
                "dast_target_unreachable"
            );
        }
    }

    #[test]
    fn private_scopes_come_from_github_packages_resolutions_only() {
        let v3 = serde_json::json!({"packages": {
            "": {"name": "app"},
            "node_modules/react": {"resolved": "https://registry.npmjs.org/react/-/react-18.2.0.tgz"},
            "node_modules/@xell-shop/ui": {"resolved": "https://npm.pkg.github.com/download/@xell-shop/ui/1.0.0/abc"},
            "node_modules/a/node_modules/@kasymir/ui-commons": {"resolved": "https://npm.pkg.github.com/download/@kasymir/ui-commons/2.0.0/def"},
            "node_modules/@types/node": {"resolved": "https://registry.npmjs.org/@types/node/-/node-20.0.0.tgz"}
        }});
        let scopes: Vec<String> = github_packages_scopes(&v3).into_iter().collect();
        assert_eq!(scopes, ["@kasymir", "@xell-shop"]);
        let v1 = serde_json::json!({"dependencies": {
            "@byte4bit-fenextjs/core": {"resolved": "https://npm.pkg.github.com/download/@byte4bit-fenextjs/core/1.0.0/x",
                "dependencies": {"@acme/inner": {"resolved": "https://npm.pkg.github.com/download/@acme/inner/1.0.0/y"}}}
        }});
        let scopes: Vec<String> = github_packages_scopes(&v1).into_iter().collect();
        assert_eq!(scopes, ["@acme", "@byte4bit-fenextjs"]);
        assert!(github_packages_scopes(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn the_npmrc_points_scopes_at_the_proxy_without_a_credential() {
        let scopes: std::collections::BTreeSet<String> =
            ["@kasymir".to_string(), "@xell-shop".to_string()]
                .into_iter()
                .collect();
        let npmrc = github_packages_npmrc(&scopes, "http://proxy:8080/r/TOKEN/ghpkg/");
        assert!(
            npmrc.contains("@kasymir:registry=http://proxy:8080/r/TOKEN/ghpkg/\n"),
            "{npmrc}"
        );
        assert!(
            npmrc.contains("@xell-shop:registry=http://proxy:8080/r/TOKEN/ghpkg/\n"),
            "{npmrc}"
        );
        assert!(
            npmrc.contains("replace-registry-host=npm.pkg.github.com\n"),
            "{npmrc}"
        );
        assert!(!npmrc.contains("_authToken"), "{npmrc}");
    }

    #[test]
    fn exit_codes_survive_the_status_conversion() {
        assert!(exit_status(0).success());
        assert_eq!(exit_status(2).code(), Some(2));
        assert_eq!(exit_status(255).code(), Some(255));
    }
}
