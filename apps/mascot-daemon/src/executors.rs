//! Tier 1 & Tier 2 local execution primitives (Shell, Filesystem, Safe Trash, AppleScript).
//!
//! Every mutating invocation MUST be approved by `policy::evaluate_action` first.

use crate::redact::LocalRedactor;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const MAX_OUTPUT_BYTES: usize = 32 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExecResult {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub truncated: bool,
}

pub async fn run_shell_command(command: &str, timeout_secs: u64) -> Result<ExecResult> {
    let shell = if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/sh"
    };
    let fut = Command::new(shell).arg("-c").arg(command).output();
    let output = timeout(Duration::from_secs(timeout_secs), fut)
        .await
        .context("shell command timed out")?
        .context("failed to spawn shell command")?;

    let (stdout, out_trunc) = clean_output(&output.stdout);
    let (stderr, err_trunc) = clean_output(&output.stderr);

    Ok(ExecResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        truncated: out_trunc || err_trunc,
    })
}

pub async fn run_applescript(script: &str, timeout_secs: u64) -> Result<ExecResult> {
    if !cfg!(target_os = "macos") {
        bail!("AppleScript executor is only available on macOS");
    }
    let fut = Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script)
        .output();
    let output = timeout(Duration::from_secs(timeout_secs), fut)
        .await
        .context("AppleScript timed out")?
        .context("failed to spawn osascript")?;

    let (stdout, out_trunc) = clean_output(&output.stdout);
    let (stderr, err_trunc) = clean_output(&output.stderr);

    Ok(ExecResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        truncated: out_trunc || err_trunc,
    })
}

pub fn expand_tilde(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home).join(rest)
    } else {
        PathBuf::from(path)
    }
}

pub fn fs_read_text(path: &str, max_bytes: usize) -> Result<String> {
    let p = expand_tilde(path);
    let data = std::fs::read(&p)
        .with_context(|| format!("failed to read file {}", p.display()))?;
    let limit = max_bytes.min(MAX_OUTPUT_BYTES);
    let slice = if data.len() > limit {
        &data[..limit]
    } else {
        &data
    };
    let raw = String::from_utf8_lossy(slice);
    let mut redactor = LocalRedactor::new();
    Ok(redactor.redact(&raw))
}

pub fn fs_write_text(path: &str, content: &str) -> Result<String> {
    let p = expand_tilde(path);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&p, content)
        .with_context(|| format!("failed to write file {}", p.display()))?;
    Ok(format!("Wrote {} bytes to {}", content.len(), p.display()))
}

pub fn fs_list_dir(path: &str, limit: usize) -> Result<Vec<String>> {
    let p = expand_tilde(path);
    let mut items = Vec::new();
    for entry in std::fs::read_dir(&p)
        .with_context(|| format!("failed to list directory {}", p.display()))?
    {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let kind = if entry.file_type()?.is_dir() { "/" } else { "" };
        items.push(format!("{name}{kind}"));
        if items.len() >= limit {
            break;
        }
    }
    items.sort();
    Ok(items)
}

/// Reversible trash: moves file to macOS `~/.Trash` (or `~/.nexusmind/trash` on Linux/other)
/// rather than permanently unlinking user files.
pub fn fs_move_to_trash(path: &str) -> Result<String> {
    let src = expand_tilde(path);
    if !src.exists() {
        bail!("path does not exist: {}", src.display());
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let trash_dir = if cfg!(target_os = "macos") {
        PathBuf::from(home).join(".Trash")
    } else {
        PathBuf::from(home).join(".nexusmind").join("trash")
    };
    std::fs::create_dir_all(&trash_dir)?;
    let fname = src
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "item".into());
    let stamp = chrono::Utc::now().timestamp();
    let dst = unique_trash_path(&trash_dir, &fname, stamp);
    std::fs::rename(&src, &dst)
        .with_context(|| format!("failed to move {} to trash {}", src.display(), dst.display()))?;
    Ok(format!("Moved {} to reversible trash {}", src.display(), dst.display()))
}

fn unique_trash_path(dir: &Path, name: &str, stamp: i64) -> PathBuf {
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    dir.join(format!("{stamp}-{name}"))
}

fn clean_output(bytes: &[u8]) -> (String, bool) {
    let truncated = bytes.len() > MAX_OUTPUT_BYTES;
    let slice = if truncated {
        &bytes[..MAX_OUTPUT_BYTES]
    } else {
        bytes
    };
    let raw = String::from_utf8_lossy(slice);
    let mut redactor = LocalRedactor::new();
    (redactor.redact(&raw), truncated)
}
