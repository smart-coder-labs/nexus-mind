# Tasks — Security Scanner Templates (Phase 1: `security_scan`)

**Change:** `security-scanner-templates`
**Project:** `nexus-mind`

TDD is strict for backend. Every implementation task starts RED, then GREEN. Loosening command execution is
security-surface work, so this ships as a small feature-branch chain with an adversarial review gate before
merge — not one large PR. Phase 2 (`security_dast`) is a separate change and is NOT tracked here.

## Delivery chain

| PR | Slice | Approx. authored lines |
|---|---|---:|
| 1 | Dedicated scanner runner + output→finding mapping (pure, well-tested) | 250–400 |
| 2 | `security_scan` template definition + worker wiring (prompt, evaluate arm, delivery/budgets) | 300–450 |
| 3 | Runtime provisioning + operations docs + E2E against a seeded repo | 200–350 |

## Phase 0 — Contracts and threat model

- [ ] 0.1 Extend `docs/automation-threat-model.md` (or a `security-scan` section) with: scanner trust
  boundary, why a dedicated runner instead of loosening `run_allowlisted_commands`, output-leakage flow,
  and the `scanner_unavailable` failure mode.
- [x] 0.2 Freeze the agent output contract for `security_scan`:
  `{"summary": string, "findings": [{"title","severity","summary","fingerprint","evidence":{...}}]}`,
  and the fingerprint formulas (`sha(check_id+path+line)` for SAST, `sha(package+advisory)` for SCA).
  → `contracts.md` C1/C2; enforced by `security_scan::fingerprint_sast/_sca`.
- [x] 0.3 Freeze the fixed argv templates for `semgrep` and `osv-scanner` (closed flags; only ruleset /
  ecosystem injected from validated config) and the normalized severity map
  (scanner severity → `info|low|medium|high|critical`). → `contracts.md` C3/C4;
  enforced by `security_scan::build_*_argv` + `normalize_*_severity`.

## Phase 1 — Dedicated scanner runner (PR 1)

- [x] 1.1 RED: unit tests for the scanner runner — pure allowlist/argv/ruleset tests
  (`program_allowlist_is_closed`, `semgrep_ruleset_validation_blocks_traversal_and_junk`,
  `semgrep_argv_is_fixed_and_injects_only_validated_slots`, `osv_argv_is_fixed_json_recursive`) PLUS
  spawn-level tests in `worker.rs` (`spawn_capture_returns_stdout_on_success`,
  `spawn_capture_maps_missing_binary_to_scanner_unavailable`, `spawn_capture_times_out`,
  `run_scanner_capture_rejects_non_allowlisted_program`). All green.
- [x] 1.2 GREEN: `spawn_capture` + `run_scanner_capture` + `run_security_scanners` in `automation/worker.rs`
  (spawns via `tokio::process::Command`, reuses `restrict_test_environment`, non-zero exit is not a failure,
  ENOENT → `scanner_unavailable`, timeout → `scanner_timeout`, invocation cap backstop). Fixed argv/allowlist
  live in `automation/security_scan.rs`. `run_allowlisted_commands` untouched.
- [x] 1.3 RED: mapper tests — Semgrep `results[]` and osv JSON → canonical finding shape, stable
  fingerprints, normalized severity; malformed input degrades to zero findings, never a panic.
- [x] 1.4 GREEN: pure mappers (`map_semgrep_json`, `map_osv_json`) in `automation/security_scan.rs`.
- [x] 1.5 Adversarial security review run on commit `17ff6ac` (branch `agent/security-scanner-templates`):
  **no high-confidence vulnerabilities**. Verified: no shell (argv-only spawn), allowlist not bypassable,
  `validate_semgrep_ruleset` blocks absolute/`..`/URL configs, semgrep/osv run with no code-executing flags,
  and — confirmed below the prompt — `security_scan` falls to the default tool grant `plan` / `Read,Grep,Glob`
  with **no MCP/Bash/Edit/Write**, so the read-only invariant is enforced by the grant, not just the prompt.
  Secret-canary still gates output. Two optional hardening notes (non-blocking): default Semgrep ruleset
  `auto` contacts the registry+telemetry (consider `p/ci` default); document that a configured ruleset path is
  trusted against repo-controlled symlinks.

