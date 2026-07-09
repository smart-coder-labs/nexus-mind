# Exploration: Harness Format Variants

## Current State

The `feat/harness-sharing` branch already adds a first-slice Harness Library with dedicated backend tables, admin UI, OpenSpec specs, and plugin docs. Harnesses are org-scoped with optional `project_id`, immutable versions, manifest hashes, approval-gated downloads, recommendations that return metadata only, and redacted Claude config reviews.

Current creation is still generic: the admin UI creates only `name`, `slug`, `description`, and `visibility`; publishing then accepts a raw manifest JSON textarea seeded with a single generic manifest. The backend manifest validator only requires `targets` and non-empty `provenance`, so typed formats can be introduced in the manifest without breaking the existing storage model.

Claude Code config inspection, limited to safe structure, shows relevant harness families: agents as Markdown files, skills as folders containing `SKILL.md` and optional assets, commands as Markdown files, hooks as shell scripts or hook JSON pointing to commands, output styles as Markdown files, plugins as marketplace/plugin JSON plus scripts/hooks/MCP files, and themes as JSON files. Config values can include secrets and local paths, so artifacts should store redacted structures or user-supplied install examples only.

## Affected Areas

- `apps/backend/src/db/migrations.rs` — `harnesses` currently has `created_by` but no first-class owner metadata; ownership requires a migration.
- `apps/backend/src/models/types.rs` — harness DTOs and create/list responses need owner fields and typed manifest format contracts.
- `apps/backend/src/db/queries.rs` — `create_harness`, `map_harness`, list/filter queries, and manifest validation need owner handling and format validation.
- `apps/backend/src/api/harnesses.rs` — create/list endpoints should accept/return ownership and reject invalid format manifests through existing validation mapping.
- `apps/admin/src/types.ts` — harness request/response types need owner fields plus typed format/payload helpers.
- `apps/admin/src/api/client.ts` — likely only type shape changes unless adding owner/format filters.
- `apps/admin/src/pages/Harnesses.tsx` — create/publish flow should become a guided format builder with file/folder upload affordances instead of only a raw JSON textarea.
- `apps/admin/src/pages/Harnesses.test.tsx` and `apps/admin/src/api/client.harnesses.test.ts` — cover format-specific creation, upload intent, and owner display/API payloads.
- `openspec/specs/harness-library/spec.md` — add requirements for first-class ownership and typed harness manifests.
- `openspec/changes/archive/2026-07-07-harness-sharing/*` — prior design confirms dedicated harness tables and immutable manifest decisions.

## Approaches

1. **Typed manifest builder with user-owned harnesses** — keep format-specific data in `manifest_json`, add first-class `owner_user_id` to `harnesses`, and generate manifest templates per format in the admin UI.
   - Pros: smallest safe slice; preserves immutable version model; gives explicit owner display/filtering; avoids polymorphic ownership complexity; supports multi-file/folder payload intent through manifest components.
   - Cons: ownership is initially user/accountable-maintainer only; org/team/project ownership remains future work; format filtering from SQL is limited unless extra summary columns are added later.
   - Effort: Medium

2. **Polymorphic ownership plus indexed format columns** — add `owner_type`, `owner_id`, and version-level `formats_json`/`payload_kinds_json` columns for SQL filtering.
   - Pros: models user/team/org/project ownership immediately; efficient format filters; more expressive catalog metadata.
   - Cons: requires app-level referential integrity for polymorphic owners or multiple nullable FK columns; larger migration/API/UI scope; team ownership is premature because teams are not a visible harness domain today.
   - Effort: High

3. **Manifest-only metadata** — put `owner`, `format`, and upload hints entirely inside `manifest_json` with no table migration.
   - Pros: fastest implementation; no schema change for format variants.
   - Cons: fails the requirement that ownership is first-class product metadata; owner cannot be reliably listed, filtered, reassigned, or permission-checked without parsing version manifests; ownership would vary by version instead of harness identity.
   - Effort: Low, but not recommended

## Recommendation

Use approach 1.

Add `owner_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE RESTRICT` to `harnesses`, backfilled from `created_by`. Treat `created_by` as audit provenance and `owner_user_id` as the accountable product owner. For the first slice, `org_id` remains tenancy, `project_id` remains visibility/scope, and ownership means a user/maintainer. Defer team/org/project ownership until there is a concrete reassignment and permission model.

