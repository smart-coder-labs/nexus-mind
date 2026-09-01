# Contracts — `security_dast` (Phase 2, frozen)

**Change:** `security-dast-template`
**Project:** `nexus-mind`

Enforced by `apps/backend/src/automation/security_dast.rs` (pure) + the `run_dast_scan` worker path.

## C1 — Authorized target

- The scan target is a registered `AutonomousAgentTarget` of kind `web_application`, `enabled = true`,
  belonging to the agent's definition (materialized into `config["targets"]` at claim time).
- The scan URL is `target.config.url` and **nothing else** — never a free-form run-input field.
- `config.target_name` (optional) restricts the run to the named target; otherwise every enabled
  `web_application` target is scanned (capped at `MAX_DAST_INVOCATIONS = 8`).
- URL validation (`parse_http_url`): scheme must be `http`/`https` (case-insensitive); host required;
  embedded credentials (`user:pass@`) rejected; host charset `[A-Za-z0-9.-]` (IPv6 literals rejected in v1).
- No authorized target → run fails closed (`no_authorized_target`).

## C2 — Host scope guard

Every finding's `matched-at` (or `host`) must resolve to the **authorized host** (exact, case-insensitive,
port-agnostic). Anything on a different host — e.g. a redirect to `evil.com` or a subdomain — is dropped by
`map_nuclei_jsonl`. `sub.host` ≠ `host`.

## C3 — Nuclei argv (fixed flags; only validated slots injected)

```
nuclei -u <TARGET_URL> -jsonl -silent -no-interactsh -disable-update-check \
       -disable-redirects -rate-limit <RL> -timeout <T> -severity <SEV>
```

`-disable-redirects` is a **traffic-level** scope guard: without it, a template that follows redirects would
chase an attacker-influenced `3xx` from the target to another host (blind SSRF to internal endpoints), and
the finding-level host guard (C2) only drops the record after the request was already sent. Disabling
redirects keeps every request on the authorized host. Register the exact final URL (e.g. the `https://`
form) since http→https redirects are no longer followed.

- `<TARGET_URL>`: the authorized URL (re-validated in `build_nuclei_argv`).
- `<SEV>`: comma-separated subset of `info,low,medium,high,critical` (validated); default
  `medium,high,critical`.
- `<RL>`: requests/sec, clamped `[1, 500]`, default 20.
- `-no-interactsh` disables the out-of-band callback server (no external infra); `-disable-update-check`
  stops phone-home. Report is read from **stdout**.
- Program allowlist = `{ nuclei }`. Anything else → `command_not_allowlisted`.

## C4 — Finding contract

Agent output is the same single-JSON-object shape as `security_scan`
(`{summary, findings:[{title, severity, summary, fingerprint, evidence}]}`), evaluated by
`evaluate_structured_result` (summary required, ≤100 findings, valid title/severity, secret-canary).

DAST evidence:
```json
{
  "kind": "dast", "engine": "nuclei",
  "template_id": "...", "matched_at": "https://host/path",
  "type": "http", "request": "...", "response": "...", "curl_command": "...",
  "description": "...", "cwe": ["..."], "cve": ["..."], "reference": ["..."]
}
```

Fingerprint: `sha256("dast" | template_id | matched_at)` — stable across re-runs for the same template+URL.

## C5 — Failure taxonomy

| Condition | Error | Outcome |
|---|---|---|
| No enabled web_application target | `no_authorized_target` | fail closed |
| Target url missing/invalid scheme/host/creds | `target_url_*` | fail closed |
| Program not `nuclei` | `command_not_allowlisted` | reject before spawn |
| Binary missing | `scanner_unavailable` | fail closed |
| Per-target timeout (`DAST_SCAN_TIMEOUT_SECS`=1500) | `scanner_timeout` | fail closed |

All errors are sanitized before reaching events/findings/deliveries.
