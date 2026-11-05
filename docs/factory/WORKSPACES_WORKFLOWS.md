# Workspaces and durable delivery workflows

Implementation: workspace discovery, isolated agent/check execution, integrated services, versioned multi-repository change sets, verification gates, policy-controlled draft PRs, clean-context review, bounded repair, intakes and artifact UI. Database migrations: 93–95 (rebased above production's existing migrations 88–92).

External prerequisites are deliberately explicit: project-specific runtime images and QA commands, development credentials, a model endpoint/proxy, and suitable platform runners. This is not a guarantee that an arbitrary application is fully tested without configuring its acceptance criteria. Live model inference, production GitHub/Linear/Slack/Sentry connections, and mobile-device execution require operator credentials/infrastructure; local tests do not pretend otherwise.

## Execution model

```text
Linear / Slack / Sentry / manual task
  → prepare committed clones
  → independent investigator
  → independent regression-test author
  → independent implementer
  → tests + build + declared repository checks
  → integrated test services
  → functional / design / security QA
  → deterministic SHA-bound evidence gate
  → policy-controlled draft PR per changed repo
  → independent clean-context reviewer
  → accepted, or bounded repair → verification → update the same draft PRs → review
```

Every run snapshots an immutable definition and authorized workspace inventory. Each step has a new process/container/session, explicit dependencies, timeout, attempts, write scope and adapter. Read-only checks can fan out; writers are serialized against all other steps. A completion carries its input revision, fenced lease, exact resulting heads and stored evidence digests. Stale results cannot advance a run. Changed heads invalidate previous verification/approvals. Review sees code/diffs, task and QA/publication evidence, not the implementer's conversation or rationale.

Repairs preserve heads, feedback and cumulative cost. They invalidate affected descendants and reuse stable PR branches, but never exceed configured cycles/attempts. Infrastructure/configuration failures hold for an operator instead of triggering code repairs. Cancellation invalidates results and is polled during container execution; it cannot undo an already-completed external write.

## 1. Register your folders

Open **Factory → Workspaces & workflows** (`/factory/workflows`). Required permissions: `factory_policy:read` and, for changes, `factory_policy:write`.

Register a runner and save its one-time token in an owned `0600` file in a private directory. It is hashed in the database and cannot edit workflows, policies, approvals, secrets or other runners' inventory. Revocation and rotation are in the UI; rotation fences active work. Restart the runner after changing its credential.

Clone repositories yourself, for example:

```text
/home/you/work/kasymir/
  web/.git/
  backend/.git/
  mobile/.git/
  shared/.git/
```

Register that parent folder on the runner. Paths belong to the runner machine, not the browser/API host. Discovery accepts original clones, not symlink repositories or existing worktrees. Confirm selected repositories and their write permissions. Unconfirmed or changed inventories cannot start new runs; identical polling does not invalidate confirmation. Existing runs retain their immutable authorization snapshot; cancel them to withdraw in-flight access.

The runner makes separate private clones (`--no-local`), pins committed SHAs, removes their push remotes and leaves personal changes/untracked `.env` files untouched. Preparation is idempotent. There is no monorepo requirement. Git LFS materialization and submodule checkout are not provided automatically.

## 2. Configure a runner

Example private configuration (replace placeholders):

```json
{
  "api_url": "http://127.0.0.1:8080",
  "runner_token": "ONE_TIME_RUNNER_TOKEN",
  "allowed_roots": ["/home/you/work/kasymir"],
  "state_dir": "/home/you/.local/state/nexusmind/runner",
  "container_runtime": "docker",
  "allowed_egress_networks": ["nm-model-proxy"],
  "adapters": {},
  "workspace_profiles": {},
  "environments": {}
}
```

```sh
chmod 600 ~/.config/nexusmind/runner.json
python3 scripts/factory/runner.py --config ~/.config/nexusmind/runner.json
```

`--once` processes one tick. A successful tick is not a claim that the entire workflow passed; inspect its persisted status. One local file lock prevents concurrent controllers using the same state directory. Pending completions are journaled/fsynced before submission and reconciled after a disconnect. Expired leases require operator reconciliation, never blind replay of publication.

### Agent image and adapters

A supplied base agent image builds Python, Git, Node and the pinned Claude Code CLI:

```sh
docker build -f scripts/factory/Dockerfile.agent -t nexusmind-workflow-agent:local .
docker image inspect nexusmind-workflow-agent:local --format '{{.Id}}'
```

Use the resulting immutable `sha256:…` ID, never a tag, in runtime profiles. Images are never pulled automatically. Extend reviewed images with project dependencies/toolchains, browsers, scanners and offline rule/databases as needed.

An agent adapter:

```json
{
  "kind": "agent",
  "image": "sha256:REVIEWED_IMAGE_ID",
  "format": "claude",
  "role": "implementation",
  "command": ["claude", "--model", "sonnet", "--permission-mode", "bypassPermissions", "--allowedTools", "Read,Edit,Write,Grep,Glob,Bash,Task"],
  "network": "nm-model-proxy",
  "environment_refs": ["model"]
}
```

Register separate adapter names matching the template: `isolated-investigator`, `isolated-test-author`, `isolated-implementer`, `isolated-reviewer`. Give investigators/reviewers read-only tools; reviewers cannot receive environment refs. Each invocation has fresh ephemeral HOME and no repository CLI settings. Claude stream events/tool outputs are captured and redacted; fixed per-attempt model allocations bound retry spend. Generic JSON adapters/VM drivers must enforce the provided cost allowance themselves.

Named egress networks MUST be internal networks. Connect an allowlisting model proxy explicitly; raw internet/host networking is refused. Prefer short-lived broker credentials over provider keys. Services use a separate per-run internal network without host ports. Commands/checks get only explicitly selected development environment refs, never the runner/GitHub credential.

A deterministic check adapter (`isolated-verification`):

```json
{
  "kind": "checks",
  "image": "sha256:PROJECT_QA_IMAGE_ID",
  "checks": [
    {"name": "tests", "cwd": "web", "command": ["npm", "test", "--", "--run"]},
    {"name": "build", "cwd": "web", "command": ["npm", "run", "build"]},
    {"name": "lint", "cwd": "web", "command": ["npm", "run", "lint"]},
    {"name": "typecheck", "cwd": "web", "command": ["npm", "run", "typecheck"]}
  ]
}
```

Commands are operator configuration, not issue-provided shell strings. Each declared check executes and records its exit code/log. Missing commands/tools, timeouts, missing named results, source mutations during QA, or failures block publication. Existing package verification scripts cannot be changed/deleted by the implementation and then silently trusted. An operator can configure independent commands and explicitly authorize a script migration with `allow_verification_script_changes`; that flag is never read from repository content.

Images can preinstall Node dependencies under `/opt/…`; `dependency_links: {"web/node_modules":"/opt/web/node_modules"}` exposes them inside the container without adopting generated dependency trees into commits. Commands that generate tracked source must produce/commit it in implementation before final verification, not auto-fix during checks.

Configure functional/design/security adapters with real project QA commands (browser E2E, visual references/accessibility, API integration/authz, static/dependency/secret scans and authorized DAST). `network: "services"` reaches the integrated environment. Names must match each phase's `options.required_checks`. A justified `not_applicable_reason` is accepted only when the workflow operator explicitly enabled `allow_not_applicable`; skips never count as passes by default.

### Integrated environment

Under `workspace_profiles[WORKSPACE_UUID].services`, configure each service's pinned image, argv `command`, repository-relative `cwd`, `healthcheck` argv and `environment_refs`. Every service must become healthy. All services run the exact current change-set revision; source fingerprints are checked before/after QA. A new implementation tears down the old environment. Successful/cancelled/blocked runs, revoked runner credentials and expired service TTLs trigger cleanup.

Backend/database dependencies, fixture data and migration commands must be declared in reviewed service/check profiles. No production network or production data is assumed.

### Mobile and other platforms

Android, Flutter, Swift/iOS, desktop and hardware QA use platform-specific images or operator-installed VM/device sandbox drivers. An external adapter uses `driver: "vm"` and a single absolute `driver_command` executable, owned by root/the runner and not writable by other users, outside source roots. Host interpreters and repository-provided driver scripts are refused. The driver receives `--input`, `--work`, `--output`, `--timeout`; it must execute code inside its VM/device boundary and return the same result/artifact protocol. It does not receive host environment files. The operator is responsible for the driver's isolation/secret broker and SDK/device availability. An unavailable platform runner waits/blocks, never reports mobile QA passed.

## 3. Development `.env` files

Keep literal `KEY=value` files outside all source roots, owned by you with mode `0600`. Never source/evaluate them as shell scripts. Example local mapping:

```json
{
  "environments": {
    "WORKSPACE_UUID": {
      "model": "/home/you/.config/nexusmind/env/model-proxy.env",
      "backend": "/home/you/.config/nexusmind/env/kasymir-backend.env"
    }
  }
}
```

Only the selected adapter/service receives those values. They are not copied into repositories or sent to NexusMind. Known values/token patterns are redacted from uploaded text logs; generated code containing those credentials is rejected. Binary screenshots may contain application data: use disposable accounts/data and review your QA captures. Raw local logs remain in the private runner state directory; apply local retention/disk quotas appropriate to your threat model. Containers have CPU/RAM/PID/file-size/time restrictions, but bind-mounted disk space also needs filesystem quotas for hostile workloads.

## 4. Draft PRs and independent review

Select the workspace/template and assign installed adapters through the phase editor. Repository write scopes are selected from the confirmed inventory, not retyped URLs. Publishing needs an encrypted `target_secret` connector with metadata `purpose: factory_workflow_github`, an explicit `repositories` allowlist of GitHub `owner/repo` names, and scopes `target:use` plus `github:write`. Set its ID on the Publish phase. Credentials remain in the control plane.

The broker validates every changed repository, path, size, parent/tree/commit SHA and QA ancestry. Both `tests` and `build` evidence (or explicitly justified operator-approved applicability) are required. It reproduces the exact verified commits via GitHub's Git Database API, refuses a moved base, uses stable per-run branches, and reconciles uncertain writes. Multiple repos produce a linked change set of draft PRs; there is no atomic multi-repository merge and no automatic merge/deploy.

`publish` and `open_pr` are independent policies/approvals bound to the current revision. Missing policies default to manual; `never` cannot be overridden. Build/CI/dependency/agent/security-sensitive paths require a human. A new head invalidates previous approvals. Repository content cannot grant permissions or widen network/secrets.

Required GitHub checks come from protection/rules and optional Publish `options.github_required_checks`. They must actually conclude `success`; skipped, neutral, missing or failed checks are not green. While awaiting CI, the draft link is visible as `awaiting_ci`. Timeouts/failures hold the workflow, never mark it complete. Independent review then uses a fresh session, cannot write, and cannot accept with blocking/high/critical findings.

## 5. Intakes and autonomous starts

Connect Linear, Slack and Sentry to the same immutable workflow in the UI. Dedicated encrypted connectors need `target:use` and `<kind>:read`, with metadata `purpose: factory_workflow_intake`, `source_kind: linear|slack|sentry`. Existing dedicated Linear connectors can use `purpose: factory_workflow_linear`. Sentry also binds `base_host` to the validated upstream host. Intake credentials never reach a runner.

Polling is asynchronous, deduplicated and leased server-side, every five minutes while the runner is active. One outstanding task per workspace is selected by the normal poller. Manual **Take next assigned issue** is also available. Linear reads all bounded pages before sorting, urgent first/unprioritized last, stable creation-time/ID tie breaks; unresolved or unreadable blockers are excluded. Slack requires the configured approved reactions/users. Sentry uses its scoped project/query. Sources normalize to untrusted task data and do not require intermediary GitHub issues.

Manual starts explicitly authorize bounded implementation unless a policy forbids it. Scheduled starts honor `fix` policies and otherwise wait for approval. Optional Jev autonomy requires the existing explicit org data-consent allowlist/key, safe classes/floors, the shared daily start cap, and a matching criteria policy whose sole allow condition is `workflow_low_risk_intake_v1` with no unimplemented stop predicates. Unknown prose criteria hold for a human. Decision evidence is bound to the policy fingerprint and initial task/revision; policy changes cannot be bypassed by an old decision. No live intake/provider connection is claimed verified without credentials.

## 6. GitHub requested-review webhooks

See [GITHUB_REVIEW_WEBHOOKS.md](./GITHUB_REVIEW_WEBHOOKS.md). The UI can register a signed repository webhook using the backend's authenticated `gh` account and an enabled standalone PR reviewer. Webhook delivery auth is independent from posting reviews. Events are HMAC-verified, scoped, deduplicated and processed transactionally; stale heads are ignored and superseded reviews cancelled.

## Operations and evidence

The UI shows the complete phase graph, state, attempts, revisions, costs, repair cycles, PRs, candidate diffs, redacted tool/command logs, screenshots and other digest-bound artifacts. Artifact access requires org-scoped factory read permission; there are no public evidence URLs for these workflows.

PostgreSQL backup/restore includes workspace-workflow tables, binary artifacts, encrypted connectors and requested-review subscriptions. Keep the encryption key separately; restoring ciphertext without the original key cannot recover credentials. Restore revokes runners, invalidates active leases/approvals, disables intakes and holds unfinished runs. Rotate credentials and reconcile external writes before resuming. Local runner checkout/cache state is separate; back it up or retain the published branches if you need it after losing the runner disk.

Backup scope limitation: the existing PostgreSQL bundle does **not** include legacy autonomous-agent definitions/append-only history. Preserve standalone reviewer configuration/history with an offline SQLite/full-volume snapshot. PostgreSQL restoration into a database containing legacy append-only history can reject atomically; use a fresh database for that restore path and re-register standalone reviewers. This limitation is not a claim that legacy Factory backup is complete.

Validation includes real local HTTP + Python runner + Docker execution through investigation, regression authoring, implementation, integrated QA and clean review, plus SHA-exact/idempotent GitHub publication against a mock upstream, cancellation/restart/concurrency/repair/backup/permission tests. The supplied agent image was built and its CLI/tool versions checked. These tests use deterministic agent fixtures, not paid live model calls. Mobile VMs/devices and real external integrations still need deployment-specific acceptance testing. The existing full admin suite has unrelated Node/localStorage failures reproduced on clean HEAD; focused Factory tests and the production build are checked separately.
