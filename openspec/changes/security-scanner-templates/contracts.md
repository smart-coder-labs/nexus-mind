# Contracts — `security_scan` (Phase 1, frozen)

**Change:** `security-scanner-templates`
**Project:** `nexus-mind`
**Covers tasks:** 0.2 (agent output contract + fingerprints), 0.3 (argv templates + severity map).

These are frozen before PR 1 so the runner, the mapper, and the prompt are all built against one source of
truth. Any change here is a contract change and requires re-reviewing PR 1 and PR 2.

---

## C1 — Agent output contract

The agent returns **exactly one JSON object and nothing else** (same discipline as QA / Lead Generation).

```json
{
  "summary": "string — one line: what was scanned and the headline count",
  "findings": [
    {
      "title": "string",
      "severity": "info | low | medium | high | critical",
      "summary": "string — one line describing the issue",
      "fingerprint": "string — stable, see C2",
      "evidence": { }
    }
  ]
}
```

- `findings` MAY be empty (clean scan). Empty is success, not failure.
- More than 100 findings is a hard failure (`too_many_findings`) — inherited from
  `evaluate_structured_result`. The agent must cap/prioritize before returning.
- No prose, no markdown fences around the object.

### Evidence shape by source

SAST (Semgrep):
```json
{
  "kind": "sast",
  "engine": "semgrep",
  "rule_id": "string (check_id)",
  "path": "relative/path.ext",
  "start_line": 42,
  "end_line": 45,
  "snippet": "the matched lines (secret-redacted by the canary scan before persist)",
  "message": "rule message",
  "cwe": ["CWE-79"],
  "references": ["https://..."]
}
```

SCA (osv-scanner / npm audit):
```json
{
  "kind": "sca",
  "engine": "osv-scanner | npm-audit",
  "ecosystem": "npm | crates.io | PyPI",
  "package": "name",
  "installed_version": "1.2.3",
  "advisory_id": "GHSA-xxxx / CVE-xxxx / OSV id",
  "vulnerable_range": "string",
  "fixed_version": "1.2.4 | null",
  "manifest_path": "package.json | Cargo.lock | ..."
}
```

---

## C2 — Fingerprint formulas (stable dedupe key)

Fingerprints must be stable across runs so re-scans dedupe (increment `occurrence_count`) instead of
creating duplicate findings. Lowercase hex SHA-256 of a canonical, delimiter-joined string.

| Source | Formula | Rationale |
|---|---|---|
| SAST | `sha256("sast" \| rule_id \| path \| start_line)` | Same rule at same location = same finding. Line drift creates a new fingerprint; acceptable for v1. |
| SCA | `sha256("sca" \| ecosystem \| package \| advisory_id)` | Same advisory on same package = same finding, independent of installed version bumps. |

`|` is a literal `0x1f` (unit separator) join to avoid collisions from values containing delimiters.
The mapper computes the fingerprint; the agent echoes what the mapper produced.

---

## C3 — Fixed argv templates (closed flags)

The dedicated runner (`run_security_scanners()`) only accepts these programs and only these argv shapes.
The bracketed slots are the **only** injectable values, and each is validated before substitution. No
free-form flags, no shell.

> **Implemented variant (v1):** the report is captured from **stdout** (no `--output` file), and **SCA is
> osv-scanner only** — `npm audit` is deferred because osv-scanner reads npm/cargo/pip lockfiles natively.
> The allowlist is therefore `{ semgrep, osv-scanner }`. Reflected in `automation/security_scan.rs`.

### Semgrep (SAST)

```
semgrep --json --quiet --timeout <PER_RULE_TIMEOUT> --config <RULESET> <SCAN_ROOT>
```

- `<RULESET>` ∈ { `auto`, `p/ci`, `p/owasp-top-ten` } OR a repo-relative path under the checkout matching
  `^[A-Za-z0-9._/-]+\.ya?ml$` (validated; no `..`, no absolute paths). **Default `p/ci`** (not `auto`, which
  emits registry telemetry).
- `<SCAN_ROOT>` is always the checkout workdir root (`.`; host-controlled, never from config).
- `<PER_RULE_TIMEOUT>` is a fixed integer (default 30).
- The JSON report is read from **stdout**.

### osv-scanner (SCA)

```
osv-scanner --format json --recursive <SCAN_ROOT>
```

- No injectable flags beyond the fixed set. `<SCAN_ROOT>` as above. Report read from **stdout**.

### Runner invariants (asserted by tests)

- Program allowlist = { `semgrep`, `osv-scanner` }. Anything else → `command_not_allowlisted`.
- argv-only, `shell = false`, `current_dir = workdir`, env restricted via `restrict_test_environment`.
- Max 4 scanner invocations per run (`MAX_SCANNER_INVOCATIONS`).
- Non-zero exit is NOT a failure (scanners exit non-zero when they find issues); the captured stdout JSON
  is parsed regardless.
- Per-command wall-clock timeout (`SECURITY_SCANNER_TIMEOUT_SECS` = 900); on timeout → `scanner_timeout`.
- Binary absent (spawn ENOENT) → `scanner_unavailable` (fail closed, sanitized).

---

## C4 — Severity normalization map

Scanner-native severity → canonical `info | low | medium | high | critical`.

| Semgrep `extra.severity` | Canonical |
|---|---|
| `INFO` | `info` |
| `WARNING` | `medium` |
| `ERROR` | `high` |
| (Semgrep `extra.metadata.impact` HIGH, when present) | upgrade to `critical` |

| SCA (CVSS / advisory) | Canonical |
|---|---|
| CVSS 0.0 or none | `low` |
| CVSS 0.1–3.9 | `low` |
| CVSS 4.0–6.9 | `medium` |
| CVSS 7.0–8.9 | `high` |
| CVSS 9.0–10.0 | `critical` |
| GHSA/npm textual `low/moderate/high/critical` | `low/medium/high/critical` |

Unknown/unparseable severity → `medium` (never drop the finding).

---

## C5 — Failure taxonomy (typed, sanitized)

| Condition | Error | Run outcome |
|---|---|---|
| Scanner binary missing | `scanner_unavailable` | fail closed, no partial pass |
| Per-command timeout | `scanner_timeout` | fail closed |
| Program not in allowlist | `command_not_allowlisted` | reject before spawn |
| Non-object agent result | `result_not_object` | existing evaluator failure |
| >100 findings | `too_many_findings` | existing evaluator failure |
| Secret canary hit in output | `secret_canary_detected` | existing evaluator failure |

All error messages are sanitized (no absolute paths, no repo contents) before they reach events, findings,
or deliveries.
