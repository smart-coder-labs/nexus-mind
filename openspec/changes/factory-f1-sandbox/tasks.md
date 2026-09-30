# Tasks — Factory F1: Secure executor and verification gate

## 0. Spikes
- [x] 0.1 S1: passed with subscription OAuth injected by the proxy (CLI holds a placeholder); with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` the only outbound call is `/v1/messages`; sandbox needs `NO_PROXY=<proxy>`
- [x] 0.2 S2: NetworkPolicy enforced, `hostUsers: false` works, PSA restricted admits the template; no kubelet PID limit → per-exec `ulimit -u 512` (design, spike results)

## 1. Egress proxy
- [x] 1.1 `factory_egress_proxy` binary: reverse routes with credential injection, header scrubbing, CONNECT allowlist, deny by default, per-run HMAC token, request log (run_id only). Verified end to end with a real Claude session. Review fixes: resilient accept loop, tunnel connects before 200 (502 otherwise), case-insensitive Basic + 407 challenge, in-flight cap with 503 shedding, connect/read/tunnel timeouts
- [x] 1.2 Per-org `nexus-bot` user (no password, `.invalid` email) + `factory-bot` role (default read-only grants, admin edits preserved); rotation revokes old keys atomically with audit; `GET /v1/factory/bot`, `POST /v1/factory/bot/key` (key shown once, `no-store`); proxy map `org_id → key`; run token v2 carries org_id; admin panel on Factory policies

## 2. Cluster
- [x] 2.1 `deploy/oracle/k8s/factory-sandbox.yaml`: namespace (PSA restricted), default-deny + task/proxy NetworkPolicies (proxy never reaches cluster CIDRs), proxy Deployment/Service; strict schema validation passed; **not applied yet**
- [x] 2.2 `factory-worker` ServiceAccount and Role (pods, pods/exec, pods/log in the sandbox namespace only); worker Deployment must adopt the ServiceAccount when the executor ships

## 3. Sandbox executor
- [x] 3.1 Pod template, create/wait/delete with kube-rs; drop guard; GC of expired task pods (worker tick, deadline + 5 min)
- [ ] 3.2 Workspace in (tar over exec, no credentials), Claude over exec with transcript capture, diff out and apply — reviewer wired behind `isolation: "sandbox"`; `apply_sandbox_diff` ready, wired when QA/resolver migrate

## 4. Tests and scanners in the sandbox
- [ ] 4.1 `run_allowlisted_commands` and the security scanners execute inside the task pod — verification commands run for sandboxed runs in a separate commands pod holding only a registry-only token (env on stdin); the local resolver path and the scanners move with the QA/resolver migration

## 5. Verification gate
- [x] 5.1 `VerificationReport` builder bound to `head_sha`; the merge path requires it (stored per run and head, v81; required again after the soak; local reviewers decline)

## 6. De-privileging and drill
- [ ] 6.1 Adversarial drill script: from a task pod try the DB, the worker environment, credential files, non-allowlisted hosts and the Kubernetes API; every attempt must fail — `scripts/factory/sandbox_drill.py` written (confinement, userns, PID ns, SA token, /data, RO rootfs, env credentials, direct network, proxy denials, registry token scope, fork limit); **run in the cluster pending**
- [ ] 6.2 Migrate templates (reviewer → QA → resolver); local execution only behind an explicit unsafe flag

## 7. Recover
- [x] 7.1 Never resume in place (the exec stream dies with the worker): lease expiry requeues from scratch; a retry deletes its orphan pods first; delivery keys are attempt-independent
