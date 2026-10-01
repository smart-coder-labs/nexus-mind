//! Driving a task pod (factory F1, design §4): create it, stream the workspace in,
//! run the task with streamed output, take the diff out, and always delete it.
//!
//! [`SandboxRuntime`] abstracts the cluster so this orchestration is tested with a
//! fake; [`KubeRuntime`] is the in-cluster implementation, exercised by the F1
//! drill against the real cluster.

use std::time::Duration;

use axum::async_trait;
use serde_json::Value;

/// Largest diff accepted back from a task: anything bigger is refused, not truncated.
pub const MAX_DIFF_BYTES: usize = 5 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodHandle {
    pub namespace: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecOutput {
    pub exit_code: i32,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[async_trait]
pub trait SandboxRuntime: Send + Sync {
    async fn create(&self, manifest: &Value) -> anyhow::Result<PodHandle>;
    async fn wait_ready(&self, pod: &PodHandle, timeout: Duration) -> anyhow::Result<()>;
    /// Runs `argv` in the pod. `stdin` is written then closed. Every stdout line is
    /// passed to `on_line` as it arrives (for transcript capture).
    async fn exec(
        &self,
        pod: &PodHandle,
        argv: &[String],
        stdin: Option<Vec<u8>>,
        on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
    ) -> anyhow::Result<ExecOutput>;
    async fn delete(&self, pod: &PodHandle) -> anyhow::Result<()>;
    /// Every task pod in the sandbox namespace (`role=task`), for the GC sweep.
    async fn list_task_pods(&self) -> anyhow::Result<Vec<TaskPodInfo>>;
}

#[derive(Clone, Debug)]
pub struct TaskPodInfo {
    pub handle: PodHandle,
    /// The pod's slot within its run (`factory.nexusmind/slot`): parallel pods of
    /// one run (resolver fanout) never treat each other as orphans.
    pub slot: Option<String>,
    /// The run the pod belongs to (`factory.nexusmind/run` label).
    pub run_id: Option<String>,
    pub created_unix: i64,
    pub deadline_secs: Option<i64>,
}

/// Grace after a pod's deadline before the sweep deletes it.
pub const GC_GRACE_SECS: i64 = 300;
/// Deadline assumed for a task pod that lacks one (the worker's maximum wall time).
const FALLBACK_DEADLINE_SECS: i64 = 3600;

/// Deletes task pods left behind by a crashed or cancelled worker. Returns how
/// many it deleted.
pub async fn gc_expired_task_pods(
    runtime: &dyn SandboxRuntime,
    now_unix: i64,
) -> anyhow::Result<usize> {
    let mut deleted = 0;
    for pod in runtime.list_task_pods().await? {
        let deadline = pod.deadline_secs.unwrap_or(FALLBACK_DEADLINE_SECS);
        if pod.created_unix + deadline + GC_GRACE_SECS < now_unix {
            // One stuck pod must not stop the sweep of the others.
            match runtime.delete(&pod.handle).await {
                Ok(()) => deleted += 1,
                Err(error) => {
                    tracing::warn!(target: "factory_sandbox", pod = %pod.handle.name, "gc delete failed: {error:#}")
                }
            }
        }
    }
    Ok(deleted)
}

async fn delete_run_orphans(runtime: &dyn SandboxRuntime, run_id: &str, slot: Option<&str>) {
    let pods = match runtime.list_task_pods().await {
        Ok(pods) => pods,
        Err(error) => {
            tracing::warn!(target: "factory_sandbox", run_id, "orphan listing failed: {error:#}");
            return;
        }
    };
    for pod in pods
        .into_iter()
        .filter(|pod| pod.run_id.as_deref() == Some(run_id) && pod.slot.as_deref() == slot)
    {
        // Best effort: the GC sweep removes anything left behind.
        if let Err(error) = runtime.delete(&pod.handle).await {
            tracing::warn!(target: "factory_sandbox", pod = %pod.handle.name, "orphan delete failed: {error:#}");
        }
    }
}

/// Deletes the pod if the job future is dropped before it finishes (worker
/// timeout, operator cancel). The normal path disarms it and deletes inline.
struct PodGuard {
    runtime: std::sync::Arc<dyn SandboxRuntime>,
    pod: Option<PodHandle>,
}

impl Drop for PodGuard {
    fn drop(&mut self) {
        let Some(pod) = self.pod.take() else { return };
        let runtime = self.runtime.clone();
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    if let Err(error) = runtime.delete(&pod).await {
                        tracing::warn!(target: "factory_sandbox", pod = %pod.name, "delete after cancel failed: {error:#}");
                    }
                });
            }
            // No runtime to delete from: the GC sweep removes it.
            Err(_) => tracing::warn!(target: "factory_sandbox", pod = %pod.name, "pod left for gc"),
        }
    }
}

/// What one sandboxed task needs.
pub struct SandboxJob {
    pub manifest: Value,
    /// `tar` of the checkout, without credentials (see design §4).
    pub workspace_tar: Vec<u8>,
    /// The agent. `None` for a commands-only pod (QA tests run apart from the agent).
    pub command: Option<Vec<String>>,
    /// Written to the agent's stdin: the prompt, so it never travels in the exec URL.
    pub command_stdin: Option<Vec<u8>>,
    /// Commit the checkout is at; the diff is taken against it.
    pub base_sha: String,
    pub ready_timeout: Duration,
    /// Upper bound for the whole job (unpack, command, diff) before the pod is deleted.
    pub wall_time: Duration,
    /// Read-only templates skip the diff: nothing they could change is kept.
    pub collect_diff: bool,
    /// Allowlisted commands (see `verification::parse_verification_commands`).
    /// They run after the agent and after the diff, so code under test can never
    /// influence what the agent sees or what change is kept.
    pub verification: Vec<Vec<String>>,
    /// The whole environment of a verification command (`env -i`): it never
    /// inherits the pod's run token, only a registry-only one.
    pub verification_env: Vec<(String, String)>,
    /// Per-command limit, seconds.
    pub command_timeout_secs: u64,
    /// Re-run a failing command once and keep both outcomes (QA flakiness).
    pub reproduce_failures: bool,
    /// Files written into the pod after the workspace (path, content), on stdin.
    pub files: Vec<(String, Vec<u8>)>,
    /// A pod directory whose artifacts (screenshots) are returned after the agent.
    pub collect_dir: Option<String>,
    /// Periodic diffs of the agent's work while it runs (WIP checkpoints), and a
    /// last one when the job times out. Requires `collect_diff`.
    pub checkpoints: Option<Checkpoints>,
}

