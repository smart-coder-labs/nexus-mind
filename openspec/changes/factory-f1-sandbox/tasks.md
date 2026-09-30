# Tasks — Factory F1: Secure executor and verification gate

## 0. Spikes
- [x] 0.1 S1: passed with subscription OAuth injected by the proxy (CLI holds a placeholder); with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` the only outbound call is `/v1/messages`; sandbox needs `NO_PROXY=<proxy>`
- [x] 0.2 S2: NetworkPolicy enforced, `hostUsers: false` works, PSA restricted admits the template; no kubelet PID limit → per-exec `ulimit -u 512` (design, spike results)

## 1. Egress proxy
- [x] 1.1 `factory_egress_proxy` binary: reverse routes with credential injection, header scrubbing, CONNECT allowlist, deny by default, per-run HMAC token, request log (run_id only). Verified end to end with a real Claude session. Review fixes: resilient accept loop, tunnel connects before 200 (502 otherwise), case-insensitive Basic + 407 challenge, in-flight cap with 503 shedding, connect/read/tunnel timeouts
- [ ] 1.2 Per-org `nexus-bot` user + custom role; proxy map `org_id → key`; run token v2 carries org_id

## 2. Cluster
- [ ] 2.1 `nexusmind-sandbox` namespace (PSA restricted), NetworkPolicies, proxy Deployment/Service
- [ ] 2.2 `factory-worker` ServiceAccount and Role (pods, pods/exec and pods/log only, in the sandbox namespace)

## 3. Sandbox executor
- [ ] 3.1 Pod template, create/wait/delete with kube-rs; drop guard; GC of expired task pods
- [ ] 3.2 Workspace in (tar over exec, no credentials), Claude over exec with transcript capture, diff out and apply

## 4. Tests and scanners in the sandbox
- [ ] 4.1 `run_allowlisted_commands` and the security scanners execute inside the task pod

## 5. Verification gate
- [ ] 5.1 `VerificationReport` builder bound to `head_sha`; the merge path requires it

## 6. De-privileging and drill
- [ ] 6.1 Adversarial drill script: from a task pod try the DB, the worker environment, credential files, non-allowlisted hosts and the Kubernetes API; every attempt must fail
- [ ] 6.2 Migrate templates (reviewer → QA → resolver); local execution only behind an explicit unsafe flag

## 7. Recover
- [ ] 7.1 Resume only with an owned lease and a live pod; never repeat completed writes
