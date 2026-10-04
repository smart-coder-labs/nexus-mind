//! Moving a checkout into and out of a task pod (factory F1, design §4).
//!
//! In: a `tar` of the worker's checkout files, without `.git` (the pod builds its
//! own baseline commit; it never needs history, and a worktree's `.git` is a file
//! pointing outside the directory). The clone credential travels in environment
//! variables (`authenticated_git`), so it is never on disk; as a second line, the
//! archive is refused if any known secret value appears in it.
//!
//! Out: the pod's `git diff --binary`, applied to the worker's working tree
//! (symlinks and gitlinks refused), after which the usual secret scan and gates
//! run exactly as for a local agent's edits.

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
    // The top-level entries, not ".": the pod's /workspace is a root-owned emptyDir
    // and a non-root `tar -x` fails restoring the root's mode and mtime. Any `.git`
    // at any depth is left out (the repository, a worktree's `.git` file,
    // submodules, vendored repositories); the pod builds its own baseline. So is
    // any `.codex`: the Codex CLI starts the MCP servers a repository's
    // `.codex/config.toml` declares, which would run repository commands in the
    // agent pod before the model acts (verified with codex-cli 0.160).
    let mut child = Command::new("sh")
        .current_dir(workdir)
        .args([
            "-c",
            "find . -mindepth 1 -maxdepth 1 ! -name .git ! -name .codex -print0 | tar --null -c -f - --exclude=.git --exclude=.codex -T -",
        ])
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

/// Puts `workdir` back at `HEAD` (tracked and untracked changes dropped), then
/// applies `diff`: every diff from the pod is the whole change so far, so the
/// checkout always mirrors the pod's latest state.
pub async fn reset_and_apply(workdir: &Path, diff: &[u8]) -> anyhow::Result<()> {
    for args in [
        &["reset", "-q", "--hard", "HEAD"][..],
        &["clean", "-q", "-fd"][..],
    ] {
        let status = Command::new("git")
            .current_dir(workdir)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status()
            .await?;
        if !status.success() {
            anyhow::bail!("workspace_reset_failed")
        }
    }
    apply_sandbox_diff(workdir, diff).await
}

/// Whether a diff creates or changes a symlink (120000) or a gitlink (160000).
/// The pod is untrusted: a symlink written into the worker's checkout could make
/// the worker read and publish any file it can reach (its environment, tokens).
fn has_refused_mode(diff: &[u8]) -> bool {
    diff.split(|b| *b == b'\n').any(|line| {
        let line = String::from_utf8_lossy(line);
        let mode = if let Some(rest) = line.strip_prefix("new file mode ") {
            Some(rest)
        } else if let Some(rest) = line.strip_prefix("new mode ") {
            Some(rest)
        } else if line.starts_with("index ") {
            // `index <old>..<new> <mode>` on a modification of an existing file.
            line.split_whitespace().nth(2)
        } else {
            None
        };
        matches!(mode.map(str::trim), Some("120000" | "160000"))
    })
}

