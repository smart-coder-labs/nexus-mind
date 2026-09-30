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

#[derive(Clone, Debug)]
pub struct TaskPodRequest {
    pub org_id: String,
    pub run_id: String,
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

pub fn task_pod_manifest(request: &TaskPodRequest) -> Value {
    let env: Vec<Value> = task_env(request)
        .into_iter()
        .map(|(name, value)| json!({"name": name, "value": value}))
        .collect();
    json!({
        "apiVersion": "v1",
        "kind": "Pod",
        "metadata": {
            "name": task_pod_name(&request.run_id),
            "namespace": SANDBOX_NAMESPACE,
            "labels": {"role": "task", "app": "factory-task"},
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
            image: "ghcr.io/acme/sandbox@sha256:abc".into(),
            run_token: "v2.org-1.run.1.sig".into(),
            wall_time_secs: 1800,
        }
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
