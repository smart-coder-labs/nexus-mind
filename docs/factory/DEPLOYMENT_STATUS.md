# Factory workflows deployment — 2026-11-02

## Running release

- Admin: https://admin.nexusmind.smartcoderlabs.com/factory/workflows
- API: https://api.nexusmind.smartcoderlabs.com
- Release branch: `deploy/factory-workflows-v95`
- Deployed commit: `b35237e29626b8b083bbacc5b420ba1af5667617`
- CI/deployment: https://github.com/smart-coder-labs/nexus-mind/actions/runs/41518216168 (all jobs successful).
- Integration PR: https://github.com/smart-coder-labs/nexus-mind/pull/410 (draft, NOT merged).
- Backend/worker image: `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:0d3f6b4af74335b8cfab8a8a32fefc9529dacaf05d68a249e8bfd65c2b14c4cf`.
- Backend and autonomous worker: 2/2 ready, zero restarts at validation. Egress proxy: 1/1 ready.
- Live SQLite schema: **95**. New workflow migrations were renumbered **93–95** because main/production already had unrelated migrations 88–92.
- Existing PVC, secrets, credentials and deployment settings retained. Images are digest-pinned.

## Source warning

The original working tree (`fix/admin-panel-tests`) was 120 main commits behind. It was NOT reset/rebased or deployed directly. The release was integrated and tested in a separate worktree based on main `8e03010`, preserving newer production changes. The original uncommitted work remains intact.

**Integrate PR #410 before subsequent main deployments or allocating migrations 93–95 elsewhere.** Deploying the old working tree/main would remove these features and could conflict with the running schema. Do not copy just the migration numbers into the old branch: the intervening main migrations must also be present.

## Verification

- 1,850 backend unit tests passed (4 explicitly ignored), 22 workflow/backup integration tests passed.
- 45 Python tests, including real container workflows, passed in CI.
- 30 focused admin tests and production build passed.
- Public health endpoint returned OK. New authenticated endpoints return 401 without credentials; the public signed callback rejects unknown connectors with `webhook_not_found`.
- The public admin serves the new Workflows bundle and requested-review controls.
- Backend `gh` identity: `CesarRuizBotAgents`. Connector encryption is configured.

## Backups and rollback

Online SQLite backup before deployment: `/data/pre-workflow-release-20261102T180400Z.db`; another verified backup is made by the deployment job immediately before rollout.

Independent, private copies of the first backup:
- Oracle: `~/nexusmind-backups/pre-workflow-release-20261102T180400Z.db`.
- Operator machine: `~/.local/state/nexusmind/deploy-backups/pre-workflow-release-20261102T180400Z.db`, plus `.metadata.json`.
- SHA-256: `8303753e83b77ef8ce80e6cf7df23b1f7a6ab7600f7cb7a70f3921704c7670cc` (server/off-server match; SQLite quick check OK).

Do not stream a large database through `kubectl exec` without checking its final checksum: the first transfer was truncated despite a successful stream exit. It was replaced using verified `kubectl cp` plus SCP.

Previous backend/worker image: `ghcr.io/smart-coder-labs/nexusmind-backend@sha256:6d2d40d6008093ab8c745050b740d14a3f09dd424e4b3d2a94d99798403e601e`. Do not restore an old DB over new production writes without an explicit recovery plan.

## Operator acceptance testing

Connect a selected enabled/validated PR reviewer under **GitHub requested reviews**, using the public API URL above, then request `CesarRuizBotAgents` in GitHub. No repository webhook was registered or additional repository/model authority granted automatically by deployment. Workspace runners, development environments, project QA profiles and platform-specific images must still be configured explicitly. Live paid model inference/mobile device QA was not claimed tested.
