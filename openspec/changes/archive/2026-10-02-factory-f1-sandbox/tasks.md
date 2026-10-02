# Tasks — Factory F1: Secure executor and verification gate

## 0. Spikes
- [x] 0.1 S1: passed with subscription OAuth injected by the proxy (CLI holds a placeholder); with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` the only outbound call is `/v1/messages`; sandbox needs `NO_PROXY=<proxy>`
- [x] 0.2 S2: NetworkPolicy enforced, `hostUsers: false` works, PSA restricted admits the template; no kubelet PID limit → per-exec `prlimit --nproc=512:512` (dash has no `ulimit -u`; design, spike results)

## 1. Egress proxy
- [x] 1.1 `factory_egress_proxy` binary: reverse routes with credential injection, header scrubbing, CONNECT allowlist, deny by default, per-run HMAC token, request log (run_id only). Verified end to end with a real Claude session. Review fixes: resilient accept loop, tunnel connects before 200 (502 otherwise), case-insensitive Basic + 407 challenge, in-flight cap with 503 shedding, connect/read/tunnel timeouts
- [x] 1.2 Per-org `nexus-bot` user (no password, `.invalid` email) + `factory-bot` role (default read-only grants, admin edits preserved); rotation revokes old keys atomically with audit; `GET /v1/factory/bot`, `POST /v1/factory/bot/key` (key shown once, `no-store`); proxy map `org_id → key`; run token v2 carries org_id; admin panel on Factory policies

## 2. Cluster
- [x] 2.1 `deploy/oracle/k8s/factory-sandbox.yaml`: namespace (PSA restricted), default-deny + task/proxy NetworkPolicies (proxy never reaches cluster CIDRs), proxy Deployment/Service; strict schema validation passed; **not applied yet**
- [x] 2.2 `factory-worker` ServiceAccount and Role (pods, pods/exec, pods/log in the sandbox namespace only); worker Deployment must adopt the ServiceAccount when the executor ships

## 3. Sandbox executor
- [x] 3.1 Pod template, create/wait/delete with kube-rs; drop guard; GC of expired task pods (worker tick, deadline + 5 min)
- [x] 3.2 Workspace in (files only, no `.git`), Claude over exec with transcript capture (prompt on stdin), diff out against a pod-side baseline and applied to the worker's working tree (symlinks/gitlinks refused)

## 4. Tests and scanners in the sandbox
- [x] 4.1 `run_allowlisted_commands` and the security scanners execute inside the task pod — verification, QA test commands and semgrep/osv/nuclei run in commands pods; scans fail closed (semgrep fatal errors, unreachable DAST targets)

## 5. Verification gate
- [x] 5.1 `VerificationReport` builder bound to `head_sha`; the merge path requires it (stored per run and head, v81; required again after the soak; local reviewers decline)

## 6. De-privileging and drill
- [x] 6.1 Adversarial drill: `scripts/factory/sandbox_drill.py` run in production (2026-10-02) inside the agent pod and the commands pod of a real sandboxed PR review (run `73c1e1a7`, PR #284). Every attack blocked: confinement (non-root, no caps, no_new_privs, seccomp), own user and PID namespaces, no SA token, no DB/volume/credentials (no mount, no credential files), read-only root, no direct network (internet, Anthropic, GitHub, Kubernetes API, backend, metadata), proxy denials (no token 407, off-allowlist/metadata/non-inference/forged 403, registry token to Anthropic 403), no agent token anywhere in the commands pod, fork limit. The first run found `workspace_unpack_failed` (non-root tar on the root-owned `/workspace`, git dubious ownership), fixed in #285
- [x] 6.2 Templates migrated (reviewer, QA, judge, resolver single + fanout, security_scan, security_dast); `FACTORY_ISOLATION_DEFAULT=sandbox` set in production after the drill (2026-10-02); `isolation: "local"` stays as an explicit, UI-flagged unsafe escape hatch

## 7. Recover
- [x] 7.1 Never resume in place (the exec stream dies with the worker): lease expiry requeues from scratch; a retry deletes its orphan pods first; delivery keys are attempt-independent

## Rollout record (production)
- #281 F1 code; #282 bookworm builders + `ldd` guard (the trixie builder broke glibc and took prod down ~15 min; rolled back by image digest); #283 proxy in its own namespace (`nexusmind-egress`) and the ServiceAccount token mounted only in the worker container; #285 non-root workspace unpack.
- Cluster: `nexusmind-sandbox` (task pods, no credentials), `nexusmind-egress` (proxy + its secret), `factory-worker` Role scoped to the sandbox namespace (verified: 200 there, 403 on egress and nexusmind).
- Secrets via `scripts/factory/configure_sandbox_secrets.zsh`. Bot keys exist for SmartCoderLabs only; other orgs get one when they run sandboxed agents (their NexusMind route answers 503 until then).
