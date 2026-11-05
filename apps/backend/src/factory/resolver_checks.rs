//! Mandatory pre-PR verification for the legacy issue resolver. Discovery uses
//! BOTH the pinned base and candidate manifests: deleting a script cannot skip it.
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};
use tokio::process::Command;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub commands: Vec<Vec<String>>,
    pub roles: BTreeMap<String, Vec<Vec<String>>>,
    pub not_applicable: BTreeMap<String, String>,
}
fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}
fn npm_command(manager: &str, directory: &str, tail: &[&str]) -> Vec<String> {
    let mut command = vec![manager.to_string()];
    if directory != "." && !directory.is_empty() {
        command.extend(argv(&[
            match manager {
                "npm" => "--prefix",
                "pnpm" => "--dir",
                _ => "--cwd",
            },
            directory,
        ]));
    }
    command.extend(argv(tail));
    command
}
pub fn from_manifests(
    manifests: &BTreeMap<String, Vec<Value>>,
    files: &BTreeSet<String>,
    config: &Value,
) -> Result<Plan> {
    let mut roles: BTreeMap<String, Vec<Vec<String>>> = BTreeMap::new();
    let mut setup = Vec::new();
    for (path, versions) in manifests {
        if versions.len() > 1 {
            let original = &versions[0]["scripts"];
            let candidate = &versions[versions.len() - 1]["scripts"];
            if let Some(scripts) = original.as_object() {
                for (name, body) in scripts {
                    let role = match name.as_str() {
                        "test" | "test:ci" | "test:unit" | "test:integration" | "test:e2e"
                        | "test:smoke" => Some("tests"),
                        "build" | "build:prod" => Some("build"),
                        "lint" | "lint:ci" => Some("lint"),
                        "typecheck" | "type-check" | "check:types" | "types:check" | "check" => {
                            Some("typecheck")
                        }
                        _ => None,
                    };
                    if let Some(role) = role {
                        if candidate.get(name).is_some_and(|new| new != body)
                            && config
                                .pointer(&format!("/verification_plan/{role}"))
                                .is_none()
                        {
                            bail!("resolver_verification_script_changed_requires_explicit_plan")
                        }
                    }
                }
            }
        }
        let directory = Path::new(path)
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .and_then(|p| p.to_str())
            .unwrap_or(".");
        let mut manager = "npm";
        for version in versions {
            if let Some(spec) = version["packageManager"].as_str() {
                manager = if spec.starts_with("pnpm@") {
                    "pnpm"
                } else if spec.starts_with("yarn@") {
                    "yarn"
                } else if spec.starts_with("bun@") {
                    "bun"
                } else {
                    "npm"
                };
            }
        }
        let mut ancestor = Path::new(directory);
        loop {
            let prefix = if ancestor == Path::new(".") || ancestor.as_os_str().is_empty() {
                String::new()
            } else {
                format!("{}/", ancestor.to_string_lossy())
            };
            let lock = if files.contains(&format!("{prefix}pnpm-lock.yaml")) {
                Some(("pnpm", "pnpm-lock.yaml"))
            } else if files.contains(&format!("{prefix}yarn.lock")) {
                Some(("yarn", "yarn.lock"))
            } else if files.contains(&format!("{prefix}bun.lock"))
                || files.contains(&format!("{prefix}bun.lockb"))
            {
                Some(("bun", "bun.lock"))
            } else if files.contains(&format!("{prefix}package-lock.json")) {
                Some(("npm", "package-lock.json"))
            } else {
                None
            };
            if let Some((selected, _)) = lock {
                manager = selected;
                let dir = if prefix.is_empty() {
                    "."
                } else {
                    prefix.trim_end_matches('/')
                };
                let command = match manager {
                    "pnpm" => npm_command(manager, dir, &["install", "--frozen-lockfile"]),
                    "yarn" => npm_command(manager, dir, &["install", "--frozen-lockfile"]),
                    "bun" => npm_command(manager, dir, &["install", "--frozen-lockfile"]),
                    _ => npm_command("npm", dir, &["ci", "--no-audit", "--no-fund"]),
                };
                if !setup.contains(&command) {
                    setup.push(command)
                }
                break;
            }
            match ancestor.parent() {
                Some(parent) if parent != ancestor => ancestor = parent,
                _ => break,
            }
        }
        let mut scripts = BTreeSet::new();
        for version in versions {
            if let Some(map) = version["scripts"].as_object() {
                scripts.extend(map.keys().cloned());
            }
        }
        let mut add = |role: &str, name: &str| {
            let cmd = npm_command(manager, directory, &["run", name]);
            let list = roles.entry(role.into()).or_default();
            if !list.contains(&cmd) {
                list.push(cmd)
            }
        };
        for name in [
            "test:ci",
            "test",
            "test:unit",
            "test:integration",
            "test:e2e",
            "test:smoke",
        ] {
            if scripts.contains(name) {
                add("tests", name);
            }
        }
        for (role, candidates) in [
            ("build", vec!["build", "build:prod"]),
            ("lint", vec!["lint:ci", "lint"]),
            (
                "typecheck",
                vec![
                    "typecheck",
                    "type-check",
                    "check:types",
                    "types:check",
                    "check",
                ],
            ),
        ] {
            if let Some(name) = candidates.iter().find(|name| scripts.contains(**name)) {
                add(role, name);
            }
        }
    }
    // Cargo workspaces are checked once at their root. Nested independent crates
    // receive a manifest-path plan too; no silently unbuilt changed backend.
    for file in files.iter().filter(|f| f.ends_with("Cargo.toml")) {
        if file != "Cargo.toml" && files.contains("Cargo.toml") {
            continue;
        }
        let manifest = if file == "Cargo.toml" {
            vec![]
        } else {
            argv(&["--manifest-path", file])
        };
        for (role, mut command) in [
            (
                "tests",
                argv(&["cargo", "test", "--all-targets", "--workspace", "--locked"]),
            ),
            (
                "build",
                argv(&["cargo", "build", "--all-targets", "--workspace", "--locked"]),
            ),
            (
                "lint",
                argv(&[
                    "cargo",
                    "clippy",
                    "--all-targets",
                    "--workspace",
                    "--locked",
                ]),
            ),
        ] {
            let lock = Path::new(file)
                .with_file_name("Cargo.lock")
                .to_string_lossy()
                .into_owned();
            if !files.contains(&lock) {
                command.retain(|arg| arg != "--locked");
            }
            command.extend(manifest.clone());
            if role == "lint" {
                command.extend(argv(&["--", "-D", "warnings"]))
            }
            roles.entry(role.into()).or_default().push(command);
        }
        let mut format = argv(&["cargo", "fmt", "--all"]);
        format.extend(manifest);
        format.extend(argv(&["--", "--check"]));
        roles.entry("format".into()).or_default().push(format);
    }
    for role in ["setup", "tests", "build", "lint", "typecheck", "format"] {
        if let Some(value) = config.pointer(&format!("/verification_plan/{role}")) {
            let commands = super::verification::parse_verification_commands(Some(value))?;
            if role == "setup" {
                setup = commands
            } else if commands.is_empty() {
                bail!("resolver_required_checks_empty")
            } else {
                roles.insert(role.into(), commands);
            }
        }
    }
    for command in
        super::verification::parse_verification_commands(config.get("verification_commands"))?
    {
        let role = if command
            .iter()
            .any(|s| s == "test" || s == "pytest" || s.starts_with("test:"))
        {
            "tests"
        } else if command
            .iter()
            .any(|s| s == "build" || s.starts_with("build:"))
        {
            "build"
        } else {
            "additional"
        };
        let list = roles.entry(role.into()).or_default();
        if !list.contains(&command) {
            list.push(command)
        }
    }
    let mut not_applicable = BTreeMap::new();
    for role in ["tests", "build"] {
        if !roles.get(role).is_some_and(|c| !c.is_empty()) {
            let reason = config
                .pointer(&format!("/verification_not_applicable/{role}"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| s.len() >= 8 && s.len() <= 1024)
                .ok_or_else(|| anyhow::anyhow!(format!("resolver_{role}_required")))?;
            not_applicable.insert(role.into(), reason.into());
        }
    }
    let mut commands = setup;
    for role in [
        "tests",
        "build",
        "lint",
        "typecheck",
        "format",
        "additional",
    ] {
        if let Some(list) = roles.get(role) {
            for command in list {
                if !commands.contains(command) {
                    commands.push(command.clone());
                }
            }
        }
    }
    if commands.len() > 32 {
        bail!("resolver_verification_plan_too_large_configure_aggregate_commands")
    }
    Ok(Plan {
        commands,
        roles,
        not_applicable,
    })
}
async fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        Command::new("git")
            .current_dir(root)
            .args(args)
            .kill_on_drop(true)
            .output(),
    )
    .await??;
    if !out.status.success() {
        bail!("verification_manifest_read_failed")
    }
    Ok(out.stdout)
}
pub async fn discover(root: &Path, base: &str, config: &Value) -> Result<Plan> {
    if !super::workflows::valid_sha(base) {
        bail!("invalid_base_sha")
    }
    let base_files = git(root, &["ls-tree", "-r", "--name-only", "-z", base]).await?;
    let current = git(
        root,
        &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ],
    )
    .await?;
    let mut files = BTreeSet::new();
    for path in base_files
        .split(|b| *b == 0)
        .chain(current.split(|b| *b == 0))
        .filter(|p| !p.is_empty())
    {
        let path = std::str::from_utf8(path)?;
        if !super::workflows::relative_path(path) {
            bail!("unsafe_verification_path")
        }
        if !path
            .split('/')
            .any(|p| ["node_modules", "target", ".git", ".worktrees", "vendor"].contains(&p))
        {
            files.insert(path.to_string());
        }
        if files.len() > 100000 {
            bail!("verification_manifest_limit")
        }
    }
    let paths = files
        .iter()
        .filter(|p| p.as_str() == "package.json" || p.ends_with("/package.json"))
        .collect::<Vec<_>>();
    if paths.len() > 64 {
        bail!("verification_manifest_limit")
    }
    let canonical = tokio::fs::canonicalize(root).await?;
    let mut manifests = BTreeMap::new();
    for path in paths {
        let mut versions = Vec::new();
        if let Ok(bytes) = git(root, &["show", &format!("{base}:{path}")]).await {
            if bytes.len() > 1024 * 1024 {
                bail!("verification_manifest_limit")
            }
            versions.push(serde_json::from_slice(&bytes)?);
        }
        let file = root.join(path);
        if tokio::fs::try_exists(&file).await? {
            let resolved = tokio::fs::canonicalize(&file).await?;
            if !resolved.starts_with(&canonical)
                || tokio::fs::symlink_metadata(&file)
                    .await?
                    .file_type()
                    .is_symlink()
            {
                bail!("unsafe_verification_manifest")
            }
            let bytes = tokio::fs::read(file).await?;
            if bytes.len() > 1024 * 1024 {
                bail!("verification_manifest_limit")
            }
            versions.push(serde_json::from_slice(&bytes)?);
        }
        if !versions.is_empty() {
            manifests.insert(path.clone(), versions);
        }
    }
    from_manifests(&manifests, &files, config)
}

