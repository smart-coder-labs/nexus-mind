use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};
use tokio::{process::Command, time::timeout};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct RuntimeHealth {
    pub status: String,
    pub reason_code: Option<String>,
    pub claude_version: Option<String>,
    pub checked_at: Option<String>,
    pub last_success_at: Option<String>,
    pub last_failure_at: Option<String>,
}

fn unavailable(reason: &str) -> RuntimeHealth {
    RuntimeHealth {
        status: "unavailable".into(),
        reason_code: Some(reason.into()),
        claude_version: None,
        checked_at: None,
        last_success_at: None,
        last_failure_at: None,
    }
}

fn restricted_command(bin: &str) -> Command {
    let allowed = [
        "HOME",
        "PATH",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "TMPDIR",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "SSL_CERT_FILE",
        "NODE_EXTRA_CA_CERTS",
    ];
    let values = allowed
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (*key, value)))
        .collect::<Vec<_>>();
    let mut command = Command::new(bin);
    command.env_clear();
    for (key, value) in values {
        command.env(key, value);
    }
    command.env("DISABLE_AUTOUPDATER", "1").kill_on_drop(true);
    command
}

/// Probes the operator-managed Claude Code installation without reading its
/// credential files or retaining command output. Authentication remains host
/// infrastructure and is never copied into NexusMind persistence.
pub async fn probe_claude(bin: &str) -> RuntimeHealth {
    if !Path::new(bin).is_absolute() {
        return unavailable("claude_binary_must_be_absolute");
    }
    if !Path::new(bin).is_file() {
        return unavailable("claude_binary_missing");
    }
    if std::fs::symlink_metadata(bin)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return unavailable("claude_binary_symlink");
    }
    let version_output = match timeout(
        Duration::from_secs(10),
        restricted_command(bin).arg("--version").output(),
    )
    .await
    {
        Ok(Ok(output)) if output.status.success() => output,
        Ok(Ok(_)) => return unavailable("claude_version_failed"),
        Ok(Err(error)) if error.kind() == std::io::ErrorKind::NotFound => {
            return unavailable("claude_binary_missing")
        }
        Ok(Err(_)) => return unavailable("claude_spawn_failed"),
        Err(_) => return unavailable("claude_probe_timeout"),
    };
    let version = String::from_utf8_lossy(&version_output.stdout)
        .lines()
        .next()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(120).collect::<String>());
    if !version.as_deref().is_some_and(|value| {
        regex::Regex::new(r"^\d+\.\d+\.\d+.*Claude Code")
            .map(|pattern| pattern.is_match(value))
            .unwrap_or(false)
    }) {
        return unavailable("claude_version_unsupported");
    }

    let auth = timeout(
        Duration::from_secs(15),
        restricted_command(bin)
            .args(["auth", "status", "--json"])
            .output(),
    )
    .await;
    match auth {
        Ok(Ok(output)) if output.status.success() => {
            let status: serde_json::Value = match serde_json::from_slice(&output.stdout) {
                Ok(status) => status,
                Err(_) => return unavailable("claude_auth_status_malformed"),
            };
            if status.get("loggedIn").and_then(|value| value.as_bool()) != Some(true) {
                return RuntimeHealth {
                    status: "reauth_required".into(),
                    reason_code: Some("claude_auth_required".into()),
                    claude_version: version,
                    checked_at: None,
                    last_success_at: None,
                    last_failure_at: None,
                };
            }
            RuntimeHealth {
                status: "ready".into(),
                reason_code: None,
                claude_version: version,
                checked_at: None,
                last_success_at: None,
                last_failure_at: None,
            }
        }
        Ok(Ok(_)) => RuntimeHealth {
            status: "reauth_required".into(),
            reason_code: Some("claude_auth_required".into()),
            claude_version: version,
            checked_at: None,
            last_success_at: None,
            last_failure_at: None,
        },
        Ok(Err(_)) => unavailable("claude_auth_probe_failed"),
        Err(_) => unavailable("claude_auth_probe_timeout"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn executable(contents: &str) -> tempfile::TempPath {
        use std::os::unix::fs::PermissionsExt;
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), contents).unwrap();
        let mut permissions = std::fs::metadata(file.path()).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(file.path(), permissions).unwrap();
        // Close the writable handle before the file gets executed: on Linux,
        // exec of a file still open for writing fails with ETXTBSY (surfaced as
        // claude_spawn_failed), which made these tests flaky on CI runners.
        file.into_temp_path()
    }

    #[tokio::test]
    async fn missing_absolute_binary_is_unavailable_without_shelling_out() {
        let health = probe_claude("/definitely/missing/nexusmind-claude").await;
        assert_eq!(health.status, "unavailable");
        assert_eq!(health.reason_code.as_deref(), Some("claude_binary_missing"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unsupported_binary_and_malformed_auth_status_fail_closed() {
        let unsupported = executable("#!/bin/sh\necho 'not claude'\n");
        let health = probe_claude(unsupported.to_str().unwrap()).await;
        assert_eq!(
            health.reason_code.as_deref(),
            Some("claude_version_unsupported")
        );

        let malformed = executable(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.0 (Claude Code)'; else echo 'not-json'; fi\n",
        );
        let health = probe_claude(malformed.to_str().unwrap()).await;
        assert_eq!(
            health.reason_code.as_deref(),
            Some("claude_auth_status_malformed")
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn valid_machine_readable_authenticated_status_is_ready() {
        let binary = executable(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo '2.1.0 (Claude Code)'; else echo '{\"loggedIn\":true}'; fi\n",
        );
        let health = probe_claude(binary.to_str().unwrap()).await;
        assert_eq!(health.status, "ready");
    }
}
