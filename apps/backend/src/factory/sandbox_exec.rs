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
}

/// What one sandboxed task needs.
pub struct SandboxJob {
    pub manifest: Value,
    /// `tar` of the checkout, without credentials (see design §4).
    pub workspace_tar: Vec<u8>,
    pub command: Vec<String>,
    /// Commit the checkout is at; the diff is taken against it.
    pub base_sha: String,
    pub ready_timeout: Duration,
    /// Upper bound for the whole job (unpack, command, diff) before the pod is deleted.
    pub wall_time: Duration,
}

#[derive(Debug)]
pub struct SandboxResult {
    pub output: ExecOutput,
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
    runtime: &dyn SandboxRuntime,
    job: &SandboxJob,
    on_line: &mut (dyn for<'l> FnMut(&'l [u8]) + Send),
) -> anyhow::Result<SandboxResult> {
    // Validated before any pod exists: it is interpolated into the diff command.
    if job.base_sha.len() != 40 || !job.base_sha.bytes().all(|b| b.is_ascii_hexdigit()) {
        anyhow::bail!("invalid_base_sha")
    }
    let pod = runtime.create(&job.manifest).await?;
    // Dropping the in-flight exec on timeout closes its stream; the delete below
    // then kills whatever still runs in the pod.
    let outcome = tokio::time::timeout(job.wall_time, drive(runtime, &pod, job, on_line))
        .await
        .unwrap_or_else(|_| Err(anyhow::anyhow!("sandbox_timeout")));
    // Always delete, whatever happened inside. A failed delete is reported only if
    // the task itself succeeded; the GC sweep removes any pod left behind.
    let deleted = runtime.delete(&pod).await;
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
    let output = runtime
        .exec(pod, &limited(&job.command), None, on_line)
        .await?;
    // No status means the stream broke: the output may be partial and the process
    // may still be editing, so neither the transcript nor a diff can be trusted.
    if output.exit_code < 0 {
        anyhow::bail!("command_status_unknown")
    }
    // Intent-to-add makes new files appear in the diff without staging content.
    let diff_script = format!(
        "cd {} && git add -A -N . && git diff --binary {}",
        super::sandbox::WORKSPACE,
        job.base_sha
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
    Ok(SandboxResult {
        output,
        diff: diff.stdout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        calls: Mutex<Vec<String>>,
        fail_ready: bool,
        fail_command: bool,
        hang_command: bool,
        lost_status: bool,
        diff: Vec<u8>,
    }

    #[async_trait]
    impl SandboxRuntime for Fake {
        async fn create(&self, _manifest: &Value) -> anyhow::Result<PodHandle> {
            self.calls.lock().unwrap().push("create".into());
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
        async fn delete(&self, _pod: &PodHandle) -> anyhow::Result<()> {
            self.calls.lock().unwrap().push("delete".into());
            Ok(())
        }
    }

    fn job(base_sha: &str) -> SandboxJob {
        SandboxJob {
            manifest: serde_json::json!({}),
            workspace_tar: vec![1, 2, 3],
            command: vec!["claude".into(), "-p".into()],
            base_sha: base_sha.into(),
            ready_timeout: Duration::from_secs(5),
            wall_time: Duration::from_secs(5),
        }
    }

    #[tokio::test]
    async fn a_hung_task_times_out_and_the_pod_is_still_deleted() {
        let fake = Fake {
            hang_command: true,
            ..Default::default()
        };
        let mut job = job(SHA);
        job.wall_time = Duration::from_millis(50);
        let error = run_in_sandbox(&fake, &job, &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "sandbox_timeout");
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn a_command_without_exit_status_is_an_error_not_a_result() {
        let fake = Fake {
            lost_status: true,
            ..Default::default()
        };
        let error = run_in_sandbox(&fake, &job(SHA), &mut |_: &[u8]| {})
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
        let fake = Fake {
            diff: b"diff --git a/x b/x".to_vec(),
            ..Default::default()
        };
        let mut lines = Vec::new();
        let result = run_in_sandbox(&fake, &job(SHA), &mut |line: &[u8]| {
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
        let fake = Fake::default();
        run_in_sandbox(&fake, &job(SHA), &mut |_: &[u8]| {})
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
        let fake = Fake {
            fail_ready: true,
            ..Default::default()
        };
        assert!(run_in_sandbox(&fake, &job(SHA), &mut |_: &[u8]| {})
            .await
            .is_err());
        assert_eq!(
            *fake.calls.lock().unwrap(),
            vec!["create", "wait", "delete"]
        );
    }

    #[tokio::test]
    async fn the_pod_is_deleted_when_the_task_fails() {
        let fake = Fake {
            fail_command: true,
            ..Default::default()
        };
        assert!(run_in_sandbox(&fake, &job(SHA), &mut |_: &[u8]| {})
            .await
            .is_err());
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn an_oversized_diff_is_refused_and_the_pod_still_deleted() {
        let fake = Fake {
            diff: vec![b'x'; MAX_DIFF_BYTES + 1],
            ..Default::default()
        };
        let error = run_in_sandbox(&fake, &job(SHA), &mut |_: &[u8]| {})
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "diff_too_large");
        assert_eq!(fake.calls.lock().unwrap().last().unwrap(), "delete");
    }

    #[tokio::test]
    async fn an_invalid_base_sha_is_refused_before_any_pod_exists() {
        let fake = Fake::default();
        for bad in ["main", "0123; rm -rf /", ""] {
            assert!(
                run_in_sandbox(&fake, &job(bad), &mut |_: &[u8]| {})
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
