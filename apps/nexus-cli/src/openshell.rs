//! Mandatory NVIDIA OpenShell boundary for agent processes and user commands.
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Output, Stdio},
    time::{Duration, Instant},
};
use uuid::Uuid;

const INSTALLER: &str = "https://raw.githubusercontent.com/NVIDIA/OpenShell/main/install.sh";
const CODEX_AUTH_BOOTSTRAP_SCRIPT: &str = r#"import json, os, pathlib, sys
auth = json.loads(sys.argv[2])
for field, name in (("access_token", "CODEX_AUTH_ACCESS_TOKEN"), ("refresh_token", "CODEX_AUTH_REFRESH_TOKEN"), ("account_id", "CODEX_AUTH_ACCOUNT_ID")):
    value = os.environ.get(name)
    if not value:
        raise SystemExit("OpenShell login provider is incomplete")
    if not value.startswith("openshell:resolve:env:"):
        raise SystemExit("OpenShell login provider exposed a non-placeholder credential")
    auth["tokens"][field] = value
directory = pathlib.Path(sys.argv[1])
directory.mkdir(mode=0o700, parents=True, exist_ok=True)
path = directory / "auth.json"
descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
with os.fdopen(descriptor, "w") as file:
    json.dump(auth, file)
os.chmod(path, 0o600)
"#;

#[derive(Clone, Debug)]
pub struct OpenShell {
    binary: PathBuf,
    sandbox: String,
    workspace: String,
}

/// Fingerprints host files before an agent turn so a late sandbox download
/// cannot silently overwrite a concurrent host edit.
pub struct HostSnapshot(BTreeMap<PathBuf, Vec<u8>>);

impl HostSnapshot {
    pub fn capture(root: &Path) -> Result<Self> {
        let mut files = BTreeMap::new();
        for relative in selected_paths(root)? {
            #[cfg(unix)]
            let relative = PathBuf::from(OsStr::from_bytes(&relative));
            #[cfg(not(unix))]
            let relative = PathBuf::from(String::from_utf8(relative)?);
            let path = root.join(&relative);
            if path.is_file() && !path.is_symlink() {
                files.insert(relative, digest(&path)?);
            }
        }
        Ok(Self(files))
    }
}

