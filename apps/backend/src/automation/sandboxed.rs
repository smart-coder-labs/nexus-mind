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

/// Lifetime of the commands pod: every command at its budget, twice when
/// failures are reproduced.
fn commands_lifetime(commands: usize, reproduce_failures: bool) -> Duration {
    let runs = commands as u64 * if reproduce_failures { 2 } else { 1 };
    READY_TIMEOUT
        + Duration::from_secs(SLACK_SECS + crate::factory::verification::COMMAND_BUDGET_SECS * runs)
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
        commands_lifetime(commands, reproduce_failures)
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

pub(crate) async fn run_claude_sandboxed(
    run: SandboxedRun<'_>,
    (argv, prompt): (Vec<String>, Option<Vec<u8>>),
) -> anyhow::Result<std::process::Output> {
    let image = std::env::var("FACTORY_SANDBOX_IMAGE")
        .map_err(|_| anyhow::anyhow!("sandbox_unconfigured"))?;
    let signing_key = std::env::var("FACTORY_PROXY_SIGNING_KEY")
        .ok()
        .filter(|key| key.len() >= 32)
        .ok_or_else(|| anyhow::anyhow!("sandbox_unconfigured"))?;
    let base_sha = {
        let output = Command::new("git")
            .current_dir(run.workdir)
            .args(["rev-parse", "HEAD"])
            .output()
            .await?;
        if !output.status.success() {
            anyhow::bail!("snapshot_resolution_failed")
        }
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    let workspace_tar = pack_workspace(run.workdir, run.secret_values).await?;
    // Kubernetes label values are at most 63 characters (RUN_LABEL).
    if run.run_id.len() > 63 {
        anyhow::bail!("run_id_too_long")
    }
    let now = chrono::Utc::now().timestamp();
    let attempt = run.attempt_id.chars().take(8).collect::<String>();
    let sign = |run_id: &str, lifetime: Duration| {
        sign_run_token(
            signing_key.as_bytes(),
            run.org_id,
            run_id,
            now + lifetime.as_secs() as i64,
        )
        .map_err(|error| anyhow::anyhow!("run_token_failed: {error:?}"))
    };
    let agent_life = agent_lifetime(run.wall_time);
    let run_token = sign(run.run_id, agent_life)?;
    let commands_life = commands_lifetime(run.verification.len(), false);
    // The registry-only token outlives both pods: the commands pod starts after
    // the agent pod.
    let registry_token = sign(
        &crate::factory::egress::registry_only_run_id(run.run_id),
        agent_life + commands_life,
    )?;
    // Everything a transcript or error could echo that is still live: redact it.
    let secrets: Vec<String> = run
        .secret_values
        .iter()
        .cloned()
        .chain([run_token.clone(), registry_token.clone()])
        .collect();
    let runtime = Arc::new(KubeRuntime::in_cluster().await?);
    let pod = |profile, token: &str, suffix: String, lifetime: Duration| {
        task_pod_manifest(&TaskPodRequest {
            org_id: run.org_id.to_string(),
            run_id: run.run_id.to_string(),
            pod_suffix: suffix,
            profile,
            image: image.clone(),
            run_token: token.to_string(),
            wall_time_secs: lifetime.as_secs() as i64,
        })
    };
    let job =
        |manifest, command, command_stdin, verification: Vec<Vec<String>>, lifetime: Duration| {
            SandboxJob {
                manifest,
                workspace_tar: workspace_tar.clone(),
                command,
                command_stdin,
                base_sha: base_sha.clone(),
                ready_timeout: READY_TIMEOUT,
                wall_time: lifetime,
                // Only read-only templates run here so far.
                collect_diff: false,
                verification,
                verification_env: crate::factory::sandbox::verification_env(&registry_token),
                command_timeout_secs: crate::factory::verification::COMMAND_TIMEOUT_SECS,
                reproduce_failures: false,
            }
        };

    // 1. The agent, in a pod that runs no repository code.
    let agent_job = job(
        pod(
            PodProfile::Agent,
            &run_token,
            format!("{attempt}-{}", run.retry),
            agent_life,
        ),
        Some(argv),
        prompt,
        Vec::new(),
        agent_life,
    );
    let mut sequence = run.seq_base;
    let (store, org_id, run_id) = (run.store, run.org_id, run.run_id);
    let secrets_ref = secrets.as_slice();
    let agent = run_in_sandbox(runtime.clone(), &agent_job, &mut |line: &[u8]| {
        super::worker::record_transcript_line(
            store,
            org_id,
            run_id,
            secrets_ref,
            &mut sequence,
            line,
        )
    })
    .await?;
    let output = agent
        .output
        .ok_or_else(|| anyhow::anyhow!("sandbox_agent_missing"))?;

    // 2. Repository commands, in their own pod holding only the registry token.
    // Output-format retries pass no commands: the head is the same, so the first
    // run's receipts stand.
    if !run.verification.is_empty() {
        let commands_job = job(
            pod(
                PodProfile::Commands,
                &registry_token,
                format!("{attempt}-{}-v", run.retry),
                commands_life,
            ),
            None,
            None,
            run.verification.to_vec(),
            commands_life,
        );
        let commands = run_in_sandbox(runtime, &commands_job, &mut |_: &[u8]| {}).await?;
        if let Ok(mut slot) = run.receipts.lock() {
            *slot = commands
                .verification
                .iter()
                .map(|run| run.receipt())
                .collect();
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
    fn exit_codes_survive_the_status_conversion() {
        assert!(exit_status(0).success());
        assert_eq!(exit_status(2).code(), Some(2));
        assert_eq!(exit_status(255).code(), Some(255));
    }
}