/// Hash every committed file inside the commands pod after checks, without
/// exposing its host .git metadata. Auto-formatting cannot verify another tree.
pub const INTEGRITY_SCRIPT: &str = r#"import hashlib,json,os,sys,stat
root=sys.argv[2]
raw=open(sys.argv[1],'rb').read()
assert hashlib.sha256(raw).hexdigest()==sys.argv[3], 'verification manifest was modified'
for item in json.loads(raw):
 p=os.path.join(root,item['path']);m=os.lstat(p)
 if item['mode']=='120000': data=os.readlink(p).encode()
 else:
  assert stat.S_ISREG(m.st_mode) and not stat.S_ISLNK(m.st_mode), 'verified source type changed'
  assert m.st_size<=67108864, 'verified source size changed'
  data=open(p,'rb').read()
  assert bool(m.st_mode & 0o111)==(item['mode']=='100755'), 'verified executable mode changed'
 actual=hashlib.sha256(data).hexdigest() if 'sha256' in item else hashlib.sha1(b'blob '+str(len(data)).encode()+b'\0'+data).hexdigest()
 assert actual==item.get('sha256',item.get('sha')), 'verification mutated committed source: '+item['path']
"#;
pub async fn integrity_manifest(root: &Path) -> Result<Vec<u8>> {
    let raw = git(root, &["ls-tree", "-rz", "HEAD"]).await?;
    let mut entries = Vec::new();
    for record in raw.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let text = std::str::from_utf8(record)?;
        let (metadata, path) = text
            .split_once('\t')
            .ok_or_else(|| anyhow::anyhow!("invalid_tree_record"))?;
        let fields = metadata.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 3 || fields[1] != "blob" || !super::workflows::relative_path(path) {
            bail!("unsupported_verified_tree")
        }
        entries.push(json!({"path":path,"mode":fields[0],"sha":fields[2]}));
    }
    Ok(serde_json::to_vec(&entries)?)
}
/// The credential broker may rewrite registry URLs in lockfiles/.npmrc. Bind
/// those exact trusted substitutions, then bind the manifest itself in immutable
/// exec argv so repository code cannot rewrite the guard's expected hashes.
pub fn bind_proxy_integrity(
    files: &mut [(String, Vec<u8>)],
    commands: &mut [Vec<String>],
) -> Result<()> {
    use sha2::{Digest, Sha256};
    let Some(index) = files
        .iter()
        .position(|(path, _)| path == "/tmp/nexusmind-verified-tree.json")
    else {
        return Ok(());
    };
    let mut entries: Vec<Value> = serde_json::from_slice(&files[index].1)?;
    let prefix = format!("{}/", super::sandbox::WORKSPACE);
    for (path, bytes) in files.iter() {
        if let Some(relative) = path.strip_prefix(&prefix) {
            if let Some(entry) = entries.iter_mut().find(|e| e["path"] == relative) {
                entry["sha256"] = json!(hex::encode(Sha256::digest(bytes)));
            }
        }
    }
    files[index].1 = serde_json::to_vec(&entries)?;
    let digest = hex::encode(Sha256::digest(&files[index].1));
    for command in commands {
        if command.get(3).is_some_and(|s| s == INTEGRITY_SCRIPT) {
            if command.len() != 7 {
                bail!("invalid_integrity_command")
            };
            command[6] = digest.clone();
        }
    }
    Ok(())
}
pub fn annotate_receipts(mut receipts: Vec<Value>, plan: &Plan, head: &str) -> Vec<Value> {
    for (role, reason) in &plan.not_applicable {
        receipts.push(json!({"check":role,"status":"not_applicable","not_applicable_reason":reason,"head_sha":head}));
    }
    receipts
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn committed_manifest_removal_and_source_mutation_cannot_bypass_checks() {
        use sha2::{Digest, Sha256};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        async fn run(root: &Path, args: &[&str]) {
            assert!(Command::new("git")
                .current_dir(root)
                .args(args)
                .output()
                .await
                .unwrap()
                .status
                .success());
        }
        run(root, &["init", "-q"]).await;
        tokio::fs::write(root.join("package.json"),r#"{"scripts":{"test":"vitest run","build":"tsc","lint":"eslint .","typecheck":"tsc --noEmit"}}"#).await.unwrap();
        tokio::fs::write(root.join("package-lock.json"), "{}")
            .await
            .unwrap();
        run(root, &["add", "."]).await;
        run(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=a@test.com",
                "commit",
                "-qm",
                "base",
            ],
        )
        .await;
        let base = String::from_utf8(git(root, &["rev-parse", "HEAD"]).await.unwrap())
            .unwrap()
            .trim()
            .to_string();
        tokio::fs::write(root.join("package.json"), "{\"scripts\":{}}")
            .await
            .unwrap();
        run(root, &["add", "."]).await;
        run(
            root,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=a@test.com",
                "commit",
                "-qm",
                "candidate",
            ],
        )
        .await;
        let plan = discover(root, &base, &json!({})).await.unwrap();
        assert!(plan.roles.contains_key("tests"));
        assert!(plan.roles.contains_key("build"));
        let manifest = integrity_manifest(root).await.unwrap();
        let digest = hex::encode(Sha256::digest(&manifest));
        let path = dir.path().join("integrity.json");
        tokio::fs::write(&path, &manifest).await.unwrap();
        let check = || {
            let mut c = Command::new("python3");
            c.args([
                "-I",
                "-c",
                INTEGRITY_SCRIPT,
                path.to_str().unwrap(),
                root.to_str().unwrap(),
                &digest,
            ]);
            c
        };
        assert!(check().output().await.unwrap().status.success());
        tokio::fs::write(
            root.join("package.json"),
            "{\"scripts\":{\"test\":\"echo fake\"}}",
        )
        .await
        .unwrap();
        assert!(!check().output().await.unwrap().status.success());
        tokio::fs::write(&path, b"[]").await.unwrap();
        assert!(
            !check().output().await.unwrap().status.success(),
            "tampering with expected hashes must fail"
        );
    }
    #[test]
    fn script_removal_cannot_skip_tests_or_build() {
        let m = BTreeMap::from([(
            "package.json".into(),
            vec![
                json!({"scripts":{"test":"vitest run","build":"tsc","lint":"eslint .","typecheck":"tsc --noEmit"}}),
                json!({"scripts":{}}),
            ],
        )]);
        let p = from_manifests(
            &m,
            &BTreeSet::from(["package-lock.json".into()]),
            &json!({}),
        )
        .unwrap();
        assert!(p.roles.contains_key("tests"));
        assert!(p.roles.contains_key("build"));
        assert!(p.roles.contains_key("lint"));
        assert!(p.roles.contains_key("typecheck"));
        assert_eq!(p.commands[0][1], "ci");
    }
    #[test]
    fn cannot_replace_real_tests_with_an_echo() {
        let m = BTreeMap::from([(
            "package.json".into(),
            vec![
                json!({"scripts":{"test":"vitest run","build":"tsc"}}),
                json!({"scripts":{"test":"echo all good","build":"tsc"}}),
            ],
        )]);
        assert!(from_manifests(&m, &BTreeSet::new(), &json!({})).is_err());
    }
    #[test]
    fn empty_or_opaque_config_fails_closed() {
        assert!(from_manifests(&BTreeMap::new(), &BTreeSet::new(), &json!({})).is_err());
        assert!(from_manifests(
            &BTreeMap::new(),
            &BTreeSet::new(),
            &json!({"verification_commands":[["npm","run","ci"]]})
        )
        .is_err());
    }
    #[test]
    fn explicit_platform_plan_and_justified_na_are_supported() {
        let p = from_manifests(
            &BTreeMap::new(),
            &BTreeSet::new(),
            &json!({"verification_plan":{"tests":[["cargo","test"]],"build":[["cargo","build"]]}}),
        )
        .unwrap();
        assert_eq!(p.commands.len(), 2);
        assert!(from_manifests(&BTreeMap::new(),&BTreeSet::new(),&json!({"verification_not_applicable":{"tests":"documentation-only repository","build":"no executable artifacts"}})).is_ok());
    }
}