impl OpenShell {
    pub fn codex_auth_dir(&self) -> &'static str {
        "/sandbox/.nexus-codex-auth"
    }

    pub fn prepare_codex_auth(&self) -> Result<()> {
        let now = chrono::Utc::now();
        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"none","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&serde_json::json!({
            "iss": "https://auth.openai.com",
            "aud": "codex",
            "sub": "openshell-nexus",
            "email": "nexus@openshell.local",
            "iat": now.timestamp(),
            "exp": (now + chrono::Duration::hours(1)).timestamp()
        }))?);
        let auth = serde_json::json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": format!("{header}.{payload}.placeholder"),
                "access_token": "openshell:resolve:env:CODEX_AUTH_ACCESS_TOKEN",
                "refresh_token": "openshell:resolve:env:CODEX_AUTH_REFRESH_TOKEN",
                "account_id": "openshell:resolve:env:CODEX_AUTH_ACCOUNT_ID"
            },
            "last_refresh": now.to_rfc3339()
        })
        .to_string();
        // Keep opaque references in the sandbox auth file. OpenShell resolves
        // them in outbound HTTP headers; writing real tokens here bypasses
        // its credential binding and can cause an upstream 401.
        for _ in 0..6 {
            let output = self.raw_command_output(&[
                "python3",
                "-c",
                CODEX_AUTH_BOOTSTRAP_SCRIPT,
                self.codex_auth_dir(),
                &auth,
            ])?;
            if output.status.success() {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        bail!("OpenShell did not expose the Codex login provider; retry `nexus codex`");
    }

    pub fn clear_codex_auth(&self) {
        let _ = self.raw_command_output(&["rm", "-f", "/sandbox/.nexus-codex-auth/auth.json"]);
    }

    pub fn auth_status(&self, runtime: &str) -> Result<String> {
        let output = match runtime {
            "codex-headless" => {
                self.prepare_codex_auth()?;
                let output = self.raw_command_output(&[
                    "env",
                    "CODEX_HOME=/sandbox/.nexus-codex-auth",
                    "codex",
                    "login",
                    "status",
                ]);
                self.clear_codex_auth();
                output?
            }
            "claude-code-headless" => self.raw_command_output(&["claude", "auth", "status"])?,
            _ => bail!("auth status is available for Claude Code and Codex only"),
        };
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() {
            bail!(
                "{} login is not ready in OpenShell: {}",
                runtime,
                crate::redacted(text.trim())
            );
        }
        Ok(crate::redacted(text.trim()))
    }

    pub fn installed() -> bool {
        find_binary(&env::var_os("NEXUS_OPENSHELL_BIN").unwrap_or_else(|| "openshell".into()))
            .is_some()
    }

    /// Install via NVIDIA's official installer when absent. Never run locally.
    pub fn ensure_installed() -> Result<PathBuf> {
        let requested = env::var_os("NEXUS_OPENSHELL_BIN").unwrap_or_else(|| "openshell".into());
        if let Some(binary) = find_binary(&requested) {
            return Ok(binary);
        }
        if env::var_os("NEXUS_OPENSHELL_BIN").is_some() {
            bail!("NEXUS_OPENSHELL_BIN points to a missing executable");
        }
        if env::var_os("NEXUS_OPENSHELL_NO_INSTALL").is_some() {
            bail!("OpenShell CLI is required in the worker image; automatic installation is disabled");
        }
        eprintln!("OpenShell is required; installing it with NVIDIA's official installer…");
        let script = env::temp_dir().join(format!("nexus-openshell-install-{}.sh", Uuid::new_v4()));
        let download = Command::new("curl")
            .args(["-fLsS", "--retry", "3", "--output"])
            .arg(&script)
            .arg(INSTALLER)
            .status()
            .context("could not download the official OpenShell installer")?;
        if !download.success() {
            bail!("OpenShell installer download failed ({download})");
        }
        let status = Command::new("sh").arg(&script).status();
        let _ = fs::remove_file(&script);
        let status = status.context("could not start the official OpenShell installer")?;
        if !status.success() {
            bail!("OpenShell installation failed ({status}); see https://docs.nvidia.com/openshell/latest/about/installation");
        }
        find_binary(OsStr::new("openshell"))
            .or_else(|| find_binary(OsStr::new("/opt/homebrew/bin/openshell")))
            .context("installer completed but the OpenShell executable was not found")
    }

    pub fn ensure(root: &Path, runtime: &str) -> Result<Self> {
        let binary = Self::ensure_installed()?;
        let status = Command::new(&binary).arg("status").output()?;
        if !status.status.success() {
            bail!(
                "OpenShell gateway is unavailable: {}",
                String::from_utf8_lossy(&status.stderr).trim()
            );
        }
        let mut providers = ensure_provider(&binary, runtime)?.into_iter().collect::<Vec<_>>();
        if let Ok(extra) = env::var("NEXUS_OPENSHELL_EXTRA_PROVIDERS") {
            for name in extra.split(',').map(str::trim).filter(|name| !name.is_empty()) {
                if !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
                {
                    bail!("invalid OpenShell provider name");
                }
                if !providers.iter().any(|existing| existing == name) {
                    providers.push(name.to_string());
                }
            }
        }
        let configured_sandbox = env::var("NEXUS_OPENSHELL_SANDBOX").ok();
        let managed_sandbox = configured_sandbox.is_none();
        let create_named_sandbox =
            env::var("NEXUS_OPENSHELL_CREATE_NAMED").as_deref() == Ok("1");
        let image = if configured_sandbox.is_some() {
            env::var("NEXUS_OPENSHELL_IMAGE").unwrap_or_else(|_| "base".into())
        } else {
            image_for(root)?
        };
        let mut sandbox = configured_sandbox.unwrap_or_else(|| sandbox_name(root, runtime, &image));
        if sandbox.trim().is_empty()
            || !sandbox
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-')
        {
            bail!("invalid OpenShell sandbox name");
        }
        let basename = root
            .file_name()
            .unwrap_or_else(|| OsStr::new("project"))
            .to_string_lossy();
        let workspace = format!("/sandbox/{basename}");
        let original_sandbox = sandbox.clone();
        let mut recovery_index = 0_u8;
        let mut waiting_for_ready = false;
        let readiness_deadline = Instant::now() + Duration::from_secs(120);
        loop {
            let get = Command::new(&binary)
                .args(["sandbox", "get", &sandbox, "--output", "json"])
                .output()?;
            if get.status.success() {
                let details: serde_json::Value = serde_json::from_slice(&get.stdout)
                    .context("OpenShell returned invalid sandbox details")?;
                match details.get("phase").and_then(serde_json::Value::as_str) {
                Some("Ready") => {}
                Some("Stopped") => {
                    let started = Command::new(&binary)
                        .args(["sandbox", "start", &sandbox])
                        .output()?;
                    if !started.status.success() {
                        bail!(
                            "could not restart stopped OpenShell sandbox: {}",
                            String::from_utf8_lossy(&started.stderr).trim()
                        );
                    }
                    waiting_for_ready = true;
                    continue;
                }
                Some("Error") if managed_sandbox && recovery_index < 3 => {
                    eprintln!("OpenShell sandbox `{sandbox}` is in Error; preserving it and selecting a recovery sandbox");
                    recovery_index += 1;
                        // OpenShell caps names at 19 characters; the normal
                        // `nx-x-` + 12-hex name uses 17 of them already.
                        sandbox = format!("{original_sandbox}r{recovery_index}");
                    continue;
                }
                Some("Error") => bail!(
                    "OpenShell sandbox `{sandbox}` is in Error. Its workspace has been preserved. Inspect `openshell sandbox get {sandbox} --output json` and `openshell logs {sandbox}`. This OpenShell version cannot start or stop an Error sandbox; do not delete it until any needed files have been recovered"
                ),
                _other if waiting_for_ready && Instant::now() < readiness_deadline => {
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                other => bail!(
                    "OpenShell sandbox `{sandbox}` is not ready (phase: {}). Inspect `openshell sandbox get {sandbox} --output json` and `openshell logs {sandbox}`",
                    other.unwrap_or("unknown")
                ),
            }
                for provider in &providers {
                    let listed = Command::new(&binary)
                        .args(["sandbox", "provider", "list", &sandbox])
                        .output()?;
                    if !listed.status.success() {
                        bail!(
                            "could not inspect OpenShell login providers ({})",
                            listed.status
                        );
                    }
                    let present = String::from_utf8_lossy(&listed.stdout)
                        .lines()
                        .any(|line| line.split_whitespace().next() == Some(provider.as_str()));
                    if !present {
                        let attached = Command::new(&binary)
                            .args(["sandbox", "provider", "attach", &sandbox, provider])
                            .output()?;
                        if !attached.status.success() {
                            bail!(
                                "could not attach the OpenShell login provider ({}): {}",
                                attached.status,
                                crate::redacted(String::from_utf8_lossy(&attached.stderr).trim())
                            );
                        }
                    }
                }
            } else {
                if waiting_for_ready {
                    if Instant::now() >= readiness_deadline {
                        bail!("new OpenShell sandbox `{sandbox}` did not become visible within 120 seconds");
                    }
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
                if !managed_sandbox && !create_named_sandbox {
                    bail!("configured OpenShell sandbox `{sandbox}` does not exist or is inaccessible");
                }
                let mut create = Command::new(&binary);
                create.args([
                    "sandbox", "create", "--name", &sandbox, "--detach", "--from", &image,
                ]);
                for provider in &providers {
                    create.args(["--provider", provider]);
                }
                let output = create
                    .output()
                    .context("could not create OpenShell sandbox")?;
                if !output.status.success() {
                    bail!(
                        "OpenShell sandbox creation failed: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                waiting_for_ready = true;
                continue;
            }
            break;
        }
        if runtime == "codex-headless" {
            ensure_codex_policy(&binary, &sandbox, managed_sandbox)?;
        }
        Ok(Self {
            binary,
            sandbox,
            workspace,
        })
    }

    pub fn sandbox(&self) -> &str {
        &self.sandbox
    }
    pub fn workspace(&self) -> &str {
        &self.workspace
    }

    pub fn upload(&self, root: &Path) -> Result<()> {
        let existing =
            self.raw_command_output(&["git", "-C", &self.workspace, "status", "--porcelain"])?;
        if existing.status.success() && !existing.stdout.is_empty() {
            bail!("OpenShell workspace has unsynchronized changes; inspect sandbox `{}` before a new upload", self.sandbox);
        }
        // A filtered archive prevents .git, .nexus sessions, ignored build
        // artifacts, and common credential files from entering the sandbox.
        let archive = make_archive(root)?;
        let remote = format!(
            "/sandbox/{}",
            archive
                .file_name()
                .unwrap_or_else(|| OsStr::new("nexus-upload.tar"))
                .to_string_lossy()
        );
        let mkdir = self.raw_command_output(&["mkdir", "-p", &self.workspace])?;
        if !mkdir.status.success() {
            bail!("could not prepare OpenShell workspace");
        }
        let output = Command::new(&self.binary)
            .args(["sandbox", "upload", &self.sandbox])
            .arg(&archive)
            .arg("/sandbox")
            .output()?;
        let _ = fs::remove_file(&archive);
        if !output.status.success() {
            bail!(
                "OpenShell repository upload failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        self.remove_stale_guest_files(root)?;
        let extract = self.command_output(&["tar", "-xf", &remote, "-C", &self.workspace], None)?;
        let _ = self.raw_command_output(&["rm", "-f", &remote]);
        if !extract.status.success() {
            bail!(
                "OpenShell archive extraction failed: {}",
                String::from_utf8_lossy(&extract.stderr).trim()
            );
        }
        let baseline = self.command_output(&["sh", "-c",
            "git init -q && git add -A && git -c user.name=Nexus -c user.email=nexus@localhost commit -qm 'Nexus sandbox baseline' --allow-empty"], None)?;
        if !baseline.status.success() {
            bail!(
                "could not prepare Git baseline in OpenShell: {}",
                String::from_utf8_lossy(&baseline.stderr).trim()
            );
        }
        Ok(())
    }

    /// Copy worker-only configuration outside the synchronized project tree.
    /// The caller must never put credentials in these files; OpenShell providers
    /// own secret injection.
    pub fn upload_worker_file(&self, source: &Path) -> Result<String> {
        if !source.is_file() {
            bail!("worker configuration file does not exist");
        }
        let destination = format!("/sandbox/.nexus-worker-{}", Uuid::new_v4());
        let prepared = self.raw_command_output(&["mkdir", "-m", "700", &destination])?;
        if !prepared.status.success() {
            bail!("could not prepare OpenShell worker configuration directory");
        }
        let uploaded = Command::new(&self.binary)
            .args(["sandbox", "upload", &self.sandbox])
            .arg(source)
            .arg(&destination)
            .output()?;
        if !uploaded.status.success() {
            bail!("could not upload worker configuration to OpenShell");
        }
        let filename = source
            .file_name()
            .context("worker configuration has no filename")?
            .to_string_lossy();
        Ok(format!("{destination}/{filename}"))
    }

    pub fn upload_worker_mcp_config(&self, source: &Path) -> Result<String> {
        let body = fs::read(source).context("could not read worker MCP configuration")?;
        if body.len() > 64 * 1024 {
            bail!("worker MCP configuration exceeds 64 KiB");
        }
        let mut config: serde_json::Value =
            serde_json::from_slice(&body).context("worker MCP configuration is invalid JSON")?;
        if let Some(servers) = config.get("mcpServers").and_then(serde_json::Value::as_object) {
            for server in servers.values() {
                if server
                    .get("env")
                    .and_then(serde_json::Value::as_object)
                    .is_some_and(|env| {
                        env.keys().any(|key| {
                            let upper = key.to_ascii_uppercase();
                            upper.contains("KEY")
                                || upper.contains("TOKEN")
                                || upper.contains("SECRET")
                                || upper.contains("PASSWORD")
                        })
                    })
                {
                    bail!("worker MCP configuration must not contain credential environment variables; use an OpenShell provider");
                }
            }
        }
        if let Some(args) = config
            .pointer_mut("/mcpServers/playwright/args")
            .and_then(serde_json::Value::as_array_mut)
        {
            if let Some(index) = args.iter().position(|value| value.as_str() == Some("--output-dir")) {
                args.remove(index);
                if index < args.len() {
                    args.remove(index);
                }
            }
            let screenshots = format!("{}/screenshots", self.workspace);
            let prepared = self.raw_command_output(&["mkdir", "-p", &screenshots])?;
            if !prepared.status.success() {
                bail!("could not prepare OpenShell screenshot directory");
            }
            args.push(serde_json::json!("--output-dir"));
            args.push(serde_json::json!(screenshots));
        }
        let temporary = env::temp_dir().join(format!("nexus-worker-mcp-{}.json", Uuid::new_v4()));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(&config)?)?;
        drop(file);
        let uploaded = self.upload_worker_file(&temporary);
        let _ = fs::remove_file(&temporary);
        uploaded
    }

    /// Upload read-only sibling Git repositories for resolver context. The
    /// archive uses the same Git-aware exclusion rules as the main workspace.
    pub fn upload_worker_context(&self, root: &Path) -> Result<String> {
        let archive = make_archive(root)?;
        let destination = format!("/sandbox/.nexus-context-{}", Uuid::new_v4());
        let remote_archive = format!(
            "/sandbox/{}",
            archive.file_name().context("context archive has no filename")?.to_string_lossy()
        );
        let prepared = self.raw_command_output(&["mkdir", "-m", "700", &destination])?;
        if !prepared.status.success() {
            let _ = fs::remove_file(&archive);
            bail!("could not prepare OpenShell context directory");
        }
        let uploaded = Command::new(&self.binary)
            .args(["sandbox", "upload", &self.sandbox])
            .arg(&archive)
            .arg("/sandbox")
            .output();
        let _ = fs::remove_file(&archive);
        let uploaded = uploaded?;
        if !uploaded.status.success() {
            bail!("could not upload worker context to OpenShell");
        }
        let extracted = self.raw_command_output(&[
            "tar", "-xf", &remote_archive, "-C", &destination,
        ])?;
        let _ = self.raw_command_output(&["rm", "-f", &remote_archive]);
        if !extracted.status.success() {
            bail!("could not extract worker context in OpenShell");
        }
        Ok(destination)
    }

    fn remove_stale_guest_files(&self, root: &Path) -> Result<()> {
        let expected: BTreeSet<Vec<u8>> = selected_paths(root)?.into_iter().collect();
        let previous =
            self.raw_command_output(&["git", "-C", &self.workspace, "ls-files", "-z"])?;
        if !previous.status.success() {
            return Ok(());
        } // first upload, before git init
        for relative in previous
            .stdout
            .split(|byte| *byte == 0)
            .filter(|path| !path.is_empty())
        {
            if expected.contains(relative) {
                continue;
            }
            let name = std::str::from_utf8(relative)
                .context("sandbox contains a non-UTF-8 tracked path")?;
            if Path::new(name)
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
            {
                bail!("sandbox returned an unsafe tracked path");
            }
            let target = format!("{}/{name}", self.workspace);
            let removed = self.raw_command_output(&["rm", "-f", "--", &target])?;
            if !removed.status.success() {
                bail!("could not remove stale sandbox file {name}");
            }
        }
        Ok(())
    }

    pub fn download(&self, root: &Path, before: &HostSnapshot) -> Result<usize> {
        let staging = env::temp_dir().join(format!("nexus-sandbox-download-{}", Uuid::new_v4()));
        fs::create_dir(&staging)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staging, fs::Permissions::from_mode(0o700))?;
        }
        let output = Command::new(&self.binary)
            .args(["sandbox", "download", &self.sandbox, &self.workspace])
            .arg(&staging)
            .output()?;
        if !output.status.success() {
            bail!(
                "OpenShell repository download failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        // Directory download copies its contents into the destination.
        let allowed_children = if is_git_worktree(root)? {
            None
        } else {
            Some(
                child_repositories(root)?
                    .into_iter()
                    .map(|(name, _)| name)
                    .collect::<BTreeSet<_>>(),
            )
        };
        let result = sync_files(&staging, &staging, root, before, allowed_children.as_ref())
            .and_then(|copied| Ok(copied + remove_deleted_files(&staging, root, before)?))
            .and_then(|count| {
                let committed = self.command_output(&["sh", "-c",
                    "git add -A && git -c user.name=Nexus -c user.email=nexus@localhost commit -qm 'Nexus synchronized turn' --allow-empty"], None)?;
                if !committed.status.success() {
                    bail!("files synchronized, but the OpenShell Git checkpoint failed: {}", String::from_utf8_lossy(&committed.stderr).trim());
                }
                Ok(count)
            });
        if result.is_ok() {
            let _ = fs::remove_dir_all(&staging);
        } else {
            eprintln!(
                "Sandbox download retained for recovery: {}",
                staging.display()
            );
        }
        result
    }

    pub fn exec_command(
        &self,
        argv: &[&str],
        tty: bool,
        timeout_secs: Option<u64>,
    ) -> Result<Command> {
        if argv.is_empty() {
            bail!("OpenShell command cannot be empty");
        }
        let mut command = Command::new(&self.binary);
        command.args(self.exec_args(argv, timeout_secs, tty));
        Ok(command)
    }

    fn raw_command_output(&self, argv: &[&str]) -> Result<Output> {
        let mut command = Command::new(&self.binary);
        command.args(["sandbox", "exec", "-n", &self.sandbox, "--no-tty", "--"]);
        command.args(argv);
        Ok(command.output()?)
    }

    pub fn command_output(&self, argv: &[&str], timeout_secs: Option<u64>) -> Result<Output> {
        self.exec_command(argv, false, timeout_secs)?
            .output()
            .with_context(|| {
                format!(
                    "could not execute command in OpenShell sandbox `{}`",
                    self.sandbox
                )
            })
    }

    pub fn interactive_shell(&self) -> Result<ExitStatus> {
        let shell = env::var("NEXUS_OPENSHELL_SHELL").unwrap_or_else(|_| "/bin/bash".into());
        self.exec_command(&[&shell], true, None)?
            .status()
            .with_context(|| format!("could not open TTY in OpenShell sandbox `{}`", self.sandbox))
    }

    fn exec_args(&self, argv: &[&str], timeout_secs: Option<u64>, tty: bool) -> Vec<OsString> {
        let mut args = vec![
            "sandbox".into(),
            "exec".into(),
            "-n".into(),
            self.sandbox.as_str().into(),
            "--workdir".into(),
            self.workspace.as_str().into(),
        ];
        if let Some(timeout) = timeout_secs {
            args.extend(["--timeout".into(), timeout.to_string().into()]);
        }
        if tty {
            args.push("--tty".into());
        } else {
            args.push("--no-tty".into());
        }
        args.push("--".into());
        args.extend(argv.iter().map(OsString::from));
        args
    }
}

fn ensure_codex_policy(binary: &Path, sandbox: &str, managed: bool) -> Result<()> {
    const HOSTS: [&str; 3] = ["api.openai.com", "auth.openai.com", "chatgpt.com"];
    const BINARIES: [&str; 3] = [
        "/usr/bin/codex",
        "/usr/bin/node",
        "/usr/lib/node_modules/@openai/**",
    ];
    let current = Command::new(binary)
        .args(["policy", "get", sandbox, "--base", "-o", "json"])
        .output()?;
    if !current.status.success() {
        bail!(
            "could not read OpenShell policy for Codex ({})",
            current.status
        );
    }
    let value: serde_json::Value = serde_json::from_slice(&current.stdout)
        .context("OpenShell returned invalid policy JSON")?;
    let mut policy = value
        .get("policy")
        .cloned()
        .context("OpenShell policy has no payload")?;
    let rule = policy.pointer_mut("/network_policies/codex").context(
        "OpenShell sandbox has no Codex policy rule; configure inspected Codex endpoints",
    )?;
    let listed = rule
        .get("binaries")
        .and_then(serde_json::Value::as_array)
        .context("OpenShell Codex policy has no binary list")?;
    let mut existing_binaries = listed
        .iter()
        .filter_map(|item| item.get("path").and_then(serde_json::Value::as_str))
        .collect::<Vec<_>>();
    existing_binaries.sort_unstable();
    let mut expected_binaries = BINARIES.to_vec();
    expected_binaries.sort_unstable();
    if existing_binaries != expected_binaries {
        bail!("OpenShell Codex policy is customized; set credentialed Codex endpoints to inspected REST manually");
    }
    let endpoints = rule
        .get_mut("endpoints")
        .and_then(serde_json::Value::as_array_mut)
        .context("OpenShell Codex policy has no endpoints")?;
    let mut changed = false;
    for host in HOSTS {
        let endpoint = endpoints
            .iter_mut()
            .find(|item| {
                item.get("host").and_then(serde_json::Value::as_str) == Some(host)
                    && item.get("port").and_then(serde_json::Value::as_u64) == Some(443)
            })
            .with_context(|| format!("OpenShell Codex policy is missing {host}:443"))?;
        if endpoint.get("protocol").and_then(serde_json::Value::as_str) == Some("rest")
            && endpoint.get("access").and_then(serde_json::Value::as_str) == Some("read-write")
            && endpoint
                .get("enforcement")
                .and_then(serde_json::Value::as_str)
                == Some("enforce")
        {
            continue;
        }
        if endpoint.get("protocol").is_some() || endpoint.get("access").is_some() {
            bail!("OpenShell Codex policy has a custom endpoint for {host}; configure inspected REST manually");
        }
        let map = endpoint
            .as_object_mut()
            .context("OpenShell Codex endpoint is not an object")?;
        map.insert("protocol".into(), serde_json::Value::String("rest".into()));
        map.insert(
            "access".into(),
            serde_json::Value::String("read-write".into()),
        );
        map.insert(
            "enforcement".into(),
            serde_json::Value::String("enforce".into()),
        );
        changed = true;
    }
    if !changed {
        return Ok(());
    }
    if !managed {
        bail!("configured OpenShell sandbox has an L4 Codex policy; set inspected REST endpoints manually");
    }
    let fresh = Command::new(binary)
        .args(["policy", "get", sandbox, "--base", "-o", "json"])
        .output()?;
    if !fresh.status.success() || fresh.stdout != current.stdout {
        bail!("OpenShell policy changed while preparing Codex; retry without overwriting concurrent changes");
    }
    let temporary = env::temp_dir().join(format!("nexus-codex-policy-{}.yaml", Uuid::new_v4()));
    fs::write(&temporary, serde_json::to_vec_pretty(&policy)?)?;
    let applied = Command::new(binary)
        .args(["policy", "set", sandbox, "--policy"])
        .arg(&temporary)
        .arg("--wait")
        .output();
    let _ = fs::remove_file(&temporary);
    let applied = applied?;
    if !applied.status.success() {
        bail!(
            "could not activate inspected Codex policy ({}): {}",
            applied.status,
            crate::redacted(String::from_utf8_lossy(&applied.stderr).trim())
        );
    }
    Ok(())
}

fn excluded(name: &OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    matches!(
        name,
        ".git" | ".nexus" | "target" | "node_modules" | "__pycache__" | ".env"
    ) || name.starts_with(".env.")
        || name.starts_with("._")
}

fn make_archive(root: &Path) -> Result<PathBuf> {
    let listing = selected_paths(root)?;
    if listing.is_empty() {
        bail!("Git repository contains no files to upload");
    }
    let archive = env::temp_dir().join(format!("nexus-upload-{}.tar", Uuid::new_v4()));
    let mut child = Command::new("tar")
        .env("COPYFILE_DISABLE", "1")
        .args(["-cf"])
        .arg(&archive)
        .arg("-C")
        .arg(root)
        .args(["--null", "-T", "-"])
        .stdin(Stdio::piped())
        .spawn()
        .context("could not create OpenShell upload archive")?;
    if let Some(mut stdin) = child.stdin.take() {
        for path in listing {
            stdin.write_all(&path)?;
            stdin.write_all(&[0])?;
        }
    }
    let status = child.wait()?;
    if !status.success() {
        let _ = fs::remove_file(&archive);
        bail!("could not archive project files for OpenShell");
    }
    Ok(archive)
}

fn selected_paths(root: &Path) -> Result<Vec<Vec<u8>>> {
    check_repository(root)?;
    let repositories = if is_git_worktree(root)? {
        vec![(None, root.to_path_buf())]
    } else {
        child_repositories(root)?
            .into_iter()
            .map(|(name, path)| (Some(name), path))
            .collect()
    };
    let mut selected = Vec::new();
    for (name, repository) in repositories {
        selected.extend(selected_paths_in_repository(
            root,
            &repository,
            name.as_deref(),
        )?);
    }
    selected.sort();
    selected.dedup();
    Ok(selected)
}

fn selected_paths_in_repository(
    root: &Path,
    repository: &Path,
    name: Option<&OsStr>,
) -> Result<Vec<Vec<u8>>> {
    let listing = Command::new("git")
        .current_dir(repository)
        .args([
            "ls-files",
            "--cached",
            "--others",
            "--exclude-standard",
            "-z",
        ])
        .output()
        .context("could not list project files for OpenShell")?;
    if !listing.status.success() {
        bail!("could not list Git files for the OpenShell upload");
    }
    let mut paths = Vec::new();
    for path in listing
        .stdout
        .split(|byte| *byte == 0)
        .filter(|path| !path.is_empty())
    {
        if path.starts_with(b"/")
            || path.split(|byte| *byte == b'/').any(|part| {
                part.is_empty()
                    || part == b"."
                    || part == b".."
                    || part == b".git"
                    || part == b".nexus"
                    || part == b"target"
                    || part == b"node_modules"
                    || part == b"__pycache__"
                    || part == b".env"
                    || part.starts_with(b".env.")
                    || part.starts_with(b"._")
            })
        {
            continue;
        }
        #[cfg(unix)]
        let relative = PathBuf::from(OsStr::from_bytes(path));
        #[cfg(not(unix))]
        let relative = PathBuf::from(String::from_utf8_lossy(path).into_owned());
        let mut workspace_path = PathBuf::new();
        if let Some(name) = name {
            workspace_path.push(name);
        }
        workspace_path.push(&relative);
        let host_file = root.join(&workspace_path);
        if !fs::symlink_metadata(host_file).is_ok_and(|metadata| metadata.file_type().is_file()) {
            continue;
        }
        #[cfg(unix)]
        paths.push(workspace_path.as_os_str().as_bytes().to_vec());
        #[cfg(not(unix))]
        paths.push(workspace_path.to_string_lossy().as_bytes().to_vec());
    }
    Ok(paths)
}

fn is_git_worktree(root: &Path) -> Result<bool> {
    let output = Command::new("git")
        .current_dir(root)
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()
        .context("could not check whether the project is a Git repository")?;
    Ok(output.status.success() && output.stdout == b"true\n")
}

fn child_repositories(root: &Path) -> Result<Vec<(OsString, PathBuf)>> {
    let mut children = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join(".git").exists() {
            children.push((entry.file_name(), entry.path()));
        }
    }
    children.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(children)
}

/// A multi-repository parent is allowed, but only files selected by each
/// child repository's own Git ignore rules can enter the sandbox.
pub fn check_repository(root: &Path) -> Result<()> {
    if is_git_worktree(root)? || !child_repositories(root)?.is_empty() {
        return Ok(());
    }
    bail!(
        "`{}` is neither a Git repository nor a folder of direct child Git repositories. Initialize Git and review .gitignore before using Nexus here.",
        root.display()
    )
}

pub fn workspace_description(root: &Path) -> Result<String> {
    if is_git_worktree(root)? {
        return Ok("single Git repository".into());
    }
    let children = child_repositories(root)?.len();
    if children > 0 {
        Ok(format!(
            "{children} child Git repositories; loose parent files excluded"
        ))
    } else {
        Ok("not Git-ready".into())
    }
}

pub fn is_multi_repository_workspace(root: &Path) -> Result<bool> {
    Ok(!is_git_worktree(root)? && !child_repositories(root)?.is_empty())
}

pub fn git_repositories(root: &Path) -> Result<Vec<(String, PathBuf)>> {
    if is_git_worktree(root)? {
        return Ok(vec![(String::new(), root.to_path_buf())]);
    }
    let children = child_repositories(root)?;
    if children.is_empty() {
        check_repository(root)?;
    }
    Ok(children
        .into_iter()
        .map(|(name, path)| (name.to_string_lossy().into_owned(), path))
        .collect())
}

fn digest(path: &Path) -> Result<Vec<u8>> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().to_vec())
}

fn collect_hashes(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, Vec<u8>>) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if excluded(&entry.file_name()) {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            collect_hashes(root, &path, files)?;
        } else if kind.is_file() {
            files.insert(path.strip_prefix(root)?.to_path_buf(), digest(&path)?);
        }
    }
    Ok(())
}