/// Applies a diff produced in the pod to `workdir`. An empty diff is a no-op.
pub async fn apply_sandbox_diff(workdir: &Path, diff: &[u8]) -> anyhow::Result<()> {
    if diff.is_empty() {
        return Ok(());
    }
    if has_refused_mode(diff) {
        anyhow::bail!("sandbox_diff_mode_refused")
    }
    // Working tree only, as if the agent had edited the files here: the publish
    // gates (secret scan, limits, excluded paths) diff the tree against the index.
    let mut child = Command::new("git")
        .current_dir(workdir)
        .args(["apply", "--binary", "-"])
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
    async fn the_archive_has_no_root_entry_so_the_pod_never_touches_the_workspace_root() {
        let dir = repo().await;
        let raw = Command::new("tar")
            .args(["-t", "-f", "-"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        let archive = pack_workspace(dir.path(), &[]).await.unwrap();
        let mut raw = raw;
        raw.stdin.take().unwrap().write_all(&archive).await.unwrap();
        let listing = String::from_utf8(raw.wait_with_output().await.unwrap().stdout).unwrap();
        // The pod's /workspace is owned by root: restoring "." metadata fails there.
        assert!(
            !listing.lines().any(|line| line == "./" || line == "."),
            "{listing}"
        );
        assert!(
            listing.lines().any(|line| line.ends_with("a.txt")),
            "{listing}"
        );
    }

    #[tokio::test]
    async fn the_archive_carries_the_files_but_no_git_metadata() {
        let dir = repo().await;
        std::fs::create_dir(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/.gitkeep"), "").unwrap();
        let archive = pack_workspace(dir.path(), &[]).await.unwrap();
        let entries = list(&archive).await;
        assert!(entries.iter().any(|e| e == "a.txt"), "{entries:?}");
        assert!(entries.iter().any(|e| e == "src/.gitkeep"), "{entries:?}");
        assert!(
            !entries
                .iter()
                .any(|e| e == ".git" || e.starts_with(".git/")),
            "{entries:?}"
        );
    }

    #[tokio::test]
    async fn codex_configs_never_reach_the_pod() {
        let dir = tempfile::tempdir().unwrap();
        for path in [".codex", "app/.codex"] {
            std::fs::create_dir_all(dir.path().join(path)).unwrap();
            std::fs::write(dir.path().join(path).join("config.toml"), "[mcp_servers.x]\n").unwrap();
        }
        std::fs::write(dir.path().join("app/main.rs"), "fn main() {}\n").unwrap();
        let entries = list(&pack_workspace(dir.path(), &[]).await.unwrap()).await;
        assert!(entries.iter().any(|e| e == "app/main.rs"), "{entries:?}");
        assert!(!entries.iter().any(|e| e.contains(".codex")), "{entries:?}");
    }

    #[tokio::test]
    async fn a_worktree_git_file_is_left_out_too() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(".git"),
            "gitdir: /elsewhere/.git/worktrees/x\n",
        )
        .unwrap();
        std::fs::write(dir.path().join("a.txt"), "one\n").unwrap();
        let entries = list(&pack_workspace(dir.path(), &[]).await.unwrap()).await;
        assert!(!entries.iter().any(|e| e == ".git"), "{entries:?}");
    }

    #[tokio::test]
    async fn each_checkpoint_replaces_the_previous_one() {
        let dir = repo().await;
        let pod = repo().await;
        let diff_of = |dir: std::path::PathBuf| async move {
            git(&dir, &["add", "-A", "-N", "."]).await;
            Command::new("git")
                .current_dir(&dir)
                .args(["diff", "--binary", "HEAD"])
                .output()
                .await
                .unwrap()
                .stdout
        };
        std::fs::write(pod.path().join("first.txt"), "1\n").unwrap();
        let first = diff_of(pod.path().to_path_buf()).await;
        reset_and_apply(dir.path(), &first).await.unwrap();
        assert!(dir.path().join("first.txt").exists());

        std::fs::remove_file(pod.path().join("first.txt")).unwrap();
        std::fs::write(pod.path().join("a.txt"), "two\n").unwrap();
        let second = diff_of(pod.path().to_path_buf()).await;
        reset_and_apply(dir.path(), &second).await.unwrap();
        assert!(
            !dir.path().join("first.txt").exists(),
            "earlier checkpoint must not linger"
        );
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "two\n"
        );

        reset_and_apply(dir.path(), b"").await.unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "one\n"
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
    async fn applied_changes_stay_unstaged_so_publish_gates_see_them() {
        let dir = repo().await;
        let pod = repo().await;
        std::fs::write(pod.path().join("a.txt"), "two\n").unwrap();
        git(pod.path(), &["add", "-A", "-N", "."]).await;
        let diff = Command::new("git")
            .current_dir(pod.path())
            .args(["diff", "--binary", "HEAD"])
            .output()
            .await
            .unwrap()
            .stdout;
        reset_and_apply(dir.path(), &diff).await.unwrap();
        // Publish gates diff the working tree against the index.
        let numstat = Command::new("git")
            .current_dir(dir.path())
            .args(["diff", "--numstat"])
            .output()
            .await
            .unwrap()
            .stdout;
        assert!(
            String::from_utf8_lossy(&numstat).contains("a.txt"),
            "{numstat:?}"
        );
    }

    #[tokio::test]
    async fn symlinks_and_gitlinks_from_the_pod_are_refused() {
        let dir = repo().await;
        for diff in [
            "diff --git a/PENDING.md b/PENDING.md\nnew file mode 120000\nindex 0000000..1111111\n--- /dev/null\n+++ b/PENDING.md\n@@ -0,0 +1 @@\n+/proc/self/environ\n\\ No newline at end of file\n",
            "diff --git a/a.txt b/a.txt\nold mode 100644\nnew mode 120000\n",
            "diff --git a/link b/link\nindex 1111111..2222222 120000\n--- a/link\n+++ b/link\n@@ -1 +1 @@\n-x\n+/etc/passwd\n",
            "diff --git a/vendor b/vendor\nnew file mode 160000\nindex 0000000..1111111\n--- /dev/null\n+++ b/vendor\n@@ -0,0 +1 @@\n+Subproject commit 1111111111111111111111111111111111111111\n",
        ] {
            assert_eq!(
                apply_sandbox_diff(dir.path(), diff.as_bytes()).await.unwrap_err().to_string(),
                "sandbox_diff_mode_refused",
                "{diff}"
            );
        }
        assert!(!dir.path().join("PENDING.md").exists());
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
