# Issue resolver: mandatory clean-PR gate

`github_issue_resolver` no longer treats an empty `verification_commands` list as permission to publish without checks. This applies to every executor and to Continue/resume, not only Claude or specialist runs.

## What runs

Before a push/PR, the worker stages the complete change (including new files and prior checkpoints), checks secrets/diff budgets/excluded paths, and commits it. Verification then runs against that exact final SHA. Set `verification_isolation: "sandbox"` to require an independent commands pod, including when Pi/Nexus itself authenticates locally. The sandbox deployment default also selects commands pods; the legacy default remains local if `FACTORY_ISOLATION_DEFAULT` is unset. Local mode is unsafe for untrusted repository code: environment clearing is not a filesystem/network sandbox. Invalid isolation is an error, never a silent host fallback. Only successful, complete receipts plus a persisted SHA-bound report allow publishing.

Automatic plans inspect both the pinned base and candidate:

- Node packages: frozen dependency installation when a lockfile exists; declared `test`/CI/unit/integration/E2E/smoke scripts, build, lint and typecheck scripts, including nested packages.
- Rust: workspace tests/build across targets, Clippy with warnings denied, and formatting checks.
- Other stacks: explicitly configured tests/build commands for their platform runner/toolchain.
- Whitespace and source-tree integrity checks always run. Build/lint/test commands may not silently auto-fix tracked source and then claim they verified the published tree.

Deleting a script does not remove it from the plan. Replacing a pre-existing verification script requires a new explicit control-plane plan, not an agent-authored repository setting. Missing SDKs/dependencies/scripts, command failures/timeouts, missing results, stale heads or inability to persist evidence block the PR. Existing baseline failures are reported/blocked, never silently ignored.

The integrity guard checks committed blobs and executable modes. Its expected manifest is SHA-256-bound in exec argv, so tests cannot simply overwrite expected hashes. The private-registry broker rebinds only its own exact lockfile/.npmrc URL substitutions before execution; model credentials are absent from the commands pod.

## Project overrides

Use immutable agent configuration, not run input or repository instructions:

```json
{
  "verification_isolation": "sandbox",
  "verification_plan": {
    "setup": [["npm", "ci", "--no-audit", "--no-fund"]],
    "tests": [["npm", "run", "test:ci"]],
    "build": [["npm", "run", "build"]],
    "lint": [["npm", "run", "lint"]],
    "typecheck": [["npm", "run", "typecheck"]]
  },
  "required_checks": ["CI / test", "CI / build"]
}
```

This lets a monorepo owner select reviewed aggregate commands instead of an excessively large per-package plan. Existing `verification_commands` remain additional checks and can explicitly name tests/build for an otherwise unknown stack. Commands are argv arrays, not shell strings. Empty required roles and opaque `ci` commands do not count as proof of both tests and build.

A genuinely non-applicable role requires an explicit reason in `verification_not_applicable`, e.g. `{"build":"Documentation-only repository; no compiled artifact"}`. Such exemptions cannot remove discovered applicable checks and are shown as **not applicable**, never as successful tests/builds.

The execution image must contain the project's tools. `scripts/factory/Dockerfile.verify` can extend a reviewed backend image with Rust/rustfmt/Clippy and native build prerequisites; configure the resulting digest as `FACTORY_SANDBOX_IMAGE`. Build it with an explicit reviewed base, for example:

```sh
docker build --build-arg BACKEND_IMAGE=YOUR_REVIEWED_BACKEND_IMAGE \
  -f scripts/factory/Dockerfile.verify -t YOUR_REGISTRY/nexusmind-verify:VERSION .
```

For Kubernetes, push it to your approved registry and configure `FACTORY_SANDBOX_IMAGE=YOUR_REGISTRY/nexusmind-verify@sha256:MANIFEST_DIGEST` (not a local Docker config ID). The Rust image/toolchain was locally built and smoke-tested with tests, build, Clippy and formatting in a network-disabled, non-root, read-only container; that does not claim a deployed Kubernetes drill.

Other SDKs/package-manager versions and approved artifact registries remain project-specific. A missing tool blocks; there is no unsafe host fallback for a sandboxed run.

## PR/CI behavior

- PR bodies contain the verified head and actual command results/applicability.
- CI requirements combine configured names with GitHub branch protection/rules.
- Required checks must conclude `success` on the same SHA. Missing/pending jobs wait up to the configured timeout; skipped/neutral/failed jobs do not count as green.
- `verification_ci_timeout_seconds` defaults to 600, bounded to 5–1800.
- A moved PR head, cancelled run, failed CI or timeout blocks success. If no CI requirements are configured/discovered, the result says `not_configured`, not “remote CI passed”. Native pre-publication tests/build are still mandatory.

Explicit no-op completion also verifies the clean committed tree before an issue can be declared resolved. A no-op accompanied by edits/WIP relative to the pinned base blocks instead of silently closing the issue.

Checkpoints use separate `nexusmind/checkpoint-…` backup branches and do not open/update PRs. They receive a secret-diff check, and a checkpoint push is refused if someone manually opened a PR on its head. A stopped attempt remains resumable, but no unchecked partial PR is created. Continue reads new checkpoints or legacy WIP branches and returns through the same final-commit gate. It does not keep pushing unverified progress into a legacy partial PR head.

In the new workspace workflow, the default `verify` phase requires named `tests` and `build` evidence. Publication also checks those roles in its verified ancestry; a browser-only QA pass cannot stand in for a build/unit-test gate. Check adapters discard generated work and reject tracked source mutation; modifying existing verification scripts requires explicitly trusted independent local checks.