pub struct Checkpoints {
    pub every: Duration,
    pub sink: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
}

/// How long the last diff of a timed-out job may take.
const FINAL_DIFF_TIMEOUT: Duration = Duration::from_secs(60);

/// Most artifacts returned from a pod, and their total size.
pub const MAX_ARTIFACTS: usize = 50;
pub const MAX_ARTIFACT_BYTES: usize = 20 * 1024 * 1024;

/// Writes stdin to the path in argv[1].
const WRITE_FILE: &str =
    "import sys\nwith open(sys.argv[1],'wb') as f: f.write(sys.stdin.buffer.read())";
/// Prints `name<TAB>base64` for the image files of argv[1] (regular files only,
/// no links), stopping at the byte budget so one exec stays under its output cap.
const LIST_ARTIFACTS: &str = "import base64,os,sys\nd=sys.argv[1]\nleft=20971520\nif os.path.isdir(d):\n    for n in sorted(os.listdir(d))[:50]:\n        p=os.path.join(d,n)\n        if not n.lower().endswith(('.png','.jpg','.jpeg','.webp')) or os.path.islink(p) or not os.path.isfile(p):\n            continue\n        size=os.path.getsize(p)\n        if size>5242880 or size>left:\n            continue\n        left-=size\n        print(n+'\\t'+base64.b64encode(open(p,'rb').read()).decode())";

/// One command run in the pod, with its (bounded) output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CommandRun {
    pub argv: Vec<String>,
    /// `None` when the command could not be run or its status was lost.
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub reproduction: Option<Box<CommandRun>>,
}

impl CommandRun {
    pub fn receipt(&self) -> super::verification::VerificationReceipt {
        super::verification::VerificationReceipt {
            argv: self.argv.clone(),
            exit_code: self.exit_code,
            duration_ms: self.duration_ms,
        }
    }
}

/// Runs argv with an environment read from stdin (`NAME=value` entries separated
/// by NUL) and nothing inherited, so secrets never appear in the exec request.
const ENV_FROM_STDIN: &str = "import os,sys\nenv={}\nfor item in sys.stdin.buffer.read().split(b'\\0'):\n    if item:\n        name,_,value=item.partition(b'=')\n        env[name.decode()]=value.decode()\nos.execvpe(sys.argv[1],sys.argv[1:],env)";

fn with_env_from_stdin(argv: &[String]) -> Vec<String> {
    let mut command = strings(&["python3", "-c", ENV_FROM_STDIN]);
    command.extend(argv.iter().cloned());
    command
}

fn env_stdin(env: &[(String, String)]) -> Vec<u8> {
    let mut data = Vec::new();
    for (name, value) in env {
        data.extend_from_slice(name.as_bytes());
        data.push(b'=');
        data.extend_from_slice(value.as_bytes());
        data.push(0);
    }
    data
}

#[derive(Debug)]
pub struct SandboxResult {
    /// The agent's output; `None` for a commands-only pod.
    pub output: Option<ExecOutput>,
    /// Artifacts from `collect_dir`, names validated.
    pub artifacts: Vec<(String, Vec<u8>)>,
    pub verification: Vec<CommandRun>,
    /// `git diff --binary` against `base_sha`, including new files.
    pub diff: Vec<u8>,
}

/// Wraps a command so the task cannot fork-bomb the node: the kubelet has no pod
/// PID limit (spike S2), and under user namespaces `RLIMIT_NPROC` is per pod.
/// `prlimit` (util-linux) exits non-zero if it cannot set the limit, so the command
/// never runs unlimited; a shell `ulimit -u` would silently fail under dash.
pub fn limited(argv: &[String]) -> Vec<String> {
    let mut command: Vec<String> = ["prlimit", "--nproc=512:512", "--"]
        .iter()
        .map(|part| part.to_string())
        .collect();
    command.extend(argv.iter().cloned());
    command
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

pub async fn run_in_sandbox(
    runtime: std::sync::Arc<dyn SandboxRuntime>,
    job: &SandboxJob,
    on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
) -> anyhow::Result<SandboxResult> {
    // Validated before any pod exists: it is interpolated into the diff command.
    if job.base_sha.len() != 40 || !job.base_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_base_sha")
    }
    // Armed before `create` returns: the name is fixed by the manifest, so a job
    // cancelled mid-create still deletes the pod (a 404 counts as deleted).
    let planned = PodHandle {
        namespace: job.manifest["metadata"]["namespace"]
            .as_str()
            .unwrap_or(super::sandbox::SANDBOX_NAMESPACE)
            .to_string(),
        name: job.manifest["metadata"]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    };
    let mut guard = PodGuard {
        runtime: runtime.clone(),
        pod: (!planned.name.is_empty()).then_some(planned),
    };
    // A retried run first removes the pods of its earlier attempts: a crashed
    // worker's pod may still be running the agent and spending its budget.
    let labels = &job.manifest["metadata"]["labels"];
    if let Some(run_id) = labels[super::sandbox::RUN_LABEL].as_str() {
        delete_run_orphans(
            runtime.as_ref(),
            run_id,
            labels[super::sandbox::SLOT_LABEL].as_str(),
        )
        .await;
    }
    let pod = runtime.create(&job.manifest).await?;
    guard.pod = Some(pod.clone());
    // Dropping the in-flight exec on timeout closes its stream; the delete below
    // then kills whatever still runs in the pod.
    let outcome = match tokio::time::timeout(
        job.wall_time,
        drive(runtime.as_ref(), &pod, job, on_line),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            // Out of time: keep the partial work if anyone is collecting it.
            if let (true, Some(checkpoints)) = (job.collect_diff, &job.checkpoints) {
                if let Ok(Ok(diff)) = tokio::time::timeout(
                    FINAL_DIFF_TIMEOUT,
                    take_diff(runtime.as_ref(), &pod, &job.base_sha),
                )
                .await
                {
                    let _ = checkpoints.sink.send(diff);
                }
            }
            Err(anyhow::anyhow!("sandbox_timeout"))
        }
    };
    // Always delete, whatever happened inside. The guard stays armed until the
    // delete completes, so a cancel during it still deletes. A failed delete is
    // reported only if the task itself succeeded; the GC sweep removes leftovers.
    let deleted = runtime.as_ref().delete(&pod).await;
    guard.pod = None;
    match (outcome, deleted) {
        (Ok(result), Ok(())) => Ok(result),
        (Ok(_), Err(error)) => Err(error.context("sandbox_delete_failed")),
        (Err(error), _) => Err(error),
    }
}

