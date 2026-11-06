# Fix SDK permissions and consolidate workspace workflows

## Goal / What

Resolve the complete-workflow setup block and leave a single correct Kasymir
workflow definition, as explicitly requested by the user. Preserve execution
history and the newly published `kasymir-code-review-pr` harness.

## Root cause / Why

The active release directory's `Dockerfile.agent` had mode **0664**. The managed
SDK provisioner correctly refuses group/world-writable recipes. This raised
`runtime_image_provision_failed` before using the already installed, correctly
labelled SDK image. Runtime revision 23 therefore stayed unapplied, without Pi
or backend model capabilities, and could not run a complete workflow.

The newest definition named `Final Workflow` contained **only preparation**;
it was not a complete delivery graph. Four other definitions were still listed.

## Accomplished / Where

- Removed group/other write permissions from the operator-owned active runner
  directory: `/home/ubuntu/.local/lib/nexusmind/runner-5b05955` (`chmod -R go-w`).
  The SDK recipe is now 0644. Content hashes are unchanged.
- Verified the existing SDK image and recipe label:
  `sha256:7400ba364509574839c96cc5df264e856a603ffe47eea2ba5c5adc7a6cba1463`.
  No image rebuild, new credentials, model call or network grant was needed.
- Confirmed the runner applied runtime revision **23**. API reports `ready=true`,
  `missing=[]`, `error=null`, with Pi/backend and automatic environments available.
- Confirmed Factory remains enabled and backend publication is authorized for
  this organization under `CesarRuizBotAgents`. Publishing is still draft-only,
  with all existing verification/approval policies intact.
- Kept the existing complete definition
  `73791cbf-0095-4717-930b-f2c6a22549e6`, named
  `Kasymir · Oracle · tarea a draft PR revisado`, **unchanged**: 12 phases,
  $20 budget, up to two repair cycles, configured write scope, tests/build,
  environment, functional/design/security QA, evidence gate, draft publication,
  and independent read-only review. Inventory scope and DAG were checked.
- Deleted the other **four definitions**, scoped to the Kasymir organization and
  workspace. There were no intake subscriptions to remap or delete.
- Preserved all **five execution rows** (three succeeded, two blocked), their
  snapshot bytes, statuses, costs, commit heads, phases, artifacts, approvals,
  publications, and prior events. None were restarted or resumed.
- To satisfy definition foreign keys without deleting history, changed only the
  historical runs' `definition_id` association to the canonical definition.
  Appended `run.definition_consolidated` events recording each original definition
  ID and the private backup name. The original executed definitions remain in
  their immutable run snapshots; they were not rewritten to the new graph.
- Confirmed exactly one definition remains and `PRAGMA foreign_key_check` is clean.
  The new harness and its published version remain intact.

## Backup / Learned

Verified private SQLite backup before consolidation:
`/data/maintenance-backups/before-workflow-consolidation-20261106T085149Z.db`.
It includes the original definitions and associations. No schema migration or
production migration runner was invoked. Do not restore this database over newer
writes without an explicit recovery plan.

**Deployment gotcha:** Git archive extraction can produce group-writable files
under the remote umask. After extracting a runner, remove group/other write bits
before activation, and verify recipe mode/owner. Check workspace runtime readiness,
not only service liveness, advertised protocol capabilities or backend health.
Never remove the provisioner's trusted-recipe permission check to mask this issue.

## Next Steps

Refresh the admin; only the complete canonical definition should be selectable.
No new task execution or successful application QA is claimed. The two historical
blocked runs remain blocked pending an explicit operator action. Isolated extra
MCP transport provisioning is a separate, still-existing requirement for steps
opting into the per-agent resource runtime.

The user's screenshot was a local macOS path, unavailable on the Linux operator
machine. Diagnosis above uses live database/runtime state, not a claim to have
viewed that image.