fn remove_deleted_files(
    staging: &Path,
    target_root: &Path,
    before: &HostSnapshot,
) -> Result<usize> {
    let mut remote = BTreeMap::new();
    collect_hashes(staging, staging, &mut remote)?;
    let deletions = before
        .0
        .iter()
        .filter(|(relative, _)| !remote.contains_key(*relative))
        .collect::<Vec<_>>();
    if deletions.len() > 100 {
        bail!(
            "OpenShell requested deletion of {} host files; refusing bulk deletion",
            deletions.len()
        );
    }
    for (relative, original) in &deletions {
        let target = target_root.join(relative);
        if !target.is_file() || digest(&target)? != **original {
            bail!(
                "host file changed during OpenShell turn; refusing to delete {}",
                target.display()
            );
        }
    }
    for (relative, _) in &deletions {
        fs::remove_file(target_root.join(relative))?;
    }
    Ok(deletions.len())
}

fn sync_files(
    source_root: &Path,
    dir: &Path,
    target_root: &Path,
    before: &HostSnapshot,
    allowed_children: Option<&BTreeSet<OsString>>,
) -> Result<usize> {
    if !source_root.is_dir() {
        bail!("OpenShell download did not contain the repository directory");
    }
    if dir == source_root {
        if let Some(allowed) = allowed_children {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                if !excluded(&entry.file_name()) && !allowed.contains(&entry.file_name()) {
                    bail!(
                        "OpenShell wrote outside a selected child repository: {}",
                        entry.path().display()
                    );
                }
            }
        }
    }
    let mut copied = 0;
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if excluded(&entry.file_name()) {
            continue;
        }
        let source = entry.path();
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copied += sync_files(source_root, &source, target_root, before, allowed_children)?;
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        let relative = source.strip_prefix(source_root)?;
        let mut checked = target_root.to_path_buf();
        for component in relative.components() {
            if let std::path::Component::Normal(part) = component {
                checked.push(part);
                if fs::symlink_metadata(&checked)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    bail!(
                        "refusing to synchronize through a host symlink: {}",
                        checked.display()
                    );
                }
            } else {
                bail!("OpenShell returned an unsafe relative path");
            }
        }
        let target = target_root.join(relative);
        let remote_hash = digest(&source)?;
        if target.is_file() && digest(&target)? == remote_hash {
            continue;
        }
        match (before.0.get(relative), target.is_file()) {
            (Some(original), true) if &digest(&target)? == original => {}
            (None, false) => {}
            _ => bail!(
                "host file changed during OpenShell turn; refusing to overwrite {}",
                target.display()
            ),
        }
        let parent = target.parent().context("target file has no parent")?;
        fs::create_dir_all(parent)?;
        let temporary = parent.join(format!(".nexus-sync-{}", Uuid::new_v4()));
        fs::copy(&source, &temporary)?;
        fs::rename(&temporary, &target)?;
        copied += 1;
    }
    Ok(copied)
}

