# Agent runtime release — 2026-11-06 UTC

**Historical first runtime release. Superseded by application `5b05955`.**
Current verified deployment, runner v2, backup and remaining provisioning boundary:
[HARNESS_VALIDATION_FIXES.md](HARNESS_VALIDATION_FIXES.md).

## Running version

- Deployed application commit: `781ee89652ff795f0954bf6833deaa07f174d820`.
- Branch: `feat/agent-runtime-config`.
- PR: https://github.com/smart-coder-labs/nexus-mind/pull/413 (open, not merged).
- Base: `fix/workspace-output-diagnostics`, commit `dcf9842`; prior published runner fixes are retained.
- Successful release: https://github.com/smart-coder-labs/nexus-mind/actions/runs/42150149840.
- Backend, autonomous worker and egress image:
  `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:218644a02086c36a301544f88b421bf53a38edc31608cd50d326403b81cc487d`.
- Admin: https://admin.nexusmind.smartcoderlabs.com.
- Immutable Pages deployment: https://d7e55f77.nexusmind-admin.pages.dev.
- Runner: `/home/ubuntu/.local/lib/nexusmind/runner-781ee89/runner.py`.
- Service: `nexusmind-kasymir-runner.service`, drop-in `90-agent-runtime.conf`.

The original dirty `fix/admin-panel-tests` checkout was preserved. Integration and
release were performed in `/tmp/nexusmind-agent-runtime`; do not deploy the old
checkout over this release. No branches were deleted or merged. Integrate the
stacked PRs #412 and #413 before subsequent main deployments, or those deployments
can remove the live workflow/runtime changes.

## Verification

- All seven release jobs succeeded. CI: 1,866 backend unit tests passed (4 ignored),
  27 workflow/backup integration tests passed, all 96 Python tests passed with real
  container/relay checks, and 145 focused admin tests passed plus production build.
- The first pipeline stopped before deployment on an obsolete Pi test that
  required `--no-mcp`. The updated test retains read-only tools and all discovery
  prohibitions while allowing only the isolated runtime to opt into assigned MCPs.
  No failing checks were bypassed.
- Public `/v1/health`: HTTP 200, `status=ok`, `db=ok`.
- Backend and autonomous worker: 2/2 ready, zero restarts. Egress proxy: 1 ready.
- Public admin entrypoint: `/assets/index-S2GzbVIe.js`.
- Public JS bytes match the release artifact:
  - `WorkflowsPage-UFb4HpB8.js`: SHA-256 `2a0bac04e36c4bf314bc1bc6efb79194b2d7b87b9acc57802737e1576b2da96a`.
  - `AgentRuntimeEditor-CVsDhv7B.js`: SHA-256 `a01e07e2e158e9621b272a8c894f2f34d955b8aa8c41e964bcc892000aa07d88`.
- Runner switched only after backend/admin deployment. Its `runner.lock` was
  acquired before stopping the service, so an executing phase was not killed.
  Service active, zero restarts; Python module hashes match the release checkout.
- Existing Pi QA extension, model relay configuration, role-specific phase prompts,
  readonly mounts, private/ZIP exclusions and long socket support were retained.
- No live paid model invocation or agent/configuration mutation was used as a smoke test.

## Provisioning boundary

Autonomous worker readiness was checked without emitting credentials: the
NexusMind stdio configuration, executable, scoped key and Pi OAuth file are present.

**The isolated workspace runner does not yet have `nexusmind_mcp` configured.**
Additional per-step runtimes requiring that transport remain explicitly blocked
until a scoped, operator-approved MCP/proxy transport is provisioned. Existing
workspace defaults and legacy workflow definitions continue unchanged. No keys,
provider permissions, network grants, agent revisions or workflow definitions were
created/changed automatically. Extra credentialed MCPs in autonomous pods likewise
require an approved proxy route; remote marketplaces are not auto-installed.

Configuration reference: [AGENT_RUNTIMES.md](AGENT_RUNTIMES.md).

## Backup and rollback

- Verified online SQLite backup made by the release before rollout:
  `/data/pre-workflows-20261106T024130Z.db`.
- This feature requires no schema migration; no production migration job was invoked.
- Previous backend/worker/egress image:
  `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:3daa08cb99e68190309f569195b9bec24ead3acd5fb4d8443a9efeffc758b84e`.
- Previous runner: `/home/ubuntu/.local/lib/nexusmind/runner-dcf9842/runner.py`.
- Roll back application images/admin and point the runner drop-in to its prior
  version only under explicit operator recovery instructions. Do not restore an
  old database over newer production writes.
