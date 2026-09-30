# Design — Factory F1: Secure executor and verification gate

## 0. Spikes (gate everything else)

### S1 — Claude Code through an auth-injecting proxy

**Question:** can Claude Code, holding **no real credential**, work through a reverse proxy that adds the credential?

**Setup:**

- The sandbox gets `ANTHROPIC_BASE_URL=http://egress-proxy.<ns>:8080/anthropic` and a placeholder credential, so the CLI starts.
- The proxy forwards to `https://api.anthropic.com` and replaces the auth header with the real one, taken from its own secret.

**Variants, in order of preference:**

1. **Subscription OAuth.** The proxy injects `Authorization: Bearer <token from claude setup-token>` plus the OAuth beta header.
2. **Anthropic API key.** The proxy injects `x-api-key`. This changes billing from the subscription to the API.

**Pass criteria:**

- `claude -p` completes a tool-using task;
- the placeholder never reaches Anthropic;
- the CLI makes no other outbound call that would fail under default-deny (telemetry, auto-update); if it does, it is listed and blocked or allowlisted.

### S2 — k3s enforcement

**Question:** on the production node (k3s v1.35, containerd 2.2, kernel 6.8), are these enforced:

- a default-deny egress NetworkPolicy (k3s embeds kube-router's policy controller unless it was disabled);
- `hostUsers: false` (user namespaces)?

**Method:** a throwaway namespace with two pods (probe and target), torn down afterwards.

**Pass criteria:** blocked egress really fails, allowed egress works, and in-pod UID 0 maps to an unprivileged host UID.

## 1. Egress proxy

**Where:** a small Rust binary (`factory_egress_proxy`) in the backend crate. It is deployed as its own Deployment and Service in the sandbox namespace, and it is **the only pod there with internet egress**.

**Routes:**

| Route | Upstream | Injected |
|---|---|---|
| `/anthropic/*` | `https://api.anthropic.com` | Claude credential (S1 variant) |
| `/nexusmind/*` | `https://api.nexusmind.smartcoderlabs.com` | A **per-run, read-only** NexusMind token |
| `CONNECT host:443` | Allowlist only: npm, PyPI, crates.io and GitHub read-only codeload | Nothing: an opaque tunnel |

**Rules:**

- **Per-task identity.** Each pod gets a random run token (the only secret it holds). The proxy maps it to the run, and every request is logged with it. An unknown token gets 403.
- **Header scrubbing.** Inbound `Authorization`, `x-api-key` and `cookie` headers are always stripped before injection, so a sandbox cannot pick its own identity.
- **No auth on tunnels.** CONNECT tunnels never carry injected credentials; they are for public registries only.
- **Deny by default.** Anything not on the list is refused, and each refusal is logged (the drill in F1 success criteria checks this).

**Run token (stateless).** `v1.<run_id>.<expires_unix>.<hex HMAC-SHA256(secret, "v1.<run_id>.<expires_unix>")>`.
- The worker signs it and the proxy verifies it with a shared secret (`FACTORY_PROXY_SIGNING_KEY`, present only in the worker and the proxy): constant-time compare, rejected once expired, no database lookup.
- **Where it travels:** clients cannot add custom headers (Claude Code, MCP servers), so for reverse routes it is **in the base-URL path**, `http://proxy/r/<token>/anthropic`. For tunnels it is in `Proxy-Authorization: Basic run:<token>` (`HTTPS_PROXY=http://run:<token>@proxy:8080`, which npm and pip support).
- **Logging:** the token appears in logs only as its `run_id`, never whole.

**NexusMind access: a bot user per organization (decision 2026-09-30).**

- Each organization has a bot user (`nexus-bot`) whose permissions are set with a custom role, editable per organization without code changes.
- Its API key exists **only** in the egress proxy, as a map `org_id → key` (`FACTORY_NEXUSMIND_KEYS`, JSON, from a Kubernetes secret).
- The run token therefore also carries the org: `v2.<org_id>.<run_id>.<expires_unix>.<hmac>`. The proxy injects that org's key and fails closed (503) when the org has none.
- The sandbox never sees a NexusMind credential.
- Scoping the bot to one project is a follow-up; today its role limits it org-wide.

## 2. Namespace, policy and RBAC

- Namespace `nexusmind-sandbox`, with Pod Security Admission `restricted`.
- NetworkPolicies:
  - `default-deny`: all ingress and egress;
  - `sandbox-egress`: pods labeled `role=task` may reach the proxy Service port and kube-dns (UDP/TCP 53), nothing else;
  - `proxy-egress`: the proxy may reach 0.0.0.0/0 on 443 and kube-dns;
  - ingress to the proxy is allowed only from `role=task` pods.
- The worker uses the ServiceAccount `factory-worker` in namespace `nexusmind`. It gets a Role in `nexusmind-sandbox` limited to `pods` (create, get, list, delete, watch), `pods/exec` (create) and `pods/log` (get). It has **no** access to secrets or other namespaces.

## 3. Task pod template

```yaml
spec:
  automountServiceAccountToken: false
  hostUsers: false                      # user namespaces (S2)
  restartPolicy: Never
  activeDeadlineSeconds: <wall_time>    # hard stop, also kills stuck tasks
  securityContext:
    runAsNonRoot: true
    runAsUser: 10001
    seccompProfile: {type: RuntimeDefault}
  containers:
  - name: task
    image: <sandbox image @digest>
    command: ["sleep", "infinity"]      # the worker drives it with exec
    securityContext:
      allowPrivilegeEscalation: false
      readOnlyRootFilesystem: true
      capabilities: {drop: ["ALL"]}
    resources: {limits: {cpu: "2", memory: 4Gi, ephemeral-storage: 8Gi}}
    env: [ANTHROPIC_BASE_URL, NEXUSMIND_BASE_URL, HTTPS_PROXY, FACTORY_RUN_TOKEN]
    volumeMounts: [workspace -> /workspace, home -> /home/task, tmp -> /tmp]   # emptyDir only
```

The node has no kubelet `podPidsLimit` (S2), so every `exec` is wrapped in `ulimit -u 512`. That is effectively per pod under user namespaces (see Spike results).

**Image:** v1 reuses the backend image (it already contains Claude Code, git, node, Python and the Playwright browsers) with the command overridden. A slim sandbox image is a follow-up.

## 4. Sandbox executor (worker)

1. The **worker** prepares the repository as today (clone with its GitHub token, checkout of the exact SHA). The token never enters the pod.
2. It creates the pod, waits until it is Ready, and streams the workspace in with `exec tar -x`, excluding `.git/config` credentials. Only the checkout content and `.git` objects are sent.
3. It runs Claude through `exec`, streaming JSONL, so the existing transcript capture is reused unchanged. Tests and scanners run through `exec` in the same pod (item 4).
4. It streams the result out with `exec git diff --binary` plus the result files, and applies it to the worker's checkout. `ensure_diff_has_no_secrets` and the F0 gates run as today, and the worker publishes.
5. It **always deletes** the pod, via drop guard and `finally`. A GC loop deletes any `role=task` pod older than its deadline plus a grace period, which covers worker crashes.

Kubernetes access goes through the `kube` crate (kube-rs) using the in-cluster ServiceAccount. It is a new dependency, and the alternative, shelling out to `kubectl`, would put a binary and a kubeconfig in the image.

### Executor structure (implementation plan)

- **`factory/sandbox.rs` (pure, done):** `task_pod_name`, `task_env` and `task_pod_manifest`. Tests pin every hardening field.
- **`factory/sandbox_exec.rs`:** a `SandboxRuntime` trait, so the worker is tested with a fake runtime and never needs a cluster in unit tests:
  ```rust
  #[async_trait]
  trait SandboxRuntime {
      async fn create(&self, manifest: &Value) -> Result<PodHandle>;
      async fn wait_ready(&self, pod: &PodHandle, timeout: Duration) -> Result<()>;
      /// argv runs as `sh -c 'ulimit -u 512; exec "$@"'`; stdin optional; stdout streamed line by line.
      async fn exec(&self, pod: &PodHandle, argv: &[String], stdin: Option<Vec<u8>>, on_line: &mut (dyn FnMut(&[u8]) + Send)) -> Result<ExecOutput>;
      async fn delete(&self, pod: &PodHandle) -> Result<()>;
      async fn list_expired(&self, now: DateTime<Utc>) -> Result<Vec<PodHandle>>;
  }
  ```
  - `KubeRuntime` implements it with kube-rs (`Api<Pod>::create`, `await_condition(is_pod_running)`, `exec` over WebSocket, `delete` with a grace period of 0).
  - `FakeRuntime` implements it for tests.
- **`run_in_sandbox(runtime, request, workspace_tar, argv)`:** create → wait → `exec tar -x` (stdin) → `exec argv` (stream) → `exec git diff --binary` → delete. An RAII guard deletes the pod on every exit path, including errors and panics.
- **Workspace in:** a `tar` of the checkout with `.git/config` rewritten to drop remotes that carry credentials (`git remote remove origin`), and without `.git/hooks`.
- **Diff out:** `git diff --binary <base_sha>` plus untracked files (`git add -N .` first), capped at 5 MB. It is applied in the worker with `git apply --index` and then goes through the existing `ensure_diff_has_no_secrets`.
- **GC:** a worker tick lists `role=task` pods older than `activeDeadlineSeconds + 5 min` and deletes them.

### Migration order

`github_pr_reviewer` goes first: it is read-only, so the diff is empty and the review output is the transcript. Then `qa`, then `github_issue_resolver`. Each template gets `executor: "sandbox"`, which is the default once the drill passes; `executor: "local"` stays as an explicit, UI-flagged unsafe escape hatch.

## 5. VerificationReport gate

A builder turns the results of tests, security scanners and DAST into the F0 `VerificationReport` contract for **one** `head_sha`:

- a blocking check that fails → it goes to `blocking_failures`;
- `passed` and `eligible_for_merge` follow the contract's cross-field rules.

The merge path (F0 item 4) requires a report whose `head_sha` equals the reviewed SHA.

## 6. Worker de-privileging

After items 3–4, no untrusted process runs in the worker container. What remains:

- the worker keeps `/data` and its secrets, because it publishes and records;
- `restrict_claude_environment` and the local Claude path are removed for sandboxed templates;
- the local path stays only behind an explicit `executor: "local"` config that the admin UI marks as unsafe, until every template is migrated.

## 7. Recover

- A run interrupted mid-task (worker restart) is resumed **only** when its lease is still owned and its pod still exists. Otherwise it is retried from scratch.
- Completed external writes (PR opened, comment posted) are never repeated: the existing delivery idempotency keys are checked before each write.

## 8. Rollout

1. Spikes S1 and S2.
2. Proxy, namespace and policies deployed, with no traffic yet.
3. The adversarial drill passes.
4. Templates move one by one: the reviewer first (read-only), then QA, then the resolver.
5. Local execution is disabled by default.

## Spike results

### S2 — k3s enforcement (run 2026-09-30 on `agency-os-production`, namespace torn down)

| Check | Result |
|---|---|
| PSA `restricted` admits the hardened template (non-root, RO rootfs, drop ALL, seccomp) | ✅ admitted |
| `hostUsers: false` | ✅ in-pod UID 0 maps to host UID 4110352384 (`uid_map: 0 4110352384 65536`) |
| Default-deny egress + allow-list | ✅ allowed pod:port reachable; internet (1.1.1.1:443) and the Kubernetes API (10.43.0.1:443) **blocked**; DNS allowed |
| Default-deny ingress | ✅ unlisted ingress blocked |
| PID limit | ❌ `pids.max = 28686`: the kubelet has no `podPidsLimit` |

**PID limit decision:** setting `podPidsLimit` means restarting k3s on a node shared with other workloads. Instead, the executor wraps every `exec` in `ulimit -u 512`. `RLIMIT_NPROC` is counted per kernel UID, and with user namespaces each pod gets its own host UID range, so the limit is effectively per pod. The node-level limit stays a follow-up for a maintenance window.

### S1 — Claude Code through an auth-injecting proxy (passed, 2026-09-30, CLI 2.1.280)

| Check | Result |
|---|---|
| `ANTHROPIC_BASE_URL` honored for inference | ✅ `POST /v1/messages?beta=true` goes to the proxy route |
| The CLI starts with a placeholder credential | ✅ `CLAUDE_CODE_OAUTH_TOKEN=<placeholder>` is sent as `Authorization`; the proxy strips it (Anthropic answers 401 with no injection, as intended) |
| Side traffic | ⚠️ Without flags the CLI also opens `CONNECT api.anthropic.com:443` (non-inference calls that bypass `ANTHROPIC_BASE_URL`). With `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` **all** side traffic disappears: only `/v1/messages` through the proxy |
| `HTTP_PROXY` + base URL | ⚠️ With `HTTP_PROXY` set, the base-URL request is sent in proxy form and misses the route: the sandbox must set `NO_PROXY=<proxy host>` (or not set `HTTP_PROXY`) |
| Injected real OAuth token → 200 | ✅ With a `claude setup-token` token injected **only by the proxy** (the CLI held a placeholder), a Bash-tool task completed (`result: sandbox-ok`, 2 turns, both `/v1/messages` → 200, no side traffic). **Variant 1 (subscription OAuth) works; no API key needed.** |

The sandbox env therefore becomes: `ANTHROPIC_BASE_URL`, a placeholder `CLAUDE_CODE_OAUTH_TOKEN`, `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`, `DISABLE_AUTOUPDATER=1`, `HTTPS_PROXY` (registries only), and `NO_PROXY=<proxy service>`.