## Phase 2 — Template + worker wiring (PR 2)

- [x] 2.1 RED+GREEN: `security_scan_template_is_registered_read_only_and_shaped` asserts v1, exact
  capabilities, absence of any write/scan:active capability, workflow, and `repository.required`.
- [x] 2.2 GREEN: `security_scan` `AutonomousAgentTemplate` added to `managed_templates()`
  (`api/autonomous_agents.rs`).
- [~] 2.3 Capability shape asserted in 2.1 (read-only-compatible: no `repository_write`/`merge`/etc., so
  `policy.rs::validate_capabilities` allows it under `read-only`). **Remaining:** an explicit
  `tests/automation_policy.rs` case exercising `resolve_execution` for this definition.
- [x] 2.4 RED: `security_scan_prompt_is_triage_only_and_read_only` (triage-only, read-only, preserves
  fingerprint/evidence, no invention) + `security_scan_evaluator_follows_finding_contract` (summary
  required, clean scan passes, secret canary trips).
- [x] 2.5 GREEN: `fixed_prompt` `security_scan` arm added; `"security_scan"` added to the
  `matches!(template, ...)` set in `evaluate_structured_result`. Worker-driven injection block added to
  `execute_claim` (`run_security_scanners` → `runtime_config.scanner_findings`).
- [x] 2.6 Delivery/budgets confirmed **template-agnostic**: `execute_claim` evaluates any findings-producing
  template (the generic `else` branch) and delivery is driven by `outputs` config (like `lead_generation`),
  so `nexusmind`/`slack`/`github_issue` route without a per-key arm. No worker delivery change needed.
- [x] 2.7 `scanner_unavailable` → the `execute_claim` block returns `blocked_policy` with the sanitized
  code and tears down the workdir; `spawn_capture` maps ENOENT → `scanner_unavailable`
  (`spawn_capture_maps_missing_binary_to_scanner_unavailable`).

## Phase 3 — Provisioning, docs, E2E (PR 3)

- [x] 3.1 Provisioning documented in `openspec/changes/security-scanner-templates/operations.md` (semgrep +
  osv-scanner, versions/Docker, PATH, failure modes, verify commands). (Global
  `docs/autonomous-agents-operations.md` does not exist in this tree; kept self-contained in the change.)
- [~] 3.2 E2E: `security_scan_e2e_finds_a_planted_semgrep_hit` (`#[ignore]`) exercises the real scanner path
  end-to-end when `semgrep` is on PATH. **Remaining (needs runtime with binaries + run harness):** full-run
  E2E asserting exact findings + fingerprints and re-run dedupe (`occurrence_count`).
- [~] 3.3 The triage prompt preserves fingerprint/evidence and forbids inventing findings; the read-only
  profile + no write capability prevents repo mutation; the secret-canary scan runs pre-persist.
  **Remaining:** a live-run assertion of a clean worktree + secret-free deliveries.
- [~] 3.4 The card renders from the templates endpoint with **no admin-UI code change** (list is served by
  `list_templates`). **Remaining:** manual visual smoke in the Templates tab.

## Status

Branch `agent/security-scanner-templates` → **PR #268**
(https://github.com/smart-coder-labs/nexus-mind/pull/268). Adversarial security review: clean.
**CI: all green** (Backend/Rust, Admin, Backoffice, E2E Smoke).

Commits: `17ff6ac` feature · `aea2cc8` ruleset hardening · `59404dd`+`0333473` fix two pre-existing
stale tests (`main` was already red on both) · `3abf0ae` clear pre-existing clippy lints under
`-D warnings` (stable 1.98). The test/clippy fixes are repo-wide CI debt unrelated to the feature,
folded in here to unblock the pipeline.

## Definition of done

- [ ] `security_scan` v1 runs end to end, records deduped findings with file+line evidence, and delivers to
  the configured outputs.
- [ ] The scanner runner is isolated from the test runner; `run_allowlisted_commands` is unchanged.
- [ ] Missing scanners fail closed with a sanitized error.
- [ ] Adversarial review on the security-surface PR passed.
- [ ] Operations doc updated. Phase 2 (`security_dast`) explicitly deferred.
