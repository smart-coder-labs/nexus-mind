//! Task pods (factory F1, design §3): the hardened, ephemeral pod each sandboxed
//! task runs in. Pure: it builds the manifest and the environment; creating,
//! driving and deleting the pod is the executor's job.
//!
//! The pod holds no credential. Its only secret is the run token, embedded in the
//! proxy URLs; every real credential is injected by the egress proxy.

use serde_json::{json, Value};

pub const SANDBOX_NAMESPACE: &str = "nexusmind-sandbox";
/// Service DNS name and port of the egress proxy inside the cluster.
pub const PROXY_AUTHORITY: &str = "factory-egress-proxy.nexusmind-sandbox.svc.cluster.local:8080";
/// Non-root UID the task runs as (mapped to an unprivileged host UID by user namespaces).
pub const TASK_UID: i64 = 10001;
/// Workspace mount inside the pod.
pub const WORKSPACE: &str = "/workspace";
/// Label carrying the run id on every task pod.
pub const RUN_LABEL: &str = "factory.nexusmind/run";
/// Label separating parallel pods of one run (resolver fanout: one per issue).
pub const SLOT_LABEL: &str = "factory.nexusmind/slot";

#[derive(Clone, Debug)]
pub struct TaskPodRequest {
    pub org_id: String,
    pub run_id: String,
    /// Distinguishes the pods of one run (lease attempt, retry); part of the name.
    pub pod_suffix: String,
    pub profile: PodProfile,
    /// Parallel pods of one run each get their own slot (`main` otherwise).
    pub slot: String,
    pub image: String,
    /// Signed run token (see `egress::sign_run_token`).
    pub run_token: String,
    pub wall_time_secs: i64,
}

/// A DNS-1123 pod name derived from the run id: lowercase alphanumerics and `-`,
/// at most 63 characters, never starting or ending with `-`.
pub fn task_pod_name(run_id: &str) -> String {
    let mut slug = String::new();
    for c in run_id.chars().flat_map(char::to_lowercase) {
        let c = if c.is_ascii_alphanumeric() { c } else { '-' };
        // Collapse runs of separators.
        if !(c == '-' && slug.ends_with('-')) {
            slug.push(c);
        }
    }
    let slug = slug.trim_matches('-');
    let mut name = format!("task-{slug}");
    name.truncate(63);
    name.trim_end_matches('-').to_string()
}

/// A short, unique pod name: the run id's first segment for humans, then a hash
/// of the full run id and suffix, so nothing distinguishing is lost to the
/// 63-character limit (parallel issues, retries, commands pods).
fn unique_pod_name(run_id: &str, pod_suffix: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = hex::encode(Sha256::digest(format!("{run_id}/{pod_suffix}").as_bytes()));
    let prefix: String = run_id.chars().take(8).collect();
    task_pod_name(&format!("{prefix}-{}", &digest[..16]))
}

/// The pod's environment: proxy routes for Claude and NexusMind, a placeholder
/// Claude credential (the proxy replaces it), and no other secret.
pub fn task_env(request: &TaskPodRequest) -> Vec<(String, String)> {
    let token = &request.run_token;
    let proxy_host = PROXY_AUTHORITY.split(':').next().unwrap_or(PROXY_AUTHORITY);
    [
        ("HOME", "/home/task".to_string()),
        ("TMPDIR", "/tmp".to_string()),
        (
            "ANTHROPIC_BASE_URL",
            format!("http://{PROXY_AUTHORITY}/r/{token}/anthropic"),
        ),
        (
            "NEXUSMIND_BASE_URL",
            format!("http://{PROXY_AUTHORITY}/r/{token}/nexusmind"),
        ),
        // The CLI needs some credential to start; the proxy strips and replaces it.
        (
            "CLAUDE_CODE_OAUTH_TOKEN",
            "sandbox-placeholder-not-a-credential".to_string(),
        ),
        // Spike S1: without this the CLI opens side connections that bypass the base URL.
        ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1".to_string()),
        ("DISABLE_AUTOUPDATER", "1".to_string()),
        // Registries only (CONNECT allowlist). The proxy itself must not be proxied.
        (
            "HTTPS_PROXY",
            format!("http://run:{token}@{PROXY_AUTHORITY}"),
        ),
        ("NO_PROXY", format!("{proxy_host},localhost,127.0.0.1")),
        ("NEXUSMIND_MCP_TOOL_PROFILE", "only_context".to_string()),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value))
    .collect()
}

