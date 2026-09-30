//! Moving a checkout into and out of a task pod (factory F1, design §4).
//!
//! In: a `tar` of the worker's checkout, without `.git/hooks`. The clone
//! credential travels in environment variables (`authenticated_git`), so it is
//! never on disk; as a second line, the archive is refused if any known secret
//! value appears in it.
//!
//! Out: the pod's `git diff --binary`, applied to the worker's checkout with
//! `git apply --index`, after which the usual secret scan and gates run.

use std::path::Path;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

/// Largest workspace sent into a pod; the archive is held in memory.
pub const MAX_WORKSPACE_BYTES: usize = 256 * 1024 * 1024;
/// Secret values shorter than this are too likely to match by accident.
const MIN_SECRET_LEN: usize = 8;

pub async fn pack_workspace(workdir: &Path, secrets: &[String]) -> anyhow::Result<Vec<u8>> {
    pack_workspace_limited(workdir, secrets, MAX_WORKSPACE_BYTES).await
}

async fn pack_workspace_limited(
    workdir: &Path,
    secrets: &[String],
    max_bytes: usize,
) -> anyhow::Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut child = Command::new("tar")
        .current_dir(workdir)
        // Hooks would run inside the pod on git commands; they are never needed.
        .args(["-c", "-f", "-", "--exclude=.git/hooks", "."])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("tar_without_stdout"))?;
    let mut archive = Vec::new();
    stdout
        .take(max_bytes as u64 + 1)
        .read_to_end(&mut archive)
        .await?;
    if archive.len() > max_bytes {
        anyhow::bail!("workspace_too_large")
    }
    if !child.wait().await?.success() {
        anyhow::bail!("workspace_pack_failed")
    }
    let leaked = secrets
        .iter()
        .filter(|secret| secret.len() >= MIN_SECRET_LEN)
        .any(|secret| memchr::memmem::find(&archive, secret.as_bytes()).is_some());
    if leaked {
        anyhow::bail!("workspace_contains_secret")
    }
    Ok(archive)
}

/// Applies a diff produced in the pod to `workdir`. An empty diff is a no-op.
pub async fn apply_sandbox_diff(workdir: &Path, diff: &[u8]) -> anyhow::Result<()> {
    if diff.is_empty() {
        return Ok(());
    }
    let mut child = Command::new("git")
        .current_dir(workdir)
        .args(["apply", "--index", "--binary", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("git_apply_without_stdin"))?;
    // A rejected patch may close stdin early; the exit status decides.
    let _ = stdin.write_all(diff).await;
    drop(stdin);
    if !child.wait().await?.success() {
        anyhow::bail!("sandbox_diff_apply_failed")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(dir)
            .args(["-c", "user.email=t@t", "-c", "user.name=t"])
            .args(args)
            .status()
            .await
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    async fn repo() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        git(dir.path(), &["add", "."]).await;
        git(dir.path(), &["commit", "-q", "-m", "init"]).await;
        std::fs::write(dir.path().join(".git/hooks/post-checkout"), "#!/bin/sh\n").unwrap();
        dir
    }

    async fn list(archive: &[u8]) -> Vec<String> {
        let mut child = Command::new("tar")
            .args(["-t", "-f", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(archive)
            .await
            .unwrap();
        let output = child.wait_with_output().await.unwrap();
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| line.trim_start_matches("./").to_string())
            .collect()
    }

    #[tokio::test]
    async fn the_archive_carries_the_checkout_and_git_objects_but_no_hooks() {
        let dir = repo().await;
        let archive = pack_workspace(dir.path(), &[]).await.unwrap();
        let entries = list(&archive).await;
        assert!(entries.iter().any(|e| e == "a.txt"), "{entries:?}");
        assert!(
            entries.iter().any(|e| e.starts_with(".git/objects")),
            "{entries:?}"
        );
        assert!(
            !entries.iter().any(|e| e.contains(".git/hooks")),
            "{entries:?}"
        );
    }

    #[tokio::test]
    async fn an_archive_containing_a_secret_is_refused() {
        let dir = repo().await;
        std::fs::write(
            dir.path().join("leak.txt"),
            "token=ghs_supersecretvalue123\n",
        )
        .unwrap();
        let error = pack_workspace(dir.path(), &["ghs_supersecretvalue123".into(), "".into()])
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "workspace_contains_secret");
        // Short values are ignored rather than matching everything.
        pack_workspace(dir.path(), &["abc".into()]).await.unwrap();
    }

    #[tokio::test]
    async fn an_oversized_workspace_is_refused() {
        let dir = repo().await;
        std::fs::write(dir.path().join("big.bin"), vec![7u8; 64 * 1024]).unwrap();
        let error = pack_workspace_limited(dir.path(), &[], 16 * 1024)
            .await
            .unwrap_err();
        assert_eq!(error.to_string(), "workspace_too_large");
    }

    #[tokio::test]
    async fn a_pod_diff_applies_to_the_worker_checkout() {
        let dir = repo().await;
        let pod = repo().await;
        std::fs::write(pod.path().join("a.txt"), "two\n").unwrap();
        std::fs::write(pod.path().join("new.bin"), [0u8, 159, 146, 150]).unwrap();
        git(pod.path(), &["add", "-A", "-N", "."]).await;
        let diff = Command::new("git")
            .current_dir(pod.path())
            .args(["diff", "--binary", "HEAD"])
            .output()
            .await
            .unwrap()
            .stdout;

        apply_sandbox_diff(dir.path(), &diff).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "two\n"
        );
        assert_eq!(
            std::fs::read(dir.path().join("new.bin")).unwrap(),
            [0u8, 159, 146, 150]
        );
        apply_sandbox_diff(dir.path(), b"").await.unwrap();
    }

    #[tokio::test]
    async fn a_diff_that_does_not_apply_is_an_error() {
        let dir = repo().await;
        let bogus =
            b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-nope\n+two\n";
        let error = apply_sandbox_diff(dir.path(), bogus).await.unwrap_err();
        assert_eq!(error.to_string(), "sandbox_diff_apply_failed");
    }
}