fn ensure_provider(binary: &Path, runtime: &str) -> Result<Option<String>> {
    let (name, kind, key) = match runtime {
        "claude-code-headless" if env::var_os("ANTHROPIC_API_KEY").is_some() => {
            ("nexus-claude", "claude-code", Some("ANTHROPIC_API_KEY"))
        }
        "codex-headless" => ("nexus-codex-login", "codex", None),
        _ => return Ok(None),
    };
    let codex_tokens = if runtime == "codex-headless" {
        Some(host_codex_tokens()?)
    } else {
        None
    };
    let existing = Command::new(binary)
        .args(["provider", "get", name])
        .output()?;
    if !existing.status.success() || codex_tokens.is_some() {
        let mut create = Command::new(binary);
        if existing.status.success() {
            create.args(["provider", "update", name]);
        } else {
            create.args(["provider", "create", "--name", name, "--type", kind]);
        }
        if let Some(key) = key {
            create.args(["--credential", key]);
        }
        if let Some(tokens) = &codex_tokens {
            for (name, value) in tokens {
                create.args(["--credential", name]);
                create.env(name, value);
            }
        }
        let created = create.output()?;
        if !created.status.success() {
            let mut detail = String::from_utf8_lossy(&created.stderr).to_string();
            if let Some(tokens) = &codex_tokens {
                for (_, value) in tokens {
                    detail = detail.replace(value, "[redacted]");
                }
            }
            bail!(
                "OpenShell login provider setup failed ({}): {}",
                created.status,
                crate::redacted(detail.trim())
            );
        }
    }
    Ok(Some(name.into()))
}

