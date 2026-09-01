# Operations — `security_dast` runtime provisioning (Phase 2)

**Change:** `security-dast-template`
**Project:** `nexus-mind`

`security_dast` is worker-driven: the backend worker runs Nuclei itself under a fixed allowlist
(`run_dast_scan` → `run_dast_capture` → `spawn_capture`). The binary MUST be present on the runtime server,
which becomes the **origin of the scan traffic**.

## Required binary

| Tool | Purpose | Install |
|---|---|---|
| `nuclei` | Active DAST | `go install github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest`, or a pinned release binary; keep templates updated out-of-band |

Must be on the worker `PATH` (preserved by `restrict_test_environment`). Pin the version and template
version for reproducible findings. A missing binary fails the run closed (`scanner_unavailable`).

## Authorization (mandatory)

- Only registered `web_application` targets are scanned. Register them via the targets API; the URL lives in
  `target.config.url`. There is no free-form scan URL.
- **Only scan environments you own and are authorized to test.** Default to **staging/dev**, never
  production, unless you have explicit authorization and a maintenance window.
- Keep a written authorization record per target (owner, environment, date). Treat prod targets as
  opt-in with a human check.

## Network egress (strongly recommended)

Restrict the runtime's outbound network to the authorized target hosts (firewall/egress allowlist). Without
it, the per-finding host scope guard is the only barrier — defense in depth wants both.

## Bounds

- `severity` filter (default `medium,high,critical`), `rate_limit` (default 20 req/s, clamped ≤500),
  per-target timeout `DAST_SCAN_TIMEOUT_SECS` = 1500s, and `MAX_DAST_INVOCATIONS` = 8 targets/run.
- `-no-interactsh` disables the out-of-band interaction server (no third-party callback infra).

## Verify

```
nuclei -version
```

Then create a `security_dast` agent, register a `web_application` target pointing at a staging URL you own,
and run it; findings should carry request/response evidence and be scoped to that host only.