/// What a pod runs, which decides what its environment may hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PodProfile {
    /// The agent: proxy routes to Claude and NexusMind with the full run token.
    Agent,
    /// Repository code (tests, builds): only a registry-only token. Any process in
    /// a pod can read PID 1's environment, so untrusted code never shares a pod
    /// with a token that reaches a credentialed upstream.
    Commands,
}

/// Where an autonomous run executes. `isolation` in the agent config; absent means
/// `local` until the F1 drill passes, after which the default flips to `sandbox`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Isolation {
    /// In the worker container: unsafe, kept as an explicit escape hatch.
    Local,
    /// In an ephemeral task pod.
    Sandbox,
}

/// Templates migrated to the sandbox so far (design §4, migration order).
pub const SANDBOX_TEMPLATES: &[&str] = &[
    "github_pr_reviewer",
    "qa",
    "judge",
    "github_issue_resolver",
    "security_scan",
    "security_dast",
];

/// Parses and validates `isolation`. A sandbox request that cannot be honored is
/// an error, never a silent fallback to local execution.
pub fn autonomous_isolation(
    config: &Value,
    template_key: &str,
    executor: &str,
) -> anyhow::Result<Isolation> {
    resolve_isolation(config, template_key, executor, default_isolation())
}

/// The isolation of agents that do not set one: `FACTORY_ISOLATION_DEFAULT`
/// (`sandbox` once the cluster side is deployed and the drill passed), local
/// otherwise. A deploy never flips running agents before the sandbox exists.
pub fn default_isolation() -> Isolation {
    match std::env::var("FACTORY_ISOLATION_DEFAULT").as_deref() {
        Ok("sandbox") => Isolation::Sandbox,
        _ => Isolation::Local,
    }
}

/// `isolation` from the agent config, or `default` when unset. The default only
/// applies where a sandbox can run; an explicit sandbox request that cannot be
/// honored is an error, never a silent fallback to local execution.
pub fn resolve_isolation(
    config: &Value,
    template_key: &str,
    executor: &str,
    default: Isolation,
) -> anyhow::Result<Isolation> {
    let supported = executor == "claude" && SANDBOX_TEMPLATES.contains(&template_key);
    let isolation = match config.get("isolation") {
        None if supported => return Ok(default),
        None => return Ok(Isolation::Local),
        Some(Value::String(value)) if value == "local" => return Ok(Isolation::Local),
        Some(Value::String(value)) if value == "sandbox" => Isolation::Sandbox,
        _ => anyhow::bail!("invalid_isolation"),
    };
    if executor != "claude" {
        anyhow::bail!("sandbox_unsupported_executor")
    }
    if !SANDBOX_TEMPLATES.contains(&template_key) {
        anyhow::bail!("sandbox_unsupported_template")
    }
    Ok(isolation)
}

/// The complete environment of a verification command. Code under test gets a
/// registry-only token (`egress::registry_only_run_id`): it can install packages
/// but never reach Claude or NexusMind as the run.
pub fn verification_env(registry_token: &str) -> Vec<(String, String)> {
    let proxy_host = PROXY_AUTHORITY.split(':').next().unwrap_or(PROXY_AUTHORITY);
    [
        (
            "PATH",
            "/usr/local/cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin"
                .to_string(),
        ),
        ("HOME", "/tmp".to_string()),
        ("TMPDIR", "/tmp".to_string()),
        ("CI", "1".to_string()),
        (
            "HTTPS_PROXY",
            format!("http://run:{registry_token}@{PROXY_AUTHORITY}"),
        ),
        ("NO_PROXY", format!("{proxy_host},localhost,127.0.0.1")),
    ]
    .into_iter()
    .map(|(name, value)| (name.to_string(), value))
    .collect()
}

/// Most hosts one run may tunnel to (the run token's limit).
pub const MAX_SANDBOX_HOSTS: usize = 16;