fn host_codex_tokens() -> Result<Vec<(&'static str, String)>> {
    let config_root = env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .context("Codex home directory not found; run `codex login` first")?;
    let auth_path = config_root.join("auth.json");
    let data = fs::read(&auth_path)
        .context("Codex has no reusable local login; run `codex login` first")?;
    let auth: serde_json::Value = serde_json::from_slice(&data)
        .context("Codex login file is invalid; run `codex login` again")?;
    parse_codex_tokens(&auth)
}

fn parse_codex_tokens(auth: &serde_json::Value) -> Result<Vec<(&'static str, String)>> {
    if auth.get("auth_mode").and_then(serde_json::Value::as_str) != Some("chatgpt") {
        bail!("Codex is not signed in with ChatGPT; run `codex login` to use Nexus without an API key");
    }
    let tokens = auth
        .get("tokens")
        .context("Codex login has no tokens; run `codex login` again")?;
    let mut values = Vec::new();
    for (field, env_name, required) in [
        ("access_token", "CODEX_AUTH_ACCESS_TOKEN", true),
        ("refresh_token", "CODEX_AUTH_REFRESH_TOKEN", true),
        ("account_id", "CODEX_AUTH_ACCOUNT_ID", true),
        ("id_token", "CODEX_AUTH_ID_TOKEN", false),
    ] {
        if let Some(value) = tokens
            .get(field)
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.is_empty())
        {
            values.push((env_name, value.to_string()));
        } else if required {
            bail!("Codex login is incomplete; run `codex login` again");
        }
    }
    Ok(values)
}

