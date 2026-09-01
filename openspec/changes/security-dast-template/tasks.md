# Tasks — Security DAST Template (Phase 2: `security_dast`)

**Change:** `security-dast-template`
**Project:** `nexus-mind`

Active DAST sends real attack traffic, so the security-critical logic (authorized-URL-only, host scope
guard, fixed argv) is isolated in a pure, exhaustively-tested module and the agent gets no network tools.

## Phase 0 — Contracts

- [x] 0.1 Freeze the target/URL/scope/argv/finding contracts → `contracts.md` (C1–C5).

## Phase 1 — Pure module (`automation/security_dast.rs`)

- [x] 1.1 `parse_http_url` / `authorized_target_url`: http(s) only (case-insensitive), host required, reject
  embedded credentials, conservative host charset. Tests cover ftp/file/js/no-scheme/no-host/creds.
- [x] 1.2 `in_scope` host scope guard (exact host, port-agnostic; rejects subdomains and path-embedded hosts).
- [x] 1.3 `build_nuclei_argv`: fixed flags, re-validates URL + severity + rate/timeout; program allowlist
  `{nuclei}`.
- [x] 1.4 `map_nuclei_jsonl`: parse JSONL, DROP out-of-scope findings, map to canonical finding with
  request/response evidence + stable fingerprint; malformed lines skipped, never panics.
- [x] 1.5 Severity normalization + `fingerprint_dast`. (7/7 module tests green.)

## Phase 2 — Template + worker wiring

- [x] 2.1 `security_dast` template in `managed_templates()` (`scan:active` tag, target-scoped config schema,
  no url/repository field, budgets with `requests_per_second`). Presence test asserts it is evidence-only.
- [x] 2.2 `run_dast_capture` (nuclei allowlist gate) + `run_dast_scan` (resolve enabled web_application
  targets from `config["targets"]`, optional `target_name` filter, per-target nuclei run, scope-guarded map,
  invocation cap). Fails closed on `no_authorized_target`.
- [x] 2.3 `fixed_prompt` `security_dast` arm (triage-only; must not scan/fetch; preserve fingerprint/evidence)
  + `security_dast` added to `evaluate_structured_result`.
- [x] 2.4 `execute_claim` block: run `run_dast_scan` → inject `scanner_findings`. Agent gets the default tool
  grant (`plan`/`Read,Grep,Glob`) — verified no Bash/MCP, so it cannot send traffic.
- [x] 2.5 Worker tests: prompt, evaluator, `run_dast_scan` fail-closed, `run_dast_capture` allowlist.

## Phase 3 — Provisioning, docs, review

- [x] 3.1 Operations doc: nuclei provisioning, authorization, egress allowlist, bounds → `operations.md`.
- [x] 3.2 Adversarial security review (fresh context): no high-confidence vulns; verified target-authorization,
  argv-injection resistance, tool-grant (default `plan`/`Read,Grep,Glob`, no network tools), multi-tenancy,
  and secret-canary. One actionable finding fixed — added `-disable-redirects` so the host scope holds at the
  traffic layer (blind-SSRF-via-redirect guard), not just when recording findings. Noted defense-in-depth
  follow-up: blocklist a `targets` key in the per-run input merge (`queries.rs`) — currently not exploitable.
- [ ] 3.3 Live E2E on a runtime with `nuclei` + an owned staging target (needs provisioning): assert
  scoped findings with evidence and re-run dedupe.
- [ ] 3.4 Manual visual smoke of the `security_dast` card + target registration UI.

## Status

Verified: 13 new tests green (`cargo test --lib -- security_dast run_dast`), clippy clean on stable 1.98.
Design deviation from Phase 1 D4 (no `active-scan` profile — it would be decorative) documented in
`proposal.md`.
