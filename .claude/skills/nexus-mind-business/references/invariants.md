# Invariants

Breaking one of these is a critical bug, whatever the issue asked for.

## Tenancy

- The tenant is `AuthContext.org_id` from the auth middleware. Never take `org_id` from a body, query string or Factory payload (`factory/contracts.rs` keeps it out by design).
- Every query that reads or writes tenant data filters by `org_id`; project-scoped data also checks membership (`helpers.rs`).
- Projects are never created implicitly: an unknown project is `project_not_found`; `default` maps to the org-shared bucket (NULL `project_id`).
- Test it: a second org's key must not see or change the row (`scripts/test-e2e.sh` pattern, `nm_demo_techstartup_admin`).

## Audit

- The `audit` middleware (`api/middleware.rs`) logs every successful POST/PUT/PATCH/DELETE on protected routes. Handlers that write their own audit row are listed in `AUDIT_SKIP_PATTERNS`; add yours there only if you log it yourself.
- Memory writes go through `store_memory_with_audit` (one path, so migrated memories are audited too). Searches are audited with a sample of results.
- Gap: `/internal/*` (backoffice) actions are not audited (see `gaps.md`). Don't copy that pattern.

## Secrets

- Tokens at rest: AES-256-GCM in `apps/backend/src/crypto.rs`, key `NEXUSMIND_TOKEN_ENCRYPTION_KEY`. No key → don't persist the secret; never fall back to plaintext.
- Workspace secrets (`db/workspace_secrets.rs`, kinds `env` and `npmrc`, ≤64 KB) are write-only for people: the API returns names and hosts, never values. A runner gets plaintext only for a step it currently leases, never for a review step.
- Sandboxes hold no credentials (an HMAC run token only); the egress proxy (`factory/egress.rs`) injects them.
- API keys are stored hashed; a raw key is shown once.
- `.nexusmind.yaml` never carries keys, tokens, commands or absolute paths; its capability profiles only narrow MCP tools (`docs/REPOSITORY_CONFIG.md`).

## Memory

- With `topic_key`, store **updates** the row matching `(org_id, topic_key)` — org-wide, not per project — and bumps `revision_count`; 200 on update, 201 on create. Without it, store always inserts.
- `type` is free text (the protocol list lives in `CLAUDE.md`); `normalized_hash` is stored but doesn't reject duplicates.

## SDD

- Artifacts ≤ 1 MiB (`SDD_MAX_ARTIFACT_BYTES`), checked before writing.
- Identity: (change, kind, capability). Revisions are append-only; saving content identical to the latest revision is a no-op.
- `phase` is advisory; it never blocks a save. Archive is a soft delete.

## Factory

- Human approvals bind to `(run_id, action, change_revision)`; a new revision needs a new approval. Approving needs `factory_policy:write`.
- Action policies (fix, publish, open_pr, reply, close, approve, merge, deploy, notify, recover) default to `manual`. If the decision model fails or is unsure, the item is held. The `always` mode (v111) is flagged dangerous in code.
- The merge risk floor in `automation/merge_gate.rs` is not configurable from run config and must never widen eligibility.
- Publication is credential-free for runners: the control plane recreates verified commits via GitHub's Git Database API and never merges, approves or deploys (`factory/workflow_publication.rs`).
- Intake text is PII-redacted before reaching a model (`factory/redact.rs`); the target repo comes from the source config, never from the item.

## Schema

- Migrations are `run_vN` functions guarded by `PRAGMA user_version`, called in order from `run_all` in `db/migrations.rs` (current: 111). Add the next number, make it transactional, rebuild the table for CHECK changes (`*_v111` pattern). Never edit a shipped migration: production databases already ran it.

## Contracts

- MCP tools and plugins consume `/v1/*`. Response changes are additive; renaming or removing a field needs a coordinated change in `nexusmind-mcp`.