fn image_for(root: &Path) -> Result<String> {
    if let Ok(image) = env::var("NEXUS_OPENSHELL_IMAGE") {
        if image.trim().is_empty() {
            bail!("NEXUS_OPENSHELL_IMAGE cannot be empty");
        }
        return Ok(image);
    }
    if root.join("Cargo.toml").exists() || root.join("apps/backend/Cargo.toml").exists() {
        return build_rust_image();
    }
    Ok("base".into())
}

fn build_rust_image() -> Result<String> {
    const DOCKERFILE: &str = include_str!("../sandbox/Dockerfile");
    let digest = Sha256::digest(DOCKERFILE.as_bytes());
    let suffix: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let image = format!("localhost/nexus-openshell-rust:{suffix}");
    let existing = Command::new("docker").args(["image", "inspect", &image]).output()
        .context("Docker is required to build the Rust-capable OpenShell image; set NEXUS_OPENSHELL_IMAGE to an existing image to override")?;
    if existing.status.success() {
        return Ok(image);
    }
    let staging = env::temp_dir().join(format!("nexus-rust-image-{}", Uuid::new_v4()));
    fs::create_dir(&staging)?;
    let dockerfile = staging.join("Dockerfile");
    fs::write(&dockerfile, DOCKERFILE)?;
    let output = Command::new("docker")
        .args(["build", "--tag", &image, "--file"])
        .arg(&dockerfile)
        .arg(&staging)
        .output();
    let _ = fs::remove_dir_all(&staging);
    let output = output.context("could not build the Rust-capable OpenShell image")?;
    if !output.status.success() {
        bail!(
            "Rust-capable OpenShell image build failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(2000)
                .collect::<String>()
        );
    }
    Ok(image)
}