/// Hosts a sandboxed QA/judge run may reach: its enabled `web_application`
/// targets, the run's preview (`app_base_url`) and the agent's extra list
/// (`sandbox_allowed_hosts`: CDNs, SSO, payment providers). The proxy tunnels only
/// HTTPS on 443 and still refuses any name resolving to a non-public address.
pub fn sandbox_hosts(config: &Value) -> anyhow::Result<Vec<String>> {
    fn https_host(url: &str) -> Option<String> {
        reqwest::Url::parse(url)
            .ok()
            .filter(|url| url.scheme() == "https" && url.port_or_known_default() == Some(443))
            .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
            .filter(|host| super::egress::valid_host(host))
    }
    let mut hosts: Vec<String> = Vec::new();
    fn add(hosts: &mut Vec<String>, host: String) {
        if !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    let targets = config
        .get("targets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|target| target.get("kind").and_then(Value::as_str) == Some("web_application"))
        .filter(|target| target.get("enabled").and_then(Value::as_bool) == Some(true));
    for target in targets {
        // A target the proxy cannot reach is skipped: another one may be the one
        // this run needs. A run with nothing reachable fails below.
        if let Some(host) = target
            .pointer("/config/url")
            .and_then(Value::as_str)
            .and_then(https_host)
        {
            add(&mut hosts, host);
        }
    }
    if let Some(url) = config.get("app_base_url").and_then(Value::as_str) {
        let host = https_host(url).ok_or_else(|| anyhow::anyhow!("target_not_sandboxable"))?;
        add(&mut hosts, host);
    }
    if hosts.is_empty() {
        anyhow::bail!("target_not_sandboxable")
    }
    for host in parse_allowed_hosts(config.get("sandbox_allowed_hosts"))? {
        add(&mut hosts, host);
    }
    if hosts.len() > MAX_SANDBOX_HOSTS {
        anyhow::bail!("too_many_sandbox_hosts")
    }
    Ok(hosts)
}

/// `sandbox_allowed_hosts`: lowercase DNS names, no wildcards or IP literals.
pub fn parse_allowed_hosts(value: Option<&Value>) -> anyhow::Result<Vec<String>> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(Vec::new());
    };
    value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_sandbox_allowed_hosts"))?
        .iter()
        .map(|host| {
            host.as_str()
                .map(str::to_ascii_lowercase)
                .filter(|host| super::egress::valid_host(host))
                .ok_or_else(|| anyhow::anyhow!("invalid_sandbox_allowed_hosts"))
        })
        .collect::<anyhow::Result<Vec<String>>>()
        .and_then(|hosts| {
            // Checked again with the targets at run time; refuse the obvious case on save.
            if hosts.len() > MAX_SANDBOX_HOSTS {
                anyhow::bail!("too_many_sandbox_hosts")
            }
            Ok(hosts)
        })
}

/// Where the Playwright MCP writes screenshots inside the pod.
pub const QA_OUTPUT_DIR: &str = "/tmp/qa-output";
/// Playwright config written into the pod: it carries the proxy credential, so it
/// lives in a file, never in the exec request.
pub const PLAYWRIGHT_CONFIG_PATH: &str = "/tmp/playwright-config.json";

/// The `--mcp-config` value (no secret) and the Playwright config file content
/// (the browser's proxy credential) for a sandboxed QA/judge run.
pub fn playwright_mcp(proxy_token: &str) -> (String, Vec<u8>) {
    let mcp = json!({"mcpServers": {"playwright": {
        "command": "mcp-server-playwright",
        "args": [
            "--headless", "--isolated", "--no-sandbox", "--browser", "chromium",
            "--output-dir", QA_OUTPUT_DIR,
            "--config", PLAYWRIGHT_CONFIG_PATH
        ]
    }}});
    // Playwright answers the proxy's 407 challenge with these credentials.
    let file = json!({"browser": {"launchOptions": {"proxy": {
        "server": format!("http://{PROXY_AUTHORITY}"),
        "username": "run",
        "password": proxy_token
    }}}});
    (mcp.to_string(), file.to_string().into_bytes())
}

/// The `--mcp-config` value for NexusMind in an agent pod. It holds no secret:
/// the server inherits the pod's `NEXUSMIND_BASE_URL` (the proxy route) and sends
/// a placeholder key that the proxy replaces with the org's bot key.
pub fn nexusmind_mcp() -> String {
    json!({"mcpServers": {"plugin_nexusmind_nexusmind": {
        "command": "nexusmind-mcp",
        "env": {
            "NEXUSMIND_API_KEY": "sandbox-placeholder-not-a-credential",
            "NEXUSMIND_MCP_TOOL_PROFILE": "only_context"
        }
    }}})
    .to_string()
}

