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

The node has no kubelet `podPidsLimit` (S2), so every `exec` is wrapped in `prlimit --nproc=512:512 --` (the image's `/bin/sh` is dash, where `ulimit -u` is unsupported and would fail open; `prlimit` exits non-zero if it cannot set the limit). That is effectively per pod under user namespaces (see Spike results).

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
      /// argv runs under `prlimit --nproc=512:512 --`; stdin optional; stdout streamed line by line.
      async fn exec(&self, pod: &PodHandle, argv: &[String], stdin: Option<Vec<u8>>, on_line: &mut (dyn FnMut(&[u8]) + Send)) -> Result<ExecOutput>;
      async fn delete(&self, pod: &PodHandle) -> Result<()>;
      async fn list_task_pods(&self) -> Result<Vec<TaskPodInfo>>;
  }
  ```
  - `KubeRuntime` implements it with kube-rs (`Api<Pod>::create`, `await_condition(is_pod_running)`, `exec` over WebSocket, `delete` with a grace period of 0).
  - `FakeRuntime` implements it for tests.
- **`run_in_sandbox(runtime, request, workspace_tar, argv)`:** create → wait → `exec tar -x` (stdin) → `exec argv` (stream) → `exec git diff --binary` → delete. An RAII guard deletes the pod on every exit path, including errors and panics.
- **Workspace in:** a `tar` of the checkout with `.git/config` rewritten to drop remotes that carry credentials (`git remote remove origin`), and without `.git/hooks`.
- **Diff out:** `git diff --binary <base_sha>` plus untracked files (`git add -N .` first), capped at 5 MB. It is applied in the worker with `git apply --index` and then goes through the existing `ensure_diff_has_no_secrets`.
- **GC:** a worker tick lists `role=task` pods older than `activeDeadlineSeconds + 5 min` and deletes them.

### Known limits (to settle in the drill)

- The worker's pre-lease probe (`probe_claude`, `claude_ready`) still checks the worker's own Claude login, so a worker that needs re-auth blocks sandboxed runs too, although they use the proxy's credential. Once every template runs sandboxed, the probe moves to the proxy.
- Behavior that differs from local runs, all failing closed: output over 32 MiB fails (`output_too_large`) instead of being truncated, and a pod killed by OOM or eviction reports `command_status_unknown`.
- The run budget in the sandbox is `wall_time` + 180 s (scheduling and image pull) + 120 s (unpack and cleanup). The pod and the run token live that long, and the worker's outer timeout is 30 s longer, so the sandbox's own deadline fires first and deletes the pod.
- Pod names include the lease attempt id, so a run reclaimed after a worker crash never collides with the orphaned pod, which the GC sweep removes.

### QA and judge in the sandbox (ADRs 1201aa7f, 7877a09b)

- **Reachable hosts** (signed into a v3 run token): enabled `web_application` targets that are HTTPS on 443 (others are skipped), the run's preview `app_base_url` (HTTPS on 443, else the run is blocked), and the agent's `sandbox_allowed_hosts` (admin-edited: CDNs, SSO, payment providers; validated on save). At most 16; none usable → `target_not_sandboxable`. The proxy still refuses any name resolving to a non-public address. Plain-HTTP subresources are not supported (the proxy only tunnels CONNECT).
- **Run input** may only set `trigger`, `judge_targets` and `app_base_url` over the agent config, so an input can never add hosts or change isolation, auto-merge or commands.
- **Test commands** (`test_commands`) run before the agent in a commands pod: registry-only token signed with the same hosts, target credentials on stdin, per-command timeout and optional failure reproduction. Output is scrubbed of the pod's token and formatted as before. The worker keeps the lease alive and honors cancel while it runs; infrastructure failures are `blocked_runtime`.
- **Browser**: the agent pod gets a Playwright config file (written on stdin after unpack) whose `launchOptions.proxy` carries the run token; `--mcp-config` holds no secret. Screenshots come back as `name<TAB>base64` (image files only, ≤50, ≤20 MiB); invalid names are skipped, and collection never fails a finished run.
- **Slack**: no Slack MCP exists; delivery stays worker-side via the connector webhook, so sandboxed runs do not list `mcp__slack__*`.
- Target credentials reach the agent only through the prompt (on stdin) and are redacted from transcripts.

### Resolver in the sandbox (ADR a8882f51)

- **Files, not history.** The pod receives the checkout's files without any `.git` (at any depth: repository, worktree file, submodules, vendored repos). When the job keeps a diff, the executor commits the unpacked tree as a baseline in a fresh repository and every diff is `git diff --binary HEAD`. This works the same for clones and fanout worktrees and never exposes history to the pod.
- **Applying.** Pod diffs are untrusted: any diff that creates or changes a symlink (120000) or a gitlink (160000) is refused (a planted symlink could make the worker read and publish its own files). Diffs are applied to the working tree only, so the existing publish gates (secret scan, excluded paths, size limits) see them exactly like a local agent's edits. `PENDING.md` is read only if it is a regular file.
- **Checkpoints.** Every 180 s the executor sends a diff of the agent's work; on sandbox timeout it sends a last one before deleting the pod. The worker resets its checkout to `HEAD`, applies the diff and, in the fanout, pushes the WIP branch as before. The finished run's diff arrives last (`Final`) and is applied without a WIP push. `sandbox_timeout` maps to `budget_exhausted`, so the partial-PR path runs as locally.
- **Fanout.** One pod per issue (`slot = issue-<n>`); pod names are `task-<run prefix>-<sha256(run/suffix)>`, so parallel issues, retries and commands pods never collide or treat each other as orphans.
- **NexusMind MCP.** The agent pod runs `nexusmind-mcp` with an inline config holding only a placeholder key; it inherits the proxy route in `NEXUSMIND_BASE_URL`, and the proxy replaces `Authorization` with the org's bot key (verified: nexusmind-mcp 0.15.0 uses `NEXUSMIND_BASE_URL` and `Authorization`).
- **Verification at publish** runs in a commands pod over the applied checkout; a failing command blocks the publish, as locally.
- The worker's outer timeout is the pods' lifetime + 180 s, so the inner deadline (and its last diff) always fires first.

### Migration order

`github_pr_reviewer` goes first: it is read-only, so the diff is empty and the review output is the transcript. Then `qa`, then `github_issue_resolver`. Each template gets `isolation: "sandbox"` in its agent config (`executor` already names the provider, `claude` | `nexus`). Absent means `local` until the drill passes, then the default flips to `sandbox`; `isolation: "local"` stays as an explicit, UI-flagged unsafe escape hatch. A sandbox request that cannot be honored (template not migrated, `nexus` executor) is refused on save (422) and blocked at run time, never silently run locally.

## 5. VerificationReport gate

A builder turns the results of tests, security scanners and DAST into the F0 `VerificationReport` contract for **one** `head_sha`:

- a blocking check that fails → it goes to `blocking_failures`;
- `passed` and `eligible_for_merge` follow the contract's cross-field rules.

The merge path (F0 item 4) requires a report whose `head_sha` equals the reviewed SHA.

**Decision (ADR "Sandboxed reviewer produces the VerificationReport the merge requires"):**

- Only a sandboxed run produces evidence. `verification_commands` (allowlisted argv) run **inside the task pod**, and required CI checks are folded in. A reviewer running locally declines auto-merge with `verification_requires_sandbox`: repository code never runs in the worker.
- **Two pods per run.** The agent runs in an *agent pod* (profile `Agent`: proxy routes with the full run token) that never runs repository code. Verification commands run afterwards in a separate *commands pod* (profile `Commands`) whose manifest holds only a **registry-only run token** (run id + `-registry`). One pod is not enough: every process in a pod runs as the same UID and can read PID 1's environment (`/proc/1/environ`), so untrusted code must never share a pod with a token that reaches a credentialed upstream.
- The proxy opens registry tunnels for the registry-only token but denies every credentialed route (`token_scope`), so code under test can install packages but never spends the run's Claude budget or calls NexusMind as the run.
- Commands run as `prlimit … python3 -c <env-from-stdin> timeout --kill-after=10 <secs> <argv>`: the environment (and, for QA, target credentials) is written to stdin, never placed in the exec request. The agent's prompt also travels on stdin.
- A failing command or a broken exec is evidence (it blocks), not a job failure. Every check is blocking. A pending or missing required CI check blocks, and a report with no passing check is `no_verification_evidence`.
- Reports are stored per `(org, run, head_sha)` (migration v81). `auto_merge_pull` stores the report and merges only an eligible one; `merge_after_soak` requires the stored report again. Soaks started before v81 decline with `verification_missing`.
- Budget: agent pod = `wall_time` + 180 s + 120 s; commands pod = 180 s + 120 s + 310 s per command (twice with failure reproduction). Output-format retries reuse the first run's receipts (same head) and do not start a commands pod again.

### Tunnel destinations

Tunnels are judged by where a name resolves, not by its spelling: the proxy resolves the host, keeps only globally routable addresses (no private, loopback, link-local/metadata, CGNAT, ULA, IPv4-mapped or NAT64 forms of those) and connects to one of the checked addresses, so DNS rebinding cannot swap it afterwards. A name with no public address is refused (403).

Run token **v3** (`v3.<org>.<run>.<exp>.<base64url hosts>.<hmac>`) carries extra tunnel hosts for that run only (QA targets). Hosts must be lowercase DNS names; IP literals are refused at signing.

## 6. Worker de-privileging

After items 3–4, no untrusted process runs in the worker container. What remains:

- the worker keeps `/data` and its secrets, because it publishes and records;
- `restrict_claude_environment` and the local Claude path are removed for sandboxed templates;
- the local path stays only behind an explicit `isolation: "local"` config that the admin UI marks as unsafe, until every template is migrated.

## 7. Recover

- A run interrupted mid-task (worker restart) is **never resumed in place**: the agent streams through the worker's `exec`, and a crashed worker loses that stream, so there is nothing to re-attach to. When the lease expires, the attempt is revoked and the run is requeued and retried from scratch (existing behavior, bounded by `max_attempts`).
- The pods of earlier attempts are deleted before the new pod is created (label `factory.nexusmind/run`), so an orphan cannot keep running the agent and spending the run's budget while the retry runs. The GC sweep covers pods of runs that are never retried.
- Completed external writes are never repeated: delivery idempotency keys do not depend on the run or attempt (`review:<definition>:<repo>:<pr>:<head>`, `resolver:<definition>:<repo>:<issue>`, …) and `delivered` deliveries are skipped.

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
