# Proposal — Factory F1: Secure executor and verification gate

**Change:** `factory-f1-sandbox`
**Project:** nexus-mind
**Status:** proposed
**Author:** Cesar Ruiz
**Date:** 2026-09-30
**Plan:** [`docs/factory/PLAN.md`](../../../docs/factory/PLAN.md) §7 F1

---

## 1. Problem (verified in production, 2026-09-30)

Autonomous agents process untrusted content: issue text, PR diffs, repository files and dependency scripts. Today they run with the privileges of the whole backend:

- **Claude Code runs as `root`** inside the `autonomous-worker` container, in the same pod as the API.
- That container **mounts `/data`**: the production SQLite database of every organization, plus Claude's and `gh`'s credentials.
- It receives **every secret** in `nexusmind-env` (`SUPERUSER_KEY`, `NEXUSMIND_TOKEN_ENCRYPTION_KEY`, R2 keys, GitHub OAuth secret, Jev key, …). The worker clears the environment it passes to Claude, but the worker's own `/proc/1/environ` **is readable** by Claude, because both run as root in the same container.
- There is **no NetworkPolicy** in the cluster, so egress is unrestricted.
- Repository test commands and security scanners (which run repository code, e.g. `npm install` scripts) run in the same container.

A single prompt-injected issue or malicious dependency can therefore read every secret and the whole database and send them anywhere. This is risk #1 in the architecture document's risk matrix, and it is open today.

## 2. Decisions (ADR recorded 2026-09-30)

- **No interim hotfix:** go straight to the full sandbox (user's choice).
- **Isolation:** one **ephemeral Pod per task** in a dedicated namespace. The pod runs:
  - as non-root, with Kubernetes user namespaces;
  - with a read-only root filesystem, all capabilities dropped and seccomp `RuntimeDefault`;
  - without a service-account token, and without host or `/data` mounts;
  - with CPU, memory, PID and time limits.
  - gVisor is a later extra layer.
- **Network:** namespace-wide default deny. The only egress allowed is to an **egress proxy**, plus DNS.
- **Credentials:** the sandbox holds **none**. Claude Code and the NexusMind MCP reach their APIs through the proxy, which injects authentication.

## 3. Scope

| # | Item |
|---|---|
| 0 | Spikes: (a) Claude Code through an auth-injecting reverse proxy with subscription OAuth; (b) NetworkPolicy enforcement and user namespaces on the k3s node |
| 1 | Egress proxy: reverse routes that inject credentials (Anthropic, NexusMind API) and an allowlisted CONNECT tunnel (package registries), deny by default, audited |
| 2 | Sandbox namespace, NetworkPolicy, RBAC (the worker can only manage pods in that namespace), hardened pod template |
| 3 | Sandbox executor in the worker: create the pod, stream the workspace in, stream the Claude transcript, stream the result out, always delete the pod |
| 4 | Tests and scanners run inside the sandbox, never in the worker |
| 5 | `VerificationReport` builder: one gate over tests, scanners and DAST, bound to the verified `head_sha` |
| 6 | Worker de-privileging: the worker keeps DB and GitHub access for publishing only; no untrusted process ever runs in its container |
| 7 | Recover rules: resume only with the original authorization and an owned workspace; never repeat completed writes |

## 4. Out of scope

gVisor or Kata runtimes, multi-tenant isolation (plan D3: internal first), the Model Gateway and router (F3).

## 5. Success criteria

- From inside a sandbox pod, the database, the worker's environment, every credential file and any non-allowlisted host are **unreachable**. This is proven by an adversarial drill script that tries each one.
- Claude Code, tests and scanners still complete their work through the proxy.
- Every task ends with its pod deleted, including on timeout, cancellation and worker crash (garbage collection).