/// A file name the pod may hand back: a plain screenshot/trace name, never a path.
pub fn valid_artifact_name(name: &str) -> bool {
    let Some((stem, extension)) = name.rsplit_once('.') else {
        return false;
    };
    (1..=128).contains(&name.len())
        && !stem.is_empty()
        && matches!(extension, "png" | "jpg" | "jpeg" | "webp")
        && stem
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
        && !stem.starts_with('.')
}

pub fn task_pod_manifest(request: &TaskPodRequest) -> Value {
    let env = match request.profile {
        PodProfile::Agent => task_env(request),
        PodProfile::Commands => verification_env(&request.run_token),
    };
    let env: Vec<Value> = env
        .into_iter()
        .map(|(name, value)| json!({"name": name, "value": value}))
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": unique_pod_name(&request.run_id, &request.pod_suffix),
            "namespace": SANDBOX_NAMESPACE,
            "labels": {
                "role": "task",
                "app": "factory-task",
                // Lets a retried run delete the orphans of its earlier attempts.
                (RUN_LABEL): request.run_id,
                (SLOT_LABEL): request.slot
            },
            "annotations": {
                "factory.nexusmind/run-id": request.run_id,
                "factory.nexusmind/org-id": request.org_id
            }
        },
        "spec": {
            "automountServiceAccountToken": false,
            // Same private registry pull secret as the backend (copied into the namespace).
            "imagePullSecrets": [{"name": "ghcr-pull"}],
            // User namespaces: in-pod UIDs map to unprivileged host UIDs (spike S2).
            "hostUsers": false,
            "restartPolicy": "Never",
            "activeDeadlineSeconds": request.wall_time_secs,
            "enableServiceLinks": false,
            "securityContext": {
                "runAsNonRoot": true,
                "runAsUser": TASK_UID,
                "runAsGroup": TASK_UID,
                "fsGroup": TASK_UID,
                "seccompProfile": {"type": "RuntimeDefault"}
            },
            "containers": [{
                "name": "task",
                "image": request.image,
                // The executor drives the pod with exec; the container just stays up.
                "command": ["sleep", "infinity"],
                "workingDir": WORKSPACE,
                "env": env,
                "securityContext": {
                    "allowPrivilegeEscalation": false,
                    "readOnlyRootFilesystem": true,
                    "capabilities": {"drop": ["ALL"]}
                },
                "resources": {
                    "requests": {"cpu": "250m", "memory": "512Mi"},
                    "limits": {"cpu": "2", "memory": "4Gi", "ephemeral-storage": "8Gi"}
                },
                "volumeMounts": [
                    {"name": "workspace", "mountPath": WORKSPACE},
                    {"name": "home", "mountPath": "/home/task"},
                    {"name": "tmp", "mountPath": "/tmp"}
                ]
            }],
            "volumes": [
                {"name": "workspace", "emptyDir": {"sizeLimit": "6Gi"}},
                {"name": "home", "emptyDir": {"sizeLimit": "1Gi"}},
                {"name": "tmp", "emptyDir": {"sizeLimit": "2Gi"}}
            ]
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> TaskPodRequest {
        TaskPodRequest {
            org_id: "org-1".into(),
            run_id: "0F9B7C2E-Run_42".into(),
            pod_suffix: "a1b2c3d4-0".into(),
            image: "ghcr.io/acme/sandbox@sha256:abc".into(),
            run_token: "v2.org-1.run.1.sig".into(),
            profile: PodProfile::Agent,
            slot: "main".into(),
            wall_time_secs: 1800,
        }
    }

    #[test]
    fn isolation_defaults_to_local_and_sandbox_never_falls_back() {
        let reviewer = "github_pr_reviewer";
        assert_eq!(
            resolve_isolation(&json!({}), reviewer, "claude", Isolation::Local).unwrap(),
            Isolation::Local
        );
        assert_eq!(
            resolve_isolation(
                &json!({"isolation": "local"}),
                "qa",
                "claude",
                Isolation::Local
            )
            .unwrap(),
            Isolation::Local
        );
        assert_eq!(
            resolve_isolation(
                &json!({"isolation": "sandbox"}),
                reviewer,
                "claude",
                Isolation::Local
            )
            .unwrap(),
            Isolation::Sandbox
        );
        for (config, template, executor, code) in [
            (
                json!({"isolation": "sandbox"}),
                "lead_generation",
                "claude",
                "sandbox_unsupported_template",
            ),
            (
                json!({"isolation": "sandbox"}),
                reviewer,
                "nexus",
                "sandbox_unsupported_executor",
            ),
            (
                json!({"isolation": "docker"}),
                reviewer,
                "claude",
                "invalid_isolation",
            ),
            (
                json!({"isolation": true}),
                reviewer,
                "claude",
                "invalid_isolation",
            ),
        ] {
            assert_eq!(
                resolve_isolation(&config, template, executor, Isolation::Local)
                    .unwrap_err()
                    .to_string(),
                code,
                "{config} {template} {executor}"
            );
        }
    }

    #[test]
    fn a_commands_pod_holds_only_the_registry_token() {
        let mut request = request();
        request.profile = PodProfile::Commands;
        request.run_token = "v2.org-1.run-registry.1.sig".into();
        let pod = task_pod_manifest(&request);
        let env: std::collections::HashMap<String, String> = pod["spec"]["containers"][0]["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["name"].as_str().unwrap().into(),
                    e["value"].as_str().unwrap().into(),
                )
            })
            .collect();
        assert_eq!(
            env,
            verification_env("v2.org-1.run-registry.1.sig")
                .into_iter()
                .collect()
        );
        let serialized = pod.to_string();
        assert!(
            !serialized.contains("/anthropic") && !serialized.contains("/nexusmind"),
            "{serialized}"
        );
    }

    #[test]
    fn verification_gets_only_the_registry_token() {
        let env: std::collections::HashMap<String, String> =
            verification_env("v2.org-1.run-registry.1.sig")
                .into_iter()
                .collect();
        assert_eq!(
            env["HTTPS_PROXY"],
            format!("http://run:v2.org-1.run-registry.1.sig@{PROXY_AUTHORITY}")
        );
        for inherited in [
            "ANTHROPIC_BASE_URL",
            "NEXUSMIND_BASE_URL",
            "CLAUDE_CODE_OAUTH_TOKEN",
        ] {
            assert!(!env.contains_key(inherited), "{inherited}");
        }
        assert_eq!(env["HOME"], "/tmp");
    }

    #[test]
    fn sandbox_hosts_come_from_targets_preview_and_extra_list() {
        let config = json!({
            "targets": [
                {"kind": "web_application", "enabled": true, "config": {"url": "https://App.Acme.test/login"}},
                // Unusable targets are skipped, not fatal.
                {"kind": "web_application", "enabled": true, "config": {"url": "http://legacy.acme.test"}},
                {"kind": "web_application", "enabled": true, "config": {"url": "https://acme.test:3000"}},
                {"kind": "web_application", "enabled": true, "config": {}},
                {"kind": "web_application", "enabled": false, "config": {"url": "https://off.acme.test"}},
                {"kind": "repository", "enabled": true, "config": {"repository": "a/b"}}
            ],
            "app_base_url": "https://pr-42.preview.acme.test/",
            "sandbox_allowed_hosts": ["cdn.acme.test", "app.acme.test", "auth.example.com"]
        });
        assert_eq!(
            sandbox_hosts(&config).unwrap(),
            [
                "app.acme.test",
                "pr-42.preview.acme.test",
                "cdn.acme.test",
                "auth.example.com"
            ]
        );
    }

    #[test]
    fn sandbox_hosts_fail_closed() {
        let target =
            |url: &str| json!({"kind": "web_application", "enabled": true, "config": {"url": url}});
        // Nothing reachable: the run cannot do its job in the sandbox.
        for config in [
            json!({}),
            json!({"targets": [target("http://app.acme.test")]}),
            json!({"targets": [target("https://10.0.0.5")]}),
        ] {
            assert_eq!(
                sandbox_hosts(&config).unwrap_err().to_string(),
                "target_not_sandboxable",
                "{config}"
            );
        }
        // An explicit preview that cannot be reached is an error, not a skip.
        for url in [
            "http://pr.acme.test",
            "https://localhost",
            "https://pr.acme.test:8443",
            "nope",
        ] {
            let config = json!({"targets": [target("https://app.acme.test")], "app_base_url": url});
            assert_eq!(
                sandbox_hosts(&config).unwrap_err().to_string(),
                "target_not_sandboxable",
                "{url}"
            );
        }
        for extra in [
            json!(["*.acme.test"]),
            json!(["10.0.0.1"]),
            json!("cdn.acme.test"),
            json!([1]),
        ] {
            let config = json!({"targets": [target("https://app.acme.test")], "sandbox_allowed_hosts": extra});
            assert_eq!(
                sandbox_hosts(&config).unwrap_err().to_string(),
                "invalid_sandbox_allowed_hosts",
                "{extra}"
            );
        }
        let many: Vec<String> = (0..16).map(|i| format!("h{i}.acme.test")).collect();
        let config =
            json!({"targets": [target("https://app.acme.test")], "sandbox_allowed_hosts": many});
        assert_eq!(
            sandbox_hosts(&config).unwrap_err().to_string(),
            "too_many_sandbox_hosts"
        );
    }

    #[test]
    fn the_playwright_credential_stays_in_the_file() {
        let (mcp, file) = playwright_mcp("v3.org.run.1.aG9zdA.sig");
        assert!(!mcp.contains("v3.org"), "{mcp}");
        let mcp: Value = serde_json::from_str(&mcp).unwrap();
        let args: Vec<&str> = mcp["mcpServers"]["playwright"]["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap())
            .collect();
        assert!(
            args.windows(2)
                .any(|w| w == ["--config", PLAYWRIGHT_CONFIG_PATH]),
            "{args:?}"
        );
        assert!(
            args.windows(2)
                .any(|w| w == ["--output-dir", QA_OUTPUT_DIR]),
            "{args:?}"
        );
        let file: Value = serde_json::from_slice(&file).unwrap();
        let proxy = &file["browser"]["launchOptions"]["proxy"];
        assert_eq!(proxy["server"], format!("http://{PROXY_AUTHORITY}"));
        assert_eq!(proxy["username"], "run");
        assert_eq!(proxy["password"], "v3.org.run.1.aG9zdA.sig");
    }

    #[test]
    fn the_nexusmind_mcp_reaches_the_api_only_through_the_proxy() {
        let mcp: Value = serde_json::from_str(&nexusmind_mcp()).unwrap();
        let env = &mcp["mcpServers"]["plugin_nexusmind_nexusmind"]["env"];
        // No base URL of its own: it must inherit the proxy route from the pod.
        assert!(env.get("NEXUSMIND_BASE_URL").is_none());
        assert!(env["NEXUSMIND_API_KEY"]
            .as_str()
            .unwrap()
            .starts_with("sandbox-placeholder"));
    }

    #[test]
    fn artifact_names_are_plain_file_names() {
        for good in ["page-2026.png", "login_step.jpeg", "trace.webp"] {
            assert!(valid_artifact_name(good), "{good}");
        }
        for bad in [
            "../x.png",
            "a/b.png",
            ".png",
            "x.sh",
            "",
            "x.png\n",
            &"a".repeat(200),
        ] {
            assert!(!valid_artifact_name(bad), "{bad}");
        }
    }

    #[test]
    fn the_sandbox_default_applies_only_where_a_sandbox_can_run() {
        let unset = json!({});
        assert_eq!(
            resolve_isolation(
                &unset,
                "github_issue_resolver",
                "claude",
                Isolation::Sandbox
            )
            .unwrap(),
            Isolation::Sandbox
        );
        // Unsupported template or executor: the default cannot apply, so it is local.
        assert_eq!(
            resolve_isolation(&unset, "lead_generation", "claude", Isolation::Sandbox).unwrap(),
            Isolation::Local
        );
        assert_eq!(
            resolve_isolation(&unset, "qa", "nexus", Isolation::Sandbox).unwrap(),
            Isolation::Local
        );
        // An explicit choice always wins over the default.
        assert_eq!(
            resolve_isolation(
                &json!({"isolation": "local"}),
                "qa",
                "claude",
                Isolation::Sandbox
            )
            .unwrap(),
            Isolation::Local
        );
    }

    #[test]
    fn pod_names_are_dns_safe() {
        assert_eq!(task_pod_name("0F9B7C2E-Run_42"), "task-0f9b7c2e-run-42");
        let long = task_pod_name(&"a".repeat(200));
        assert!(long.len() <= 63 && !long.ends_with('-'), "{long}");
        assert_eq!(task_pod_name("--x--"), "task-x");
    }

    #[test]
    fn the_pod_is_hardened() {
        let pod = task_pod_manifest(&request());
        let spec = &pod["spec"];
        assert_eq!(pod["metadata"]["namespace"], SANDBOX_NAMESPACE);
        assert_eq!(pod["metadata"]["labels"]["role"], "task");
        assert_eq!(pod["metadata"]["labels"][RUN_LABEL], "0F9B7C2E-Run_42");
        let name = pod["metadata"]["name"].as_str().unwrap();
        assert!(
            name.starts_with("task-0f9b7c2e-") && name.len() <= 63,
            "{name}"
        );
        // Every distinguishing part counts, even for long run ids and suffixes.
        let mut other = request();
        other.run_id = "0f9b7c2e-1d2a-4c3b-9e8f-0123456789ab".into();
        other.pod_suffix = "a1b2c3d4-issue-123456-10-v".into();
        let mut sibling = other.clone();
        sibling.pod_suffix = "a1b2c3d4-issue-123456-11-v".into();
        assert_ne!(
            task_pod_manifest(&other)["metadata"]["name"],
            task_pod_manifest(&sibling)["metadata"]["name"]
        );
        assert_eq!(spec["automountServiceAccountToken"], false);
        assert_eq!(spec["imagePullSecrets"], json!([{"name": "ghcr-pull"}]));
        assert_eq!(spec["hostUsers"], false);
        assert_eq!(spec["restartPolicy"], "Never");
        assert_eq!(spec["activeDeadlineSeconds"], 1800);
        assert_eq!(spec["securityContext"]["runAsNonRoot"], true);
        assert_eq!(spec["securityContext"]["runAsUser"], TASK_UID);
        assert_eq!(
            spec["securityContext"]["seccompProfile"]["type"],
            "RuntimeDefault"
        );
        let containers = spec["containers"].as_array().unwrap();
        assert_eq!(containers.len(), 1);
        let task = &containers[0];
        assert_eq!(task["securityContext"]["allowPrivilegeEscalation"], false);
        assert_eq!(task["securityContext"]["readOnlyRootFilesystem"], true);
        assert_eq!(
            task["securityContext"]["capabilities"]["drop"],
            json!(["ALL"])
        );
        assert!(task["resources"]["limits"]["memory"].is_string());
        assert!(task["resources"]["limits"]["cpu"].is_string());
        assert!(task.get("envFrom").is_none(), "no secret references");
    }

    #[test]
    fn only_empty_dirs_are_mounted() {
        let pod = task_pod_manifest(&request());
        for volume in pod["spec"]["volumes"].as_array().unwrap() {
            let kinds: Vec<&String> = volume
                .as_object()
                .unwrap()
                .keys()
                .filter(|k| *k != "name")
                .collect();
            assert_eq!(kinds, vec!["emptyDir"], "{volume}");
        }
        let mounts: Vec<&str> = pod["spec"]["containers"][0]["volumeMounts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["mountPath"].as_str().unwrap())
            .collect();
        assert!(
            mounts.contains(&WORKSPACE)
                && mounts.contains(&"/tmp")
                && mounts.contains(&"/home/task")
        );
    }

    #[test]
    fn the_environment_routes_through_the_proxy_and_holds_no_credential() {
        let env: std::collections::HashMap<String, String> =
            task_env(&request()).into_iter().collect();
        assert_eq!(
            env["ANTHROPIC_BASE_URL"],
            format!("http://{PROXY_AUTHORITY}/r/v2.org-1.run.1.sig/anthropic")
        );
        assert_eq!(
            env["NEXUSMIND_BASE_URL"],
            format!("http://{PROXY_AUTHORITY}/r/v2.org-1.run.1.sig/nexusmind")
        );
        assert_eq!(
            env["HTTPS_PROXY"],
            format!("http://run:v2.org-1.run.1.sig@{PROXY_AUTHORITY}")
        );
        assert!(
            env["NO_PROXY"].contains("factory-egress-proxy.nexusmind-sandbox.svc.cluster.local")
        );
        assert_eq!(env["CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"], "1");
        assert_eq!(env["DISABLE_AUTOUPDATER"], "1");
        assert_eq!(env["HOME"], "/home/task");
        // A placeholder, never a real credential.
        assert!(env["CLAUDE_CODE_OAUTH_TOKEN"].starts_with("sandbox-placeholder"));
        for forbidden in [
            "ANTHROPIC_API_KEY",
            "NEXUSMIND_API_KEY",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "TYPESAFE_API_KEY",
        ] {
            assert!(!env.contains_key(forbidden), "{forbidden}");
        }
        // The manifest carries exactly this environment.
        let manifest_env: Vec<(String, String)> = task_pod_manifest(&request())["spec"]
            ["containers"][0]["env"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["name"].as_str().unwrap().to_string(),
                    e["value"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        assert_eq!(manifest_env, task_env(&request()));
    }
}
