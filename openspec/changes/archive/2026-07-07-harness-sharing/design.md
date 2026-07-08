# Design: Harness Sharing

## Technical Approach

Add a first-slice Harness Library as first-class org-scoped data, not memories. The backend owns catalog/version/review/approval records and immutable manifest download; local MCP/CLI tools own file diff and installation after explicit user approval. This follows existing Rust/Axum + SQLite patterns: migration in `migrations.rs`, DTOs in `models/types.rs`, SQL helpers in `db/queries.rs`, route module wired from `api/router.rs`, and React/Vite admin pages using `client.ts`, `types.ts`, nav, route, table/modal/download patterns.

## Domain Model and DB Approach

Access patterns:
1. List visible published harnesses by org/project/status/target.
2. Fetch one harness with latest or exact version.
3. Create draft, publish immutable version, revoke version.
4. Download exact manifest hash after approval check.
5. Store redacted config review snapshots and redaction reports.

Migration creates:

| Table | Key fields | Invariants |
|---|---|---|
| `harnesses` | `id`, `org_id`, nullable `project_id`, `slug`, `name`, `visibility`, `status`, `created_by` | `UNIQUE(org_id, slug)`, FK org/project/user |
| `harness_versions` | `id`, `harness_id`, `version`, `manifest_json`, `manifest_hash`, `published_at`, `revoked_at` | `UNIQUE(harness_id, version)`, immutable once published |
| `harness_install_approvals` | `id`, `org_id`, `user_id`, `harness_version_id`, `target_tool`, `status`, `approved_at`, `metadata_json` | exact version + hash required before download/install |
| `harness_config_reviews` | `id`, `org_id`, `user_id`, `source_tool`, `redacted_config_json`, `redaction_report_json`, `content_hash`, `status` | raw config is never stored |

Indexes: `(org_id, status, project_id)`, `(harness_id, version)`, `(org_id, user_id, status)`, and `(org_id, source_tool, created_at)`.

## Architecture Decisions

| Decision | Alternatives considered | Rationale |
|---|---|---|
| Dedicated tables | Memories/collections/conventions | Harnesses need typed manifests, version immutability, approval state, and auditability. |
| App-level project visibility | Per-harness grants now | Existing `project_members` visibility is enough for slice one; grants can be added later without changing manifest format. |
| Backend download only | Backend installation | Server cannot safely mutate local Claude/Codex/OpenCode files. Local tools must show diffs and ask before apply. |
| Store redacted reviews only | Upload raw configs then redact | Raw local configs may contain secrets, paths, hostnames, and tokens; upload boundary must already be redacted. |

## Backend API Shape

Add `apps/backend/src/api/harnesses.rs` and route it in `api/router.rs`:

```text
GET    /v1/harnesses?target=&project_id=&status=
POST   /v1/harnesses
GET    /v1/harnesses/:id
PATCH  /v1/harnesses/:id
POST   /v1/harnesses/:id/versions
POST   /v1/harnesses/:id/publish
GET    /v1/harnesses/:id/versions/:version/download
POST   /v1/harnesses/:id/versions/:version/approval
POST   /v1/harness-config-reviews
GET    /v1/harness-config-reviews/:id
GET    /v1/harness-recommendations?target=&project_id=
```

Permissions: `harness:read`, `harness:write`, `harness:download`, `harness:install`, `harness:review_config`. Use `require_permission`; non-privileged users only see org-wide rows plus project rows where they are `project_members`. Audit mutating actions through blanket audit, and self-log `harness.downloaded`, `harness.install_approved`, and `harness_config_review.shared` with safe metadata only: ids, target, hash, status, never manifest bodies or config content.

## Data Flow

```text
Admin UI ──create/publish──> Axum harness API ──> SQLite tables
Agent/MCP ──recommend──> metadata only
User approves ──> approval row ──download exact hash──> Local installer
Local installer ──diff + explicit confirm──> local config files
```

## Admin UI Surface

Create `apps/admin/src/pages/Harnesses.tsx`, add `/harnesses` route, sidebar item gated by `harness:read`, and `client.ts`/`types.ts` methods. First slice UI: list filters, create draft modal, publish version modal with manifest JSON validation, detail drawer showing provenance/hash/compatibility, download button requiring an approval confirmation modal, and config review upload form that displays redaction report before submit.

## MCP / Agent Contract

Recommendations return only `{harness_id, version, name, description, targets, manifest_hash, approval_required: true, download_url}`. Agents MUST NOT auto-download or install. They may ask the user to approve, then call approval/download. Install remains external CLI/MCP behavior.

## Redaction and Provenance Rules

Manifests include `schema_version`, `targets`, `components`, `compatibility`, `provenance`, `security`, and `manifest_hash`. Backend verifies provided hash against canonical JSON. Config reviews require local redaction before upload and store `redaction_report_json`; reject payloads marked `secret_scan_status=failed`.

## File Changes

| File | Action | Description |
|---|---|---|
| `apps/backend/src/api/harnesses.rs` | Create | Handlers and tests. |
| `apps/backend/src/db/migrations.rs` | Modify | Add harness tables, indexes, seeded permissions. |
| `apps/backend/src/db/queries.rs` | Modify | Harness CRUD, visibility, approval, review queries. |
| `apps/backend/src/models/types.rs` | Modify | Harness DTOs and manifest contracts. |
| `apps/backend/src/api/router.rs` | Modify | Wire routes. |
| `apps/admin/src/types.ts` | Modify | Harness/admin types. |
| `apps/admin/src/api/client.ts` | Modify | Harness API methods. |
| `apps/admin/src/App.tsx`, `components/Layout.tsx`, `pages/Roles.tsx` | Modify | Route, nav, permissions. |
| `apps/admin/src/pages/Harnesses.tsx` | Create | Harness Library UI. |
| `docs/CLAUDE_CODE_PLUGIN.md`, `docs/CURSOR_PLUGIN.md` | Modify | Approval-first setup guidance. |

## Testing Strategy

| Layer | What to Test | Approach |
|---|---|---|
| Backend unit | hash verification, visibility, status transitions | Rust tests in queries/API modules |
| Backend integration | permissions, audit, download approval boundary | Axum request tests |
| Admin | list/create/publish/download-confirm flows | Vitest + Testing Library |

## Migration / Rollout

Ship behind navigation permission. Seed harness permissions for admins/super users; custom roles opt in. No destructive migration. Roll back by hiding nav/routes and leaving data inert.

## Open Questions

- [ ] Exact external MCP/CLI endpoint names for local installer implementation.
