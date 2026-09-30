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
    sandbox::{task_pod_manifest, TaskPodRequest},
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
}

/// The pod-side argv for a locally prepared Claude command: same arguments, the
/// binary resolved from the pod's `PATH`.
pub(crate) fn sandbox_argv(command: &Command) -> Vec<String> {
    std::iter::once("claude".to_string())
        .chain(
            command
                .as_std()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned()),
        )
        .collect()
}

/// How long the pod and its run token live for a run with `wall_time` for Claude:
/// scheduling and image pull, then the run, then unpack/cleanup slack.
pub(crate) fn sandbox_lifetime(wall_time: Duration) -> Duration {
    wall_time + READY_TIMEOUT + Duration::from_secs(SLACK_SECS)
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
    argv: Vec<String>,
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
    let pod_lifetime = sandbox_lifetime(run.wall_time).as_secs();
    let expires = chrono::Utc::now().timestamp() + pod_lifetime as i64;
    let run_token = sign_run_token(signing_key.as_bytes(), run.org_id, run.run_id, expires)
        .map_err(|error| anyhow::anyhow!("run_token_failed: {error:?}"))?;
    let job_token = run_token.clone();
    let manifest = task_pod_manifest(&TaskPodRequest {
        org_id: run.org_id.to_string(),
        run_id: format!(
            "{}-{}-{}",
            run.run_id,
            run.attempt_id.chars().take(8).collect::<String>(),
            run.retry
        ),
        image,
        run_token,
        wall_time_secs: pod_lifetime as i64,
    });
    let runtime = Arc::new(KubeRuntime::in_cluster().await?);
    let job = SandboxJob {
        manifest,
        workspace_tar,
        command: argv,
        base_sha,
        ready_timeout: READY_TIMEOUT,
        wall_time: Duration::from_secs(pod_lifetime),
        // Only read-only templates run here so far.
        collect_diff: false,
    };
    let mut sequence = run.seq_base;
    // The run token is live until the pod expires: redact it like any secret.
    let secrets: Vec<String> = run
        .secret_values
        .iter()
        .cloned()
        .chain(std::iter::once(job_token.clone()))
        .collect();
    let (store, org_id, run_id, secrets) = (run.store, run.org_id, run.run_id, secrets.as_slice());
    let result = run_in_sandbox(runtime, &job, &mut |line: &[u8]| {
        super::worker::record_transcript_line(store, org_id, run_id, secrets, &mut sequence, line)
    })
    .await?;
    Ok(std::process::Output {
        status: exit_status(result.output.exit_code),
        stdout: result.output.stdout,
        stderr: result.output.stderr,
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
    fn the_pod_argv_keeps_the_arguments_and_resolves_claude_from_path() {
        let mut command = Command::new("/usr/local/bin/claude");
        command.args(["-p", "review this", "--permission-mode", "plan"]);
        assert_eq!(
            sandbox_argv(&command),
            ["claude", "-p", "review this", "--permission-mode", "plan"]
        );
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