async fn drive(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    job: &SandboxJob,
    on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
) -> anyhow::Result<SandboxResult> {
    runtime.wait_ready(pod, job.ready_timeout).await?;
    let unpack = runtime
        .exec(
            pod,
            &limited(&strings(&[
                "tar",
                "-x",
                "-C",
                super::sandbox::WORKSPACE,
                "-f",
                "-",
            ])),
            Some(job.workspace_tar.clone()),
            &mut |_: &[u8]| {},
        )
        .await?;
    if unpack.exit_code != 0 {
        anyhow::bail!("workspace_unpack_failed")
    }
    for (path, content) in &job.files {
        let written = runtime
            .exec(
                pod,
                &limited(&strings(&["python3", "-c", WRITE_FILE, path])),
                Some(content.clone()),
                &mut |_: &[u8]| {},
            )
            .await?;
        if written.exit_code != 0 {
            anyhow::bail!("sandbox_file_write_failed")
        }
    }
    let output = match &job.command {
        Some(command) => {
            let argv = limited(command);
            let agent = runtime.exec(pod, &argv, job.command_stdin.clone(), on_line);
            let output = match (&job.checkpoints, job.collect_diff) {
                (Some(checkpoints), true) => {
                    with_checkpoints(runtime, pod, &job.base_sha, checkpoints, agent).await?
                }
                _ => agent.await?,
            };
            // No status means the stream broke: the output may be partial and the
            // process may still be editing, so neither transcript nor diff is trusted.
            if output.exit_code < 0 {
                anyhow::bail!("command_status_unknown")
            }
            Some(output)
        }
        None => None,
    };
    // Evidence is best effort: a finished, paid-for agent run is never failed
    // because a screenshot could not be brought back.
    let artifacts = match &job.collect_dir {
        Some(dir) => collect_artifacts(runtime, pod, dir)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(target: "factory_sandbox", pod = %pod.name, "artifact collection failed: {error:#}");
                Vec::new()
            }),
        None => Vec::new(),
    };
    // The diff is taken before any later verification, whose installs and builds
    // may touch tracked files (lockfiles) that are not part of the change.
    let diff = if job.collect_diff {
        take_diff(runtime, pod, &job.base_sha).await?
    } else {
        Vec::new()
    };
    let verification = verify(runtime, pod, job).await;
    Ok(SandboxResult {
        output,
        artifacts,
        verification,
        diff,
    })
}

async fn collect_artifacts(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    dir: &str,
) -> anyhow::Result<Vec<(String, Vec<u8>)>> {
    use base64::Engine;
    let listed = runtime
        .exec(
            pod,
            &limited(&strings(&["python3", "-c", LIST_ARTIFACTS, dir])),
            None,
            &mut |_: &[u8]| {},
        )
        .await?;
    if listed.exit_code != 0 {
        anyhow::bail!("artifact_collection_failed")
    }
    let mut artifacts = Vec::new();
    let mut total = 0usize;
    for line in listed
        .stdout
        .split(|b| *b == b'\n')
        .filter(|line| !line.is_empty())
    {
        // The pod is untrusted: re-check every name and bound what is kept.
        let Some((name, content)) = std::str::from_utf8(line)
            .ok()
            .and_then(|text| text.split_once('\t'))
            .filter(|(name, _)| super::sandbox::valid_artifact_name(name))
            .and_then(|(name, encoded)| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .ok()
                    .map(|content| (name, content))
            })
        else {
            tracing::warn!(target: "factory_sandbox", pod = %pod.name, "skipped an invalid artifact");
            continue;
        };
        if artifacts.len() >= MAX_ARTIFACTS || total + content.len() > MAX_ARTIFACT_BYTES {
            break;
        }
        total += content.len();
        artifacts.push((name.to_string(), content));
    }
    Ok(artifacts)
}

/// Drives the agent while sending a diff of its work every `every`. A failed
/// checkpoint is skipped: checkpoints are a safety net, not part of the result.
async fn with_checkpoints(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    base_sha: &str,
    checkpoints: &Checkpoints,
    agent: impl std::future::Future<Output = anyhow::Result<ExecOutput>>,
) -> anyhow::Result<ExecOutput> {
    tokio::pin!(agent);
    let mut ticker = tokio::time::interval(checkpoints.every);
    ticker.tick().await;
    loop {
        tokio::select! {
            output = &mut agent => return output,
            _ = ticker.tick() => {
                match take_diff(runtime, pod, base_sha).await {
                    Ok(diff) if !diff.is_empty() => {
                        let _ = checkpoints.sink.send(diff);
                    }
                    Ok(_) => {}
                    Err(error) => tracing::warn!(target: "factory_sandbox", pod = %pod.name, "checkpoint failed: {error:#}"),
                }
            }
        }
    }
}

async fn take_diff(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    base_sha: &str,
) -> anyhow::Result<Vec<u8>> {
    // Intent-to-add makes new files appear in the diff without staging content.
    let diff_script = format!(
        "cd {} && git add -A -N . && git diff --binary {}",
        super::sandbox::WORKSPACE,
        base_sha
    );
    let diff = runtime
        .exec(
            pod,
            &limited(&strings(&["sh", "-c", &diff_script])),
            None,
            &mut |_: &[u8]| {},
        )
        .await?;
    if diff.exit_code != 0 {
        anyhow::bail!("diff_failed")
    }
    if diff.stdout.len() > MAX_DIFF_BYTES {
        anyhow::bail!("diff_too_large")
    }
    Ok(diff.stdout)
}

