# Proposal — Security DAST Template (Phase 2)

**Change:** `security-dast-template`
**Project:** `nexus-mind`
**Status:** proposed
**Date:** 2026-09-01
**Builds on:** `security-scanner-templates` (Phase 1 design D4).

## Problem

Phase 1 shipped read-only SAST/SCA (`security_scan`). Teams also want an autonomous agent that runs an
**authorized active security scan** (DAST) against their own running staging/dev deployments and records
findings with request/response evidence — without hand-writing exploits, giving the agent shell access, or
risking traffic against infrastructure they do not own.

## Goals

1. Ship one versioned template — **`security_dast`** — that runs **Nuclei** against a **pre-registered,
   enabled `web_application` target** and records canonical findings with request/response evidence.
2. Keep it **worker-driven**: the worker runs Nuclei under a fixed allowlist; the agent only triages the
   results. The agent gets the default tool grant (`plan` / `Read,Grep,Glob`, no Bash/MCP), so it can never
   send traffic itself.
3. Make the target **structural, not free-form**: the scan URL comes only from a registered
   `web_application` target's `config.url`; a per-request **host scope guard** drops anything off that host.
4. Bound the scan: rate limit, per-target timeout, invocation cap, severity filter.

## Non-goals (this change)

- No OWASP ZAP (it is a daemon, not a one-shot CLI) and no SQLMap — deferred.
- No free-form scan URLs, no scanning of unregistered/disabled targets.
- No auto-remediation, no code changes, no repository access.
- No production-by-default: prod targets require explicit operator opt-in (see operations).

## Approach (summary)

`security_dast` reuses the Phase 1 worker-driven shape. `execute_claim` resolves the definition's enabled
`web_application` targets from `config["targets"]` (already materialized at claim time), runs Nuclei against
each authorized URL via `run_dast_scan`, maps `-jsonl` output to canonical findings **filtered to the
authorized host**, and injects them as `scanner_findings` for the agent to triage. Findings persist and
deliver through the existing paths.

## Design deviation from Phase 1 design D4 (surfaced during implementation)

D4 proposed a new `active-scan` execution profile + `scan:active` policy capability as guardrails. During
implementation this proved **decorative**: `resolve_execution`/`managed_profiles`/`validate_capabilities`
are only used by the standalone `GET /v1/automation/profiles` authorize endpoint (`api/automation.rs`) and
do **not** gate the run path. The real, enforced guardrails are worker-side:

- the agent's tool grant falls to the default `plan` / `Read,Grep,Glob` (no Bash/MCP), so it cannot scan;
- the worker runs Nuclei only against a registered target URL (never run input);
- the per-finding host scope guard drops off-scope results.

So this change implements the real guardrails and keeps `scan:active` only as a descriptive template
capability tag (UI/intent). The execution-profile machinery is left untouched to avoid implying enforcement
it does not provide.

## Risks

1. **Active traffic.** Nuclei sends real requests. Mitigated by authorized-target-only URLs, host scope
   guard, rate limit, timeouts, and staging-by-default operator guidance.
2. **Runtime egress.** The runtime must reach the target; without an egress allowlist the scope guard is the
   only barrier — document both.
3. **Scanner provisioning.** Missing `nuclei` fails the run closed (`scanner_unavailable`).
