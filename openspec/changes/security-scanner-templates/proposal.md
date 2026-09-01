# Proposal — Security Scanner Templates

**Change:** `security-scanner-templates`
**Project:** `nexus-mind`
**Status:** proposed
**Date:** 2026-08-31

## Problem

NexusMind ships evidence-only autonomous-agent templates (QA, Issue Resolver, PR Reviewer, Judge, Lead
Generation, AI Content Manager), but has no template that audits the security posture of the owner's own
code and deployments. Teams want an autonomous agent that finds vulnerabilities and records them as
canonical findings with technical evidence, without hand-writing exploits or bolting on a second agent
framework.

The unsafe shortcut would be to add one "security" template that immediately runs intrusive, active
attack traffic (ZAP Active Scan, Nuclei, SQLMap) against a live URL taken from free-form run input. That
breaks the platform's evidence-only invariant, turns the runtime into the origin of attack traffic, and
risks scanning infrastructure the owner does not own.

This change ships **Phase 1 only**: a read-only static template. Active DAST is scoped separately in
Phase 2 (see `design.md` D4) and is out of scope here.

## Goals

1. Ship one versioned, NexusMind-managed template — **`security_scan`** — that runs SAST (Semgrep) and
   dependency audit / SCA (osv-scanner / `npm audit`) over a repository checkout and records canonical
   findings with file+line evidence.
2. Reuse the existing safety envelope: the `read-only` execution profile, the `repository` target kind,
   the `repository:read` / `finding:write` / `delivery:write` capabilities, and the existing
   findings/deliveries pipeline. The agent never modifies the repository.
3. Introduce a **dedicated, allowlisted scanner runner** (`run_security_scanners()`) with fixed argv
   templates, instead of loosening the existing package-manager test runner
   (`run_allowlisted_commands`, `worker.rs:286`).
4. Map scanner output to the `AutonomousAgentFinding` contract deterministically (stable fingerprints,
   normalized severity) so re-runs dedupe instead of piling up.
5. Deliver findings to the configured outputs (NexusMind, Slack, GitHub issue) reusing existing delivery.

## Non-goals (this change)

- No active/intrusive scanning (no ZAP Active Scan, Nuclei, SQLMap). That is Phase 2.
- No new execution profile and no new capability (`scan:active` belongs to Phase 2).
- No `web_application` target wiring, scope guard, rate limiting, or egress firewall (Phase 2).
- No auto-remediation, no PRs, no code changes of any kind.
- No third-party offensive MCP servers.

## Approach (summary)

`security_scan` runs inside the existing worker lifecycle: the host clones the pinned repository, the
agent invokes the allowlisted scanners through `run_security_scanners()`, parses their JSON, and returns
the standard single-JSON-object result (`{summary, findings[]}`). `evaluate_structured_result` gains a
`"security_scan"` arm so it inherits salvage-over-reject and the 100-finding cap. Findings persist and
deliver through the existing paths. Scanner binaries (`semgrep`, `osv-scanner`) are provisioned on the
runtime server and documented in operations.

Full technical design, file-by-file surface, and the Phase 2 boundary live in
`openspec/changes/security-scanner-templates/design.md`.

## Risks

1. **Loosening command execution.** Even scoped to a dedicated runner, allowing new binaries widens the
   execution surface → mandatory adversarial review (arch-review / judgment-day) before merge.
2. **Scanner provisioning drift.** If `semgrep`/`osv-scanner` are missing on the runtime, runs must fail
   with a clear, sanitized `scanner_unavailable` error, never a partial silent pass.
3. **False positives.** Findings land as `open`; triage uses the existing
   `PatchAutonomousAgentFindingRequest`. No auto-close.
4. **Output leakage.** Scanner output can echo code/secrets; the existing secret-canary scan on the
   serialized result must run before persistence (already enforced in `evaluate_structured_result`).