/// Runs each command under a per-command timeout. A failing command or a broken
/// exec is recorded as evidence (the report blocks on it), not a job failure.
async fn verify(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    job: &SandboxJob,
) -> Vec<CommandRun> {
    let mut runs = Vec::new();
    for argv in &job.verification {
        let mut run = run_command(runtime, pod, job, argv).await;
        if job.reproduce_failures && run.exit_code != Some(0) {
            run.reproduction = Some(Box::new(run_command(runtime, pod, job, argv).await));
        }
        runs.push(run);
    }
    runs
}

/// Largest stdout kept per command; stderr is capped by the runtime.
const MAX_COMMAND_STDOUT: usize = 200_000;

/// A failing command or a broken exec is evidence, not a job failure.
async fn run_command(
    runtime: &dyn SandboxRuntime,
    pod: &PodHandle,
    job: &SandboxJob,
    argv: &[String],
) -> CommandRun {
    let mut timed = strings(&[
        "timeout",
        "--kill-after=10",
        &job.command_timeout_secs.to_string(),
    ]);
    timed.extend(argv.iter().cloned());
    let started = std::time::Instant::now();
    let result = runtime
        .exec(
            pod,
            &limited(&with_env_from_stdin(&timed)),
            Some(env_stdin(&job.verification_env)),
            &mut |_: &[u8]| {},
        )
        .await;
    let duration_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(output) => CommandRun {
            argv: argv.to_vec(),
            exit_code: (output.exit_code >= 0).then_some(output.exit_code),
            duration_ms,
            stdout: output.stdout.into_iter().take(MAX_COMMAND_STDOUT).collect(),
            stderr: output.stderr,
            reproduction: None,
        },
        Err(error) => {
            tracing::warn!(target: "factory_sandbox", pod = %pod.name, "command exec failed: {error:#}");
            CommandRun {
                argv: argv.to_vec(),
                exit_code: None,
                duration_ms,
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        fail_ready: bool,
        fail_command: bool,
        hang_command: bool,
        hang_create: bool,
        failing_npm: bool,
        has_orphan: bool,
        bad_artifact: bool,
        slow_command: Option<Duration>,
        lost_status: bool,
        diff: Vec<u8>,
    }

    #[async_trait]
    impl SandboxRuntime for Fake {
        async fn create(&self, _manifest: &Value) -> anyhow::Result<PodHandle> {
            self.calls.lock().unwrap().push("create".into());
            if self.hang_create {
                std::future::pending::<()>().await;
            }
            Ok(PodHandle {
                namespace: "ns".into(),
                name: "task-1".into(),
            })
        }
        async fn wait_ready(&self, _pod: &PodHandle, _timeout: Duration) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push("wait".into());
            if self.fail_ready {
                anyhow::bail!("pod_not_ready")
            }
            Ok(())
        }
        async fn exec(
            &self,
            _pod: &PodHandle,
            argv: &[String],
            stdin: Option<Vec<u8>>,
            on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
        ) -> anyhow::Result<ExecOutput> {
            let joined = argv.join(" ");
            self.calls.lock().unwrap().push(format!(
                "exec[{}]{}",
                stdin.map(|s| s.len()).unwrap_or(0),
                joined
            ));
            if joined.contains("open(sys.argv[1],'wb')") {
                return Ok(ExecOutput::default());
            }
            if joined.contains("base64.b64encode") {
                let stdout = if self.bad_artifact {
                    b"../evil.png\taGk=\n".to_vec()
                } else {
                    b"shot-1.png\taGk=\n".to_vec()
                };
                return Ok(ExecOutput {
                    exit_code: 0,
                    stdout,
                    stderr: vec![],
                });
            }
            if joined.contains(" npm ") {
                return Ok(ExecOutput {
                    exit_code: if self.failing_npm { 1 } else { 0 },
                    ..Default::default()
                });
            }
            if joined.contains(" cargo ") {
                anyhow::bail!("exec_stream_broken")
            }
            if joined.contains("git diff") {
                return Ok(ExecOutput {
                    exit_code: 0,
                    stdout: self.diff.clone(),
                    stderr: vec![],
                });
            }
            if joined.contains("claude") {
                if self.fail_command {
                    anyhow::bail!("exec_failed")
                }
                if self.hang_command {
                    std::future::pending::<()>().await;
                }
                if let Some(delay) = self.slow_command {
                    tokio::time::sleep(delay).await;
                }
                if self.lost_status {
                    return Ok(ExecOutput {
                        exit_code: -1,
                        ..Default::default()
                    });
                }
                on_line(b"{\"type\":\"result\"}");
                return Ok(ExecOutput {
                    exit_code: 0,
                    stdout: b"{\"type\":\"result\"}\n".to_vec(),
                    stderr: vec![],
                });
            }
            Ok(ExecOutput::default())
        }
        async fn delete(&self, pod: &PodHandle) -> anyhow::Result<()> {
            let entry = if pod.name == "task-1" {
                "delete".to_string()
            } else {
                format!("delete {}", pod.name)
            };
            self.calls.lock().unwrap().push(entry);
            Ok(())
        }
        async fn list_task_pods(&self) -> anyhow::Result<Vec<TaskPodInfo>> {
            let pod = |name: &str, created_unix: i64| TaskPodInfo {
                handle: PodHandle {
                    namespace: "ns".into(),
                    name: name.into(),
                },
                created_unix,
                deadline_secs: Some(600),
                run_id: Some("run-other".into()),
                slot: Some("main".into()),
            };
            let mut pods = vec![
                // Deadline + grace (600 + 300) long past.
                pod("task-old", 1_000),
                // Still inside its deadline.
                pod("task-live", 10_000),
                TaskPodInfo {
                    deadline_secs: None,
                    ..pod("task-no-deadline", 1_000)
                },
            ];
            if self.has_orphan {
                // An orphan of an earlier attempt of the run under test.
                pods.push(TaskPodInfo {
                    run_id: Some("run-A".into()),
                    ..pod("task-orphan", 10_000)
                });
                // A sibling of the same run in another slot (resolver fanout).
                pods.push(TaskPodInfo {
                    run_id: Some("run-A".into()),
                    slot: Some("issue-7".into()),
                    ..pod("task-sibling", 10_000)
                });
            }
            Ok(pods)
        }
    }

    fn job(base_sha: &str) -> SandboxJob {
        SandboxJob {
            manifest: serde_json::json!({"metadata": {
                "name": "task-1",
                "namespace": "ns",
                "labels": {"factory.nexusmind/run": "run-A", "factory.nexusmind/slot": "main"}
            }}),
            workspace_tar: vec![1, 2, 3],
            command: Some(vec!["claude".into(), "-p".into()]),
            command_stdin: Some(b"the prompt".to_vec()),
            base_sha: base_sha.into(),
            ready_timeout: Duration::from_secs(5),
            wall_time: Duration::from_secs(5),
            collect_diff: true,
            verification: Vec::new(),
            verification_env: Vec::new(),
            command_timeout_secs: 300,
            reproduce_failures: false,
            files: Vec::new(),
            collect_dir: None,
            checkpoints: None,
        }
    }

    #[tokio::test]
    async fn checkpoints_stream_diffs_while_the_agent_works() {
        let fake = Arc::new(Fake {
            slow_command: Some(Duration::from_millis(120)),
            diff: b"diff --git a/x b/x".to_vec(),
            ..Default::default()
        });
        let (sink, mut received) = tokio::sync::mpsc::unbounded_channel();
        let mut job = job(SHA);
        job.checkpoints = Some(Checkpoints {
            every: Duration::from_millis(30),
            sink,
        });
        let result = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap();
        assert_eq!(result.diff, b"diff --git a/x b/x");
        let mut checkpoints = 0;
        while let Ok(diff) = received.try_recv() {
            assert_eq!(diff, b"diff --git a/x b/x");
            checkpoints += 1;
        }
        assert!(checkpoints >= 2, "{checkpoints}");
    }

    #[tokio::test]
    async fn a_timed_out_job_hands_over_its_last_diff_before_the_pod_goes() {
        let fake = Arc::new(Fake {
            hang_command: true,
            diff: b"diff --git a/partial b/partial".to_vec(),
            ..Default::default()
        });
        let (sink, mut received) = tokio::sync::mpsc::unbounded_channel();
        let mut job = job(SHA);
        job.wall_time = Duration::from_millis(50);
        job.checkpoints = Some(Checkpoints {
            every: Duration::from_secs(3600),
            sink,
        });
        let error = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "sandbox_timeout");
        assert_eq!(
            received.try_recv().unwrap(),
            b"diff --git a/partial b/partial"
        );
        let calls = fake.calls.lock().unwrap().clone();
        let diff = calls.iter().rposition(|c| c.contains("git diff")).unwrap();
        assert!(
            diff < calls.iter().rposition(|c| c == "delete").unwrap(),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn files_go_in_on_stdin_and_artifacts_come_back_validated() {
        let fake = Arc::new(Fake::default());
        let mut job = job(SHA);
        job.files = vec![("/tmp/cfg.json".into(), b"{\"secret\":1}".to_vec())];
        job.collect_dir = Some("/tmp/out".into());
        let result = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap();
        assert_eq!(
            result.artifacts,
            [("shot-1.png".to_string(), b"hi".to_vec())]
        );
        let calls = fake.calls.lock().unwrap().clone();
        let write = calls.iter().find(|c| c.contains("/tmp/cfg.json")).unwrap();
        assert!(write.starts_with("exec[12]"), "{write}");
        assert!(!write.contains("secret"), "{write}");
        let position = |needle: &str| calls.iter().position(|c| c.contains(needle)).unwrap();
        assert!(position("tar -x") < position("/tmp/cfg.json"), "{calls:?}");
        assert!(position("/tmp/cfg.json") < position("claude"), "{calls:?}");
        assert!(
            position("claude") < position("base64.b64encode"),
            "{calls:?}"
        );

        let hostile = Arc::new(Fake {
            bad_artifact: true,
            ..Default::default()
        });
        // A hostile name is dropped; the finished run still succeeds.
        let result = run_in_sandbox(hostile.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap();
        assert!(result.artifacts.is_empty());
        assert!(result.output.is_some());
    }

    fn with_checks(mut job: SandboxJob) -> SandboxJob {
        job.verification = vec![strings(&["npm", "test"]), strings(&["cargo", "test"])];
        job.verification_env = vec![("HTTPS_PROXY".into(), "http://run:registry@proxy".into())];
        job
    }

    #[tokio::test]
    async fn verification_runs_after_the_agent_and_diff_in_a_clean_environment() {
        let fake = Arc::new(Fake {
            failing_npm: true,
            ..Default::default()
        });
        let result = run_in_sandbox(fake.clone(), &with_checks(job(SHA)), &mut |_: &[u8]| {})
            .await
            .unwrap();
        let exit_codes: Vec<Option<i32>> =
            result.verification.iter().map(|r| r.exit_code).collect();
        // A failing command and a broken exec are evidence, not a job failure.
        assert_eq!(exit_codes, [Some(1), None]);
        assert_eq!(result.verification[0].argv, ["npm", "test"]);
        let calls = fake.calls.lock().unwrap();
        let position = |needle: &str| calls.iter().position(|c| c.contains(needle)).unwrap();
        assert!(position("claude") < position("git diff"), "{calls:?}");
        assert!(position("git diff") < position(" npm "), "{calls:?}");
        let npm = &calls[position(" npm ")];
        // The environment travels on stdin: nothing of it is in the exec request.
        assert!(
            npm.starts_with("exec[38]prlimit --nproc=512:512 -- python3 -c"),
            "{npm}"
        );
        assert!(
            npm.ends_with("timeout --kill-after=10 300 npm test"),
            "{npm}"
        );
        assert!(!npm.contains("HTTPS_PROXY"), "{npm}");
        // The prompt too.
        assert!(
            calls[position("claude")].starts_with("exec[10]"),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn a_commands_only_pod_runs_no_agent_and_can_reproduce_failures() {
        let fake = Arc::new(Fake {
            failing_npm: true,
            ..Default::default()
        });
        let mut job = with_checks(job(SHA));
        job.command = None;
        job.reproduce_failures = true;
        job.verification.truncate(1);
        let result = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap();
        assert!(result.output.is_none());
        let run = &result.verification[0];
        assert_eq!(run.exit_code, Some(1));
        assert_eq!(run.reproduction.as_ref().unwrap().exit_code, Some(1));
        let calls = fake.calls.lock().unwrap();
        assert!(!calls.iter().any(|c| c.contains("claude")), "{calls:?}");
        assert_eq!(calls.iter().filter(|c| c.contains(" npm ")).count(), 2);
    }

    #[tokio::test]
    async fn a_job_cancelled_while_creating_still_deletes_its_pod() {
        let fake = Arc::new(Fake {
            hang_create: true,
            ..Default::default()
        });
        let job = job(SHA);
        let dropped = tokio::time::timeout(
            Duration::from_millis(50),
            run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {}),
        )
        .await;
        assert!(dropped.is_err());
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn read_only_jobs_skip_the_diff() {
        let fake = Arc::new(Fake::default());
        let mut job = job(SHA);
        job.collect_diff = false;
        let result = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap();
        assert!(result.diff.is_empty());
        let calls = fake.calls.lock().unwrap();
        assert!(!calls.iter().any(|c| c.contains("git diff")), "{calls:?}");
        assert_eq!(calls.last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn a_cancelled_job_still_deletes_its_pod() {
        let fake = Arc::new(Fake {
            hang_command: true,
            ..Default::default()
        });
        let job = job(SHA);
        let dropped = tokio::time::timeout(
            Duration::from_millis(50),
            run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {}),
        )
        .await;
        assert!(dropped.is_err(), "the caller gave up first");
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn gc_deletes_only_pods_past_deadline_and_grace() {
        let fake = Arc::new(Fake::default());
        let deleted = gc_expired_task_pods(fake.as_ref(), 10_500).await.unwrap();
        assert_eq!(deleted, 2);
        let calls = fake.calls.lock().unwrap();
        assert!(calls.contains(&"delete task-old".to_string()), "{calls:?}");
        // No deadline: fall back to the maximum wall time, so it is expired too.
        assert!(
            calls.contains(&"delete task-no-deadline".to_string()),
            "{calls:?}"
        );
        assert!(!calls.iter().any(|c| c.contains("task-live")), "{calls:?}");
    }

    #[tokio::test]
    async fn orphans_of_earlier_attempts_are_deleted_before_the_new_pod() {
        let fake = Arc::new(Fake {
            has_orphan: true,
            ..Default::default()
        });
        run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .unwrap();
        let calls = fake.calls.lock().unwrap();
        let orphan = calls
            .iter()
            .position(|c| c == "delete task-orphan")
            .expect("orphan deleted");
        let create = calls.iter().position(|c| c == "create").unwrap();
        assert!(orphan < create, "{calls:?}");
        // Pods of other runs are the GC's business, not this run's, and parallel
        // pods of this run in other slots are siblings, not orphans.
        assert!(!calls.iter().any(|c| c.contains("task-live")), "{calls:?}");
        assert!(
            !calls.iter().any(|c| c.contains("task-sibling")),
            "{calls:?}"
        );
    }

    #[tokio::test]
    async fn a_hung_task_times_out_and_the_pod_is_still_deleted() {
        let fake = Arc::new(Fake {
            hang_command: true,
            ..Default::default()
        });
        let mut job = job(SHA);
        job.wall_time = Duration::from_millis(50);
        let error = run_in_sandbox(fake.clone(), &job, &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "sandbox_timeout");
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn a_command_without_exit_status_is_an_error_not_a_result() {
        let fake = Arc::new(Fake {
            lost_status: true,
            ..Default::default()
        });
        let error = run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "command_status_unknown");
        let calls = fake.calls.lock().unwrap();
        assert!(!calls.iter().any(|c| c.contains("git diff")), "{calls:?}");
        assert_eq!(calls.last().unwrap(), "delete");
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    #[tokio::test]
    async fn the_happy_path_streams_output_returns_the_diff_and_deletes_the_pod() {
        let fake = Arc::new(Fake {
            diff: b"diff --git a/x b/x".to_vec(),
            ..Default::default()
        });
        let mut lines = Vec::new();
        let result = run_in_sandbox(fake.clone(), &job(SHA), &mut |line: &[u8]| {
            lines.push(line.to_vec())
        })
        .await
        .unwrap();
        assert_eq!(result.diff, b"diff --git a/x b/x");
        assert_eq!(lines, vec![b"{\"type\":\"result\"}".to_vec()]);
        let calls = fake.calls.lock().unwrap().clone();
        assert_eq!(calls.first().unwrap(), "create");
        assert_eq!(calls[1], "wait");
        assert!(
            calls[2].starts_with("exec[3]") && calls[2].contains("tar"),
            "{}",
            calls[2]
        );
        assert!(calls[3].contains("claude"));
        assert!(calls[4].contains("git diff --binary") && calls[4].contains(SHA));
        assert_eq!(calls.last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn every_command_runs_under_the_process_limit() {
        let fake = Arc::new(Fake::default());
        run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .unwrap();
        for call in fake
            .calls
            .lock()
            .unwrap()
            .iter()
            .filter(|c| c.starts_with("exec"))
        {
            assert!(call.contains("prlimit --nproc=512:512 --"), "{call}");
        }
        // No shell: prlimit fails closed if it cannot set the limit (dash has no `ulimit -u`).
        assert_eq!(
            limited(&["echo".into(), "a b".into()]),
            ["prlimit", "--nproc=512:512", "--", "echo", "a b"]
        );
    }

    #[tokio::test]
    async fn the_pod_is_deleted_when_it_never_becomes_ready() {
        let fake = Arc::new(Fake {
            fail_ready: true,
            ..Default::default()
        });
        assert!(run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .is_err());
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec!["create", "wait", "delete"]
        );
    }

    #[tokio::test]
    async fn the_pod_is_deleted_when_the_task_fails() {
        let fake = Arc::new(Fake {
            fail_command: true,
            ..Default::default()
        });
        assert!(run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .is_err());
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn an_oversized_diff_is_refused_and_the_pod_still_deleted() {
        let fake = Arc::new(Fake {
            diff: vec![b'x'; MAX_DIFF_BYTES + 1],
            ..Default::default()
        });
        let error = run_in_sandbox(fake.clone(), &job(SHA), &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "diff_too_large");
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn an_invalid_base_sha_is_refused_before_any_pod_exists() {
        let fake = Arc::new(Fake::default());
        for bad in ["main", "0123; rm -rf /", ""] {
            assert!(
                run_in_sandbox(fake.clone(), &job(bad), &mut |_: &[u8]| {})
                    .await
                    .is_err(),
                "{bad}"
            );
        }
        assert!(fake.calls.lock().unwrap().is_empty());
    }
}

/// Reads `reader` to the end, calling `on_line` for every line as it arrives.
/// Fails (never truncates) when a single line exceeds `max_line` or the whole
/// output exceeds `max_total`, so untrusted output cannot exhaust the worker.
pub async fn read_lines_bounded<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
    max_total: usize,
    max_line: usize,
) -> anyhow::Result<Vec<u8>> {
    use tokio::io::AsyncBufReadExt;
    let mut reader = tokio::io::BufReader::new(reader);
    let mut output = Vec::new();
    let mut line_start = 0;
    // Where the newline search resumes: bytes before it are known newline-free.
    let mut scanned = 0;
    loop {
        let chunk = reader.fill_buf().await?;
        if chunk.is_empty() {
            break;
        }
        let taken = chunk.len();
        if output.len() + taken > max_total {
            anyhow::bail!("output_too_large");
        }
        output.extend_from_slice(chunk);
        reader.consume(taken);
        while let Some(offset) = output[scanned..].iter().position(|&b| b == b'\n') {
            let end = scanned + offset;
            if end - line_start > max_line {
                anyhow::bail!("output_line_too_large");
            }
            on_line(&output[line_start..end]);
            line_start = end + 1;
            scanned = line_start;
        }
        scanned = output.len();
        if output.len() - line_start > max_line {
            anyhow::bail!("output_line_too_large");
        }
    }
    if line_start < output.len() {
        on_line(&output[line_start..]);
    }
    Ok(output)
}

/// Reads `reader` to EOF keeping at most `cap` bytes. Dropping the reader early
/// would make kube abort the whole exec stream and cut stdout short.
pub async fn drain_capped<R: tokio::io::AsyncRead + Unpin>(reader: R, cap: usize) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let mut reader = reader;
    let mut kept = Vec::new();
    let mut buffer = [0u8; 8192];
    while let Ok(read) = reader.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        let room = cap.saturating_sub(kept.len());
        kept.extend_from_slice(&buffer[..read.min(room)]);
    }
    kept
}

/// Exit code of a finished `exec` from its Kubernetes `Status`: `Success` is 0;
/// a failure carries the code as a cause with reason `ExitCode`. Anything else
/// (a stream that ended without a status) is reported as -1, never as success.
pub fn exit_code_from_status(
    status: Option<&k8s_openapi::apimachinery::pkg::apis::meta::v1::Status>,
) -> i32 {
    let Some(status) = status else {
        return -1;
    };
    if status.status.as_deref() == Some("Success") {
        return 0;
    }
    status
        .details
        .as_ref()
        .and_then(|details| details.causes.as_ref())
        .and_then(|causes| {
            causes
                .iter()
                .find(|cause| cause.reason.as_deref() == Some("ExitCode"))
        })
        .and_then(|cause| cause.message.as_deref())
        .and_then(|code| code.parse().ok())
        .unwrap_or(-1)
}

/// In-cluster runtime: task pods in the sandbox namespace, driven through the
/// Kubernetes API with the worker's ServiceAccount (pods, pods/exec only).
pub struct KubeRuntime {
    pods: kube::Api<k8s_openapi::api::core::v1::Pod>,
}

impl KubeRuntime {
    pub async fn in_cluster() -> anyhow::Result<Self> {
        let client = kube::Client::try_default().await?;
        Ok(Self {
            pods: kube::Api::namespaced(client, super::sandbox::SANDBOX_NAMESPACE),
        })
    }
}

#[async_trait]
impl SandboxRuntime for KubeRuntime {
    async fn create(&self, manifest: &Value) -> anyhow::Result<PodHandle> {
        let pod: k8s_openapi::api::core::v1::Pod = serde_json::from_value(manifest.clone())?;
        let created = self
            .pods
            .create(&kube::api::PostParams::default(), &pod)
            .await?;
        Ok(PodHandle {
            namespace: super::sandbox::SANDBOX_NAMESPACE.to_string(),
            name: created
                .metadata
                .name
                .ok_or_else(|| anyhow::anyhow!("pod_without_name"))?,
        })
    }

    async fn wait_ready(&self, pod: &PodHandle, timeout: Duration) -> anyhow::Result<()> {
        use kube::runtime::wait::{await_condition, conditions::is_pod_running};
        tokio::time::timeout(
            timeout,
            await_condition(self.pods.clone(), &pod.name, is_pod_running()),
        )
        .await
        .map_err(|_| anyhow::anyhow!("pod_not_ready"))??;
        Ok(())
    }

    async fn exec(
        &self,
        pod: &PodHandle,
        argv: &[String],
        stdin: Option<Vec<u8>>,
        on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
    ) -> anyhow::Result<ExecOutput> {
        use tokio::io::AsyncWriteExt;
        const MAX_STDOUT: usize = 32 * 1024 * 1024;
        // Stream-json events are single lines; a diff line can be a long binary patch.
        const MAX_LINE: usize = 8 * 1024 * 1024;
        let params = kube::api::AttachParams::default()
            .container("task")
            .stdin(stdin.is_some())
            .stdout(true)
            .stderr(true);
        let mut attached = self.pods.exec(&pod.name, argv.to_vec(), &params).await?;
        // kube multiplexes all streams through one loop with 1 KiB buffers: stdout
        // and stderr must be read while stdin is written, or a chatty command
        // blocks that loop and the stdin write never finishes.
        let status = attached.take_status();
        let stderr_reader = attached
            .stderr()
            .ok_or_else(|| anyhow::anyhow!("exec_without_stderr"))?;
        let stderr_task = tokio::spawn(drain_capped(stderr_reader, 1024 * 1024));
        let stdin_task = match (stdin, attached.stdin()) {
            (Some(data), Some(mut writer)) => Some(tokio::spawn(async move {
                writer.write_all(&data).await?;
                writer.shutdown().await
            })),
            (Some(_), None) => anyhow::bail!("exec_without_stdin"),
            _ => None,
        };
        let stdout_reader = attached
            .stdout()
            .ok_or_else(|| anyhow::anyhow!("exec_without_stdout"))?;
        // Fails closed on oversized output: a truncated transcript or diff must never
        // pass for a complete one.
        let stdout = read_lines_bounded(stdout_reader, on_line, MAX_STDOUT, MAX_LINE).await?;
        if let Some(task) = stdin_task {
            task.await??;
        }
        let status = match status {
            Some(status) => status.await,
            None => None,
        };
        let stderr = stderr_task.await.unwrap_or_default();
        attached.join().await.ok();
        Ok(ExecOutput {
            exit_code: exit_code_from_status(status.as_ref()),
            stdout,
            stderr,
        })
    }

    async fn list_task_pods(&self) -> anyhow::Result<Vec<TaskPodInfo>> {
        let params = kube::api::ListParams::default().labels("role=task");
        let pods = self.pods.list(&params).await?;
        Ok(pods
            .items
            .into_iter()
            .filter_map(|pod| {
                let name = pod.metadata.name?;
                let created_unix = pod.metadata.creation_timestamp?.0.as_second();
                Some(TaskPodInfo {
                    handle: PodHandle {
                        namespace: super::sandbox::SANDBOX_NAMESPACE.to_string(),
                        name,
                    },
                    created_unix,
                    deadline_secs: pod.spec.and_then(|spec| spec.active_deadline_seconds),
                    run_id: pod
                        .metadata
                        .labels
                        .as_ref()
                        .and_then(|labels| labels.get(super::sandbox::RUN_LABEL).cloned()),
                    slot: pod
                        .metadata
                        .labels
                        .as_ref()
                        .and_then(|labels| labels.get(super::sandbox::SLOT_LABEL).cloned()),
                })
            })
            .collect())
    }

    async fn delete(&self, pod: &PodHandle) -> anyhow::Result<()> {
        let params = kube::api::DeleteParams {
            grace_period_seconds: Some(0),
            ..Default::default()
        };
        match self.pods.delete(&pod.name, &params).await {
            Ok(_) => Ok(()),
            // Already gone (e.g. its deadline killed it): the goal is met.
            Err(kube::Error::Api(response)) if response.code == 404 => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

#[cfg(test)]
mod reader_tests {
    use super::*;

    async fn read(
        input: &[u8],
        max_total: usize,
        max_line: usize,
    ) -> (anyhow::Result<Vec<u8>>, Vec<Vec<u8>>) {
        let mut lines = Vec::new();
        let result = read_lines_bounded(
            input,
            &mut |line: &[u8]| lines.push(line.to_vec()),
            max_total,
            max_line,
        )
        .await;
        (result, lines)
    }

    #[tokio::test]
    async fn lines_stream_and_the_output_is_kept_whole() {
        let (result, lines) = read(b"a\nbb\nlast-without-newline", 1024, 64).await;
        assert_eq!(result.unwrap(), b"a\nbb\nlast-without-newline");
        assert_eq!(
            lines,
            vec![
                b"a".to_vec(),
                b"bb".to_vec(),
                b"last-without-newline".to_vec()
            ]
        );
    }

    #[tokio::test]
    async fn an_endless_line_fails_instead_of_growing_forever() {
        let endless = vec![b'y'; 10_000];
        let (result, _) = read(&endless, 1_000_000, 4096).await;
        assert_eq!(result.unwrap_err().to_string(), "output_line_too_large");
    }

    #[tokio::test]
    async fn stderr_is_drained_to_the_end_but_only_the_cap_is_kept() {
        let mut noisy: &[u8] = &[b'e'; 10_000];
        let kept = drain_capped(&mut noisy, 100).await;
        assert_eq!(kept.len(), 100);
        assert!(noisy.is_empty(), "the stream must be read to EOF");
    }

    #[tokio::test]
    async fn too_much_output_fails_instead_of_being_truncated() {
        let many = b"line\n".repeat(1000);
        let (result, _) = read(&many, 1000, 64).await;
        assert_eq!(result.unwrap_err().to_string(), "output_too_large");
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;
    use k8s_openapi::apimachinery::pkg::apis::meta::v1::{Status, StatusCause, StatusDetails};

    fn failure(code: &str) -> Status {
        Status {
            status: Some("Failure".into()),
            details: Some(StatusDetails {
                causes: Some(vec![StatusCause {
                    reason: Some("ExitCode".into()),
                    message: Some(code.into()),
                    ..Default::default()
                }]),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn exit_codes_come_from_the_status() {
        let success = Status {
            status: Some("Success".into()),
            ..Default::default()
        };
        assert_eq!(exit_code_from_status(Some(&success)), 0);
        assert_eq!(exit_code_from_status(Some(&failure("2"))), 2);
        assert_eq!(exit_code_from_status(Some(&failure("not-a-number"))), -1);
        assert_eq!(exit_code_from_status(None), -1);
        let unexplained = Status {
            status: Some("Failure".into()),
            ..Default::default()
        };
        assert_eq!(exit_code_from_status(Some(&unexplained)), -1);
    }
}
