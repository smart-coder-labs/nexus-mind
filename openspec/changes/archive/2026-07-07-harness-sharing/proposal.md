# Proposal: Harness Sharing

## Intent

Add a safe NexusMind harness library so authorized users can publish, review, download, and explicitly install reusable AI tooling setups for Claude, Codex, and OpenCode without silent local configuration changes or secret exposure.

## Scope

### In Scope
- First-class harness catalog with org/project visibility, versions, manifests, hashes, and audit events.
- Dedicated harness permissions for read/write/download/install/config-review actions.
- Admin UI flows to create, publish, inspect, and download harness manifests.
- Agent-facing recommendation/download contract: agents may recommend relevant harnesses, but download/install requires explicit user approval.
- Redacted Claude config review/share capability with preview before upload.

### Out of Scope
- Public marketplace distribution.
- Remote mutation of local Claude/Codex/OpenCode files by the backend.
- Full installer implementation outside this repo; local CLI/MCP integration is a dependency.
- Raw shell profile upload or unredacted secret sharing.

## Capabilities

### New Capabilities
- `harness-library`: Store, version, publish, list, inspect, and download harness manifests with visibility, provenance, compatibility targets, and audit trails.
- `harness-install-approval`: Define explicit approval, policy, audit, and immutable manifest-hash requirements before any download or local installation.
- `harness-config-review`: Allow users to upload/share redacted Claude configuration snapshots for review, with deterministic redaction reports and user preview.

### Modified Capabilities
- None.

## Approach

Use first-class backend tables instead of memories/collections. Persist harnesses, versions, installation approvals, and config reviews under `org_id`, with optional `project_id`. Add REST endpoints, typed admin client methods, and a Harness Library UI using existing list/modal/download patterns. Backend returns manifests and approval metadata only; local tools handle file diffs and installation after user confirmation.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | New | Harness tables, indexes, role permission seeds. |
| `apps/backend/src/models/types.rs` | New | Harness, manifest, approval, config-review DTOs. |
| `apps/backend/src/db/queries.rs` | New | Org/project-visible harness queries. |
| `apps/backend/src/api/router.rs` | Modified | `/v1/harnesses*` routes. |
| `apps/backend/src/api/helpers.rs` | Modified | Harness permission checks. |
| `apps/admin/src/api/client.ts` | Modified | Harness API client methods. |
| `apps/admin/src/types.ts` | Modified | Admin harness types. |
| `apps/admin/src/pages/*` | New | Harness Library and config review screens. |
| `docs/*PLUGIN*.md` | Modified | Document approval-first setup flow. |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Secret leakage from config sharing | High | Redact locally, show preview, store redaction report. |
| RCE through MCP/hooks/CLIs | Med | Manifest provenance, hashes, policy gates, no silent install. |
| Permission leakage across projects | Med | `org_id` + optional `project_id` checks and tests. |

## Rollback Plan

Disable harness routes/UI navigation and revert migrations in a compatible rollback. Existing data remains isolated; published manifests can be archived/revoked by status without deleting audit history.

## Dependencies

- Local CLI/MCP installer/exporter must provide redaction, diff preview, and explicit apply confirmation.

## Success Criteria

- [ ] Authorized users can publish and download immutable harness manifests.
- [ ] Agents recommend harnesses without downloading/installing until user approval.
- [ ] Claude config sharing stores only redacted reviewed content.
- [ ] Harness actions are permission-checked and audited.

## Assumptions / Deferred Questions

- Private org library first; visibility enum allows future public sharing.
- Start Claude config review with settings, MCP servers, and hooks only.
