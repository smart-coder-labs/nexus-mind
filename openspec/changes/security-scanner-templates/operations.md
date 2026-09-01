# Operations — `security_scan` runtime provisioning (Phase 1)

**Change:** `security-scanner-templates`
**Project:** `nexus-mind`

The `security_scan` template is **worker-driven**: the backend worker runs the scanners itself under a fixed
allowlist (`run_security_scanners` → `run_scanner_capture` → `spawn_capture` in `automation/worker.rs`), so
the scanner binaries MUST be present on the runtime server (the same host that runs the colocated worker /
headless Claude Code). A missing binary fails the run closed with `scanner_unavailable` — it never silently
passes.

## Required binaries

| Tool | Purpose | Install |
|---|---|---|
| `semgrep` | SAST | `pipx install semgrep` (or `pip install semgrep`); or the official Docker image `semgrep/semgrep` |
| `osv-scanner` | SCA (npm/cargo/pip/… via lockfiles) | `go install github.com/google/osv-scanner/cmd/osv-scanner@v1` or a pinned release binary |

Both must be on the worker process `PATH`. `restrict_test_environment` preserves `PATH` for spawned
scanners, so a system-wide install or a directory added to the service `PATH` both work.

## Version pinning (recommended: Docker image)

Pin exact versions so findings are reproducible across runs and don't drift when a scanner ships new rules:

- Bake `semgrep==<x.y.z>` and `osv-scanner <x.y.z>` into the worker's runtime image, OR
- Install pinned versions in the host and record them in the deploy manifest.

Semgrep's bundled rulesets (`auto`, `p/ci`, `p/owasp-top-ten`) resolve against the Semgrep registry and may
require network egress on first use; a fully offline runtime should pin a checkout-relative rules file via
`sast.ruleset` instead (validated by `validate_semgrep_ruleset`).

**Ruleset default:** `sast.ruleset` defaults to `p/ci`, NOT `auto`. `auto` contacts the Semgrep registry and
emits pseudonymous telemetry; `p/ci` is a curated offline-capable ruleset, which is the safer default for a
security tool. Admins may still select `auto` explicitly.

**Symlink trust:** a configured checkout-relative ruleset path is trusted. Because the checkout contents are
attacker-influenced (whoever controls the scanned repo), a repo-controlled symlink at that path could point
semgrep at an out-of-tree file. The blast radius is limited — the target must still be valid Semgrep rules
YAML or semgrep errors out, and any leaked content trips the output secret-canary — but operators pinning a
repo path should treat it as trusted input.

## Verifying the install

```
semgrep --version
osv-scanner --version
```

Run the ignored E2E smoke once the binaries are present:

```
cargo test --lib -- --ignored security_scan_e2e_finds_a_planted_semgrep_hit
```

## Failure modes (surfaced on the run timeline)

| Code | Meaning | Operator action |
|---|---|---|
| `scanner_unavailable` | Binary not found on PATH | Install/repair the scanner on the runtime |
| `scanner_timeout` | A scanner exceeded `SECURITY_SCANNER_TIMEOUT_SECS` (900s) | Investigate repo size / ruleset cost |
| `command_not_allowlisted` | argv[0] not in the scanner allowlist | Internal bug — argv must come from `security_scan::build_*_argv` |

## Security notes

- The scanner runner is intentionally **separate** from the package-manager test runner
  (`run_allowlisted_commands`), so widening one never widens the other.
- Scanner argv is built only by `security_scan::build_*_argv` with fixed flags; the only injected values are
  the validated ruleset and the host-controlled scan root.
- Output passes the existing secret-canary scan in `evaluate_structured_result` before any finding is
  persisted or delivered.