fn sandbox_name(root: &Path, runtime: &str, image: &str) -> String {
    let hash = Sha256::digest(format!("{}:{runtime}:{image}", root.display()).as_bytes());
    let suffix: String = hash
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let kind = if runtime == "shell" {
        "s"
    } else if runtime.starts_with("codex") || runtime.starts_with("openai") {
        "x"
    } else {
        "c"
    };
    format!("nx-{kind}-{suffix}")
}

fn find_binary(requested: &OsStr) -> Option<PathBuf> {
    let path = Path::new(requested);
    if path.components().count() > 1 {
        return path.is_file().then(|| path.to_path_buf());
    }
    env::split_paths(&env::var_os("PATH")?)
        .map(|dir| dir.join(path))
        .find(|candidate| candidate.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn temporary(name: &str) -> PathBuf {
        let path = env::temp_dir().join(format!("nexus-{name}-{}", Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn arguments_preserve_boundaries_and_force_sandbox() {
        let runner = OpenShell {
            binary: "openshell".into(),
            sandbox: "dev".into(),
            workspace: "/workspace/project".into(),
        };
        let args = runner.exec_args(&["sh", "-lc", "echo '$HOME; safe'"], Some(30), false);
        assert_eq!(
            args,
            [
                "sandbox",
                "exec",
                "-n",
                "dev",
                "--workdir",
                "/workspace/project",
                "--timeout",
                "30",
                "--no-tty",
                "--",
                "sh",
                "-lc",
                "echo '$HOME; safe'"
            ]
            .map(OsString::from)
        );
    }

    #[test]
    fn codex_chatgpt_tokens_use_provider_env_names() {
        let auth = json!({"auth_mode":"chatgpt","tokens":{
            "access_token":"access", "refresh_token":"refresh", "account_id":"account", "id_token":"id"
        }});
        let tokens = parse_codex_tokens(&auth).unwrap();
        assert_eq!(tokens.len(), 4);
        assert_eq!(tokens[0], ("CODEX_AUTH_ACCESS_TOKEN", "access".into()));
        assert_eq!(tokens[1], ("CODEX_AUTH_REFRESH_TOKEN", "refresh".into()));
        assert_eq!(tokens[2], ("CODEX_AUTH_ACCOUNT_ID", "account".into()));
    }

    #[test]
    fn codex_bootstrap_copies_revision_scoped_placeholders() {
        let root = temporary("codex-auth-bootstrap");
        let auth = json!({
            "auth_mode": "chatgpt",
            "tokens": {
                "id_token": "header.payload.signature",
                "access_token": "generic",
                "refresh_token": "generic",
                "account_id": "generic"
            }
        });
        let output = Command::new("python3")
            .args([
                "-c",
                CODEX_AUTH_BOOTSTRAP_SCRIPT,
                root.to_str().unwrap(),
                &auth.to_string(),
            ])
            .env(
                "CODEX_AUTH_ACCESS_TOKEN",
                "openshell:resolve:env:access:revision",
            )
            .env(
                "CODEX_AUTH_REFRESH_TOKEN",
                "openshell:resolve:env:refresh:revision",
            )
            .env(
                "CODEX_AUTH_ACCOUNT_ID",
                "openshell:resolve:env:account:revision",
            )
            .output()
            .unwrap();
        assert!(output.status.success());
        let stored: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("auth.json")).unwrap()).unwrap();
        assert_eq!(
            stored["tokens"]["access_token"],
            "openshell:resolve:env:access:revision"
        );
        assert_eq!(
            stored["tokens"]["refresh_token"],
            "openshell:resolve:env:refresh:revision"
        );
        assert_eq!(
            stored["tokens"]["account_id"],
            "openshell:resolve:env:account:revision"
        );
        assert_eq!(stored["tokens"]["id_token"], "header.payload.signature");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn codex_provider_rejects_api_mode_and_missing_refresh() {
        assert!(parse_codex_tokens(&json!({"auth_mode":"apikey","tokens":{}})).is_err());
        assert!(parse_codex_tokens(&json!({"auth_mode":"chatgpt","tokens":{
            "access_token":"access", "account_id":"account"
        }}))
        .is_err());
    }
    #[test]
    fn sandbox_name_is_deterministic_and_scoped_to_runtime() {
        let root = Path::new("/workspace/project");
        assert_eq!(
            sandbox_name(root, "codex-headless", "base"),
            sandbox_name(root, "codex-headless", "base")
        );
        assert_ne!(
            sandbox_name(root, "codex-headless", "base"),
            sandbox_name(root, "claude-code-headless", "base")
        );
    }

    #[test]
    fn upload_selection_excludes_private_and_build_files() {
        let root = temporary("selection");
        fs::create_dir(root.join(".nexus")).unwrap();
        fs::create_dir(root.join("target")).unwrap();
        fs::write(root.join("README.md"), "safe").unwrap();
        fs::write(root.join(".env"), "SECRET=private").unwrap();
        fs::write(root.join(".nexus/session.json"), "private").unwrap();
        fs::write(root.join("target/output"), "build").unwrap();
        assert!(Command::new("git")
            .arg("init")
            .arg(&root)
            .output()
            .unwrap()
            .status
            .success());
        let names = selected_paths(&root).unwrap();
        assert_eq!(names, vec![b"README.md".to_vec()]);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn multi_repo_workspace_respects_each_ignore_file_and_excludes_root_files() {
        let root = temporary("multi-repository-selection");
        fs::write(root.join("root-secret.txt"), "private").unwrap();
        for name in ["api", "web"] {
            let child = root.join(name);
            fs::create_dir(&child).unwrap();
            assert!(Command::new("git")
                .arg("init")
                .arg(&child)
                .output()
                .unwrap()
                .status
                .success());
            fs::write(child.join("README.md"), name).unwrap();
            fs::write(child.join(".gitignore"), "ignored.txt\n").unwrap();
            fs::write(child.join("ignored.txt"), "private").unwrap();
            fs::write(child.join(".env"), "private").unwrap();
        }
        assert!(check_repository(&root).is_ok());
        let names = selected_paths(&root).unwrap();
        assert_eq!(
            names,
            vec![
                b"api/.gitignore".to_vec(),
                b"api/README.md".to_vec(),
                b"web/.gitignore".to_vec(),
                b"web/README.md".to_vec(),
            ]
        );
        let staged = temporary("multi-repository-staging");
        fs::create_dir(staged.join("api")).unwrap();
        fs::write(staged.join("api/new.txt"), "new").unwrap();
        fs::write(staged.join("root-file.txt"), "must not sync").unwrap();
        let allowed = child_repositories(&root)
            .unwrap()
            .into_iter()
            .map(|(name, _)| name)
            .collect();
        assert!(sync_files(
            &staged,
            &staged,
            &root,
            &HostSnapshot(BTreeMap::new()),
            Some(&allowed)
        )
        .is_err());
        assert!(!root.join("api/new.txt").exists());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(staged).unwrap();
    }

    #[test]
    fn synchronization_refuses_to_overwrite_concurrent_host_edit() {
        let host = temporary("host-edit");
        let staged = temporary("staged-edit");
        fs::write(host.join("file.txt"), "before").unwrap();
        fs::write(staged.join("file.txt"), "agent").unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert(
            PathBuf::from("file.txt"),
            digest(&host.join("file.txt")).unwrap(),
        );
        fs::write(host.join("file.txt"), "concurrent").unwrap();
        assert!(sync_files(&staged, &staged, &host, &HostSnapshot(baseline), None).is_err());
        assert_eq!(
            fs::read_to_string(host.join("file.txt")).unwrap(),
            "concurrent"
        );
        fs::remove_dir_all(host).unwrap();
        fs::remove_dir_all(staged).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn synchronization_refuses_host_symlink_ancestor() {
        let host = temporary("host-symlink");
        let outside = temporary("outside-symlink");
        let staged = temporary("staged-symlink");
        fs::create_dir(staged.join("repo")).unwrap();
        fs::write(staged.join("repo/new.txt"), "agent").unwrap();
        std::os::unix::fs::symlink(&outside, host.join("repo")).unwrap();
        assert!(sync_files(
            &staged,
            &staged,
            &host,
            &HostSnapshot(BTreeMap::new()),
            None
        )
        .is_err());
        assert!(!outside.join("new.txt").exists());
        fs::remove_dir_all(host).unwrap();
        fs::remove_dir_all(outside).unwrap();
        fs::remove_dir_all(staged).unwrap();
    }

    #[test]
    fn synchronization_propagates_deletions_from_sandbox() {
        let host = temporary("host-delete");
        let staged = temporary("staged-delete");
        fs::write(host.join("file.txt"), "before").unwrap();
        let mut baseline = BTreeMap::new();
        baseline.insert(
            PathBuf::from("file.txt"),
            digest(&host.join("file.txt")).unwrap(),
        );
        assert_eq!(
            remove_deleted_files(&staged, &host, &HostSnapshot(baseline)).unwrap(),
            1
        );
        assert!(!host.join("file.txt").exists());
        fs::remove_dir_all(host).unwrap();
        fs::remove_dir_all(staged).unwrap();
    }

    #[test]
    fn synchronization_refuses_bulk_deletion_without_removing_files() {
        let host = temporary("host-bulk-delete");
        let staged = temporary("staged-bulk-delete");
        let mut baseline = BTreeMap::new();
        for index in 0..101 {
            let relative = PathBuf::from(format!("file-{index}.txt"));
            fs::write(host.join(&relative), "before").unwrap();
            baseline.insert(
                relative,
                digest(&host.join(format!("file-{index}.txt"))).unwrap(),
            );
        }
        assert!(remove_deleted_files(&staged, &host, &HostSnapshot(baseline)).is_err());
        assert!(host.join("file-0.txt").exists());
        assert!(host.join("file-100.txt").exists());
        fs::remove_dir_all(host).unwrap();
        fs::remove_dir_all(staged).unwrap();
    }
}
