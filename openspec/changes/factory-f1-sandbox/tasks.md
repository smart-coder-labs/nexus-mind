# Tasks — Factory F1: Secure executor and verification gate

## 0. Spikes
- [ ] 0.1 S1: Claude Code through an auth-injecting reverse proxy (subscription OAuth, fallback API key); list every outbound host the CLI contacts
- [x] 0.2 S2: NetworkPolicy enforced, `hostUsers: false` works, PSA restricted admits the template; no kubelet PID limit → per-exec `ulimit -u 512` (design, spike results)

## 1. Egress proxy
- [ ] 1.1 `factory_egress_proxy` binary: reverse routes with credential injection, header scrubbing, CONNECT allowlist, deny by default, per-run token, request log
- [ ] 1.2 Per-run read-only NexusMind token (only_context scope, expires with the run)

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