For typed creation formats, keep the current `manifest_json` storage but strengthen the manifest contract:

```json
{
  "schema_version": "1.1",
  "targets": ["claude"],
  "format": "skill",
  "components": [
    {
      "kind": "folder",
      "path": "skills/example-skill",
      "entries": [
        { "kind": "file", "path": "SKILL.md", "media_type": "text/markdown", "content_ref": "upload://..." }
      ]
    }
  ],
  "compatibility": {},
  "provenance": { "source": "admin-ui" },
  "security": { "requires_approval": true }
}
```

Supported `format` values for the first UI builder: `agent`, `skill`, `command`, `hook`, `output_style`, `claude_code_plugin`, and `theme`. For multi-file/folder harnesses, represent uploaded content as component metadata and content references in the manifest; do not store raw local secret-bearing config snapshots. The UI should provide direct file and folder upload controls (`multiple` files plus folder selection via browser-supported directory upload) and show a generated manifest preview per selected format.

Format-specific generated shapes:

- `agent`: Markdown file component targeting Claude agents.
- `skill`: Markdown or folder component with `SKILL.md` and optional child files.
- `command`: Markdown file component targeting commands.
- `hook`: shell script component plus optional hook event metadata; require safe review messaging because shell is executable.
- `output_style`: Markdown file component targeting output styles.
- `claude_code_plugin`: plugin/marketplace JSON component using source/install/update metadata shape; only include user-provided or redacted install paths.
- `theme`: JSON file component with theme name/base/overrides.

## Backend Schema Decision

Backend schema must change for ownership. It should not be hidden in manifest JSON because ownership belongs to the harness identity, not to one immutable version. Format variants can fit in manifest JSON for the first slice because version immutability and hash validation already protect them. Add indexed format columns only if product requires server-side format filtering beyond target filtering.

## Permission and Product Behavior

- `harness:write` can create harnesses; default `owner_user_id` is the authenticated user unless an admin explicitly assigns another user.
- Owner is displayed in the harness list/detail and included in API responses.
- First slice should not grant special owner-only permissions unless the existing permission system already has a clear pattern; keep RBAC as the enforcement boundary.
- Later slices can add owner reassignment, owner-only edit/delete rules, team ownership, or project/org owner types.

## Tests Needed

- Backend migration test: v49 adds `owner_user_id`, backfills from `created_by`, and preserves existing harness rows.
- Backend API/query tests: create harness defaults owner to caller; optional admin owner assignment validates user belongs to org; list/get returns owner metadata.
- Backend manifest tests: accepts each supported `format`; rejects unknown format, missing `format`, missing `targets`, missing `provenance`, and unsafe hook/plugin payload indicators.
- Backend download tests: typed manifests remain approval-gated and hash-stable.
- Admin tests: create flow requires owner display/default; format selector generates different manifests; file and folder upload intent populates component entries; raw JSON preview still validates.
- Admin client tests: owner fields and typed manifest payloads round-trip without local mutation fields.
- OpenSpec tests/review: specs include ownership, typed manifest formats, upload/folder payload intent, and safe plugin/theme examples.

## Risks

- Shell hooks and plugins are executable/high-trust formats; approval copy must clearly state the backend only stores/downloads manifests and local tools must show diffs before applying.
- Browser folder upload support is not perfectly standardized; the UI should degrade to multi-file upload and preserve relative paths where available.
- If plugin manifests include absolute local paths, examples must be user-provided or redacted and should not leak private paths in docs/tests.
- Manifest hash stability currently uses `serde_json::to_vec`; if UI formatting changes but semantic JSON is identical, hash behavior depends on serialized object ordering. Avoid claiming canonical cross-client hashes unless canonicalization is deliberately implemented.
- User ownership now may need future migration if the product later needs true team/org/project ownership semantics.

## Ready for Proposal

Yes. The orchestrator should propose a narrow amendment to PR #210: add first-class user ownership for harnesses, introduce typed manifest format variants in the admin builder, support file/folder upload intent in the UI, validate supported formats on publish, update OpenSpec specs/tests, then push the existing PR and remind the user to merge PR #210 before starting deeper MCP/plugin harness research.
