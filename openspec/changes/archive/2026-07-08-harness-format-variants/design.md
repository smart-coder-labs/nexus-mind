# Design: Harness Format Variants

## Technical Approach

Extend the existing Harness Library slice instead of introducing a new storage model. Ownership becomes catalog-level data on `harnesses`; format variants remain version-level manifest JSON validated before publish. The admin page moves from raw JSON-first publishing to a guided manifest builder with safe browser file/folder intake, while the backend keeps the current approval-gated download and no-local-mutation boundary.

## Architecture Decisions

| Option | Tradeoff | Decision |
|---|---|---|
| `owner_user_id` on `harnesses` | Requires migration and joins, but ownership is stable across immutable versions. | Add first-class owner, backfilled from `created_by`; keep `created_by` as audit provenance. |
| Typed manifest JSON | Less SQL-filterable than columns, but fits current immutable version table. | Add `format` + `components` validation in `manifest_json`; defer indexed format columns. |
| Browser-packaged uploads | No direct folder path access and `webkitdirectory` is browser-specific, but avoids backend filesystem access. | Use file inputs, preserve relative paths where available, and degrade to multi-file selection. |
| Executable formats allowed as manifests | Hooks/plugins are high trust. | Store only reviewed manifest/package content; require approval copy and reject secret/raw-local indicators. |

## Data Flow

```text
Admin selects format/files ──> UI builds manifest preview ──publish──> Axum validation
      │                                                                │
      └─ owner filter/display <── joined users + harness rows <── SQLite
Agent/user recommends ──metadata only──> approval ──download exact manifest hash──> local installer diff
```

## File Changes

| File | Action | Description |
|---|---|---|
| `apps/backend/src/db/migrations.rs` | Modify | Add v49 rebuild/add migration for `harnesses.owner_user_id`, backfill from `created_by`, index `(org_id, owner_user_id, status)`. |
| `apps/backend/src/models/types.rs` | Modify | Add `HarnessOwner`, `owner_user_id`, optional `owner`, `owner_user_id` create input, and typed manifest DTO aliases/enums. |
| `apps/backend/src/db/queries.rs` | Modify | Join users for owner metadata, default owner to caller, validate admin-assigned owner belongs to org, filter by owner, validate typed manifests. |
| `apps/backend/src/api/harnesses.rs` | Modify | Accept `owner_user_id` and `?owner_user_id=` list filter; map validation errors to 422. |
| `apps/admin/src/types.ts` | Modify | Mirror owner and manifest format/component contracts. |
| `apps/admin/src/api/client.ts` | Modify | Add owner filter typing; no new endpoint required. |
| `apps/admin/src/pages/Harnesses.tsx` | Modify | Add owner display/filter, format selector, template generator, file/folder controls, manifest preview. |
| `apps/admin/src/pages/Harnesses.test.tsx` | Modify | Cover owner display/filter and format/file builder behavior. |
| `apps/admin/src/api/client.harnesses.test.ts` | Modify | Cover owner fields and typed manifest payloads. |
| `openspec/specs/harness-library/spec.md` | Modify | Add ownership, typed manifest, upload/folder, validation/security requirements. |

## Interfaces / Contracts

```ts
type HarnessFormat = 'agent' | 'skill' | 'command' | 'hook' | 'output_style' | 'claude_code_plugin' | 'theme'
type ComponentKind = 'file' | 'folder' | 'plugin_marketplace' | 'theme_json'
type HarnessManifest = {
  schema_version: '1.1'
  targets: ('claude' | 'codex' | 'opencode')[]
  format: HarnessFormat
  components: Array<{
    kind: ComponentKind
    path: string
    media_type?: string
    size_bytes?: number
    sha256?: string
    content?: string
    entries?: Array<{ kind: 'file'; path: string; media_type: string; size_bytes: number; sha256: string; content?: string }>
  }>
  provenance: { source: string }
  security: { requires_approval: true; executable?: boolean; secret_scan_status?: 'passed' }
}
```

Backend validation: require `schema_version`, non-empty `targets`, supported `format`, non-empty `provenance`, components matching the format template, relative normalized paths only, no `..`/absolute paths, size caps for inline text content, no secret indicators, and `requires_approval=true` for hooks/plugins. File/folder upload is client-side packaging only; browsers never expose absolute local paths. Folder support uses `webkitdirectory` when available and falls back to `multiple` files.

## Testing Strategy

| Layer | What to Test | Approach |
|---|---|---|
| Backend unit | v49 ownership migration, owner joins/filter, owner org validation, format validator. | Rust tests in migrations/queries. |
| Backend integration | Create defaults owner to caller; admin owner assignment; publish accepts seven formats and rejects unsafe manifests. | Axum harness route tests. |
| Admin | Owner display/filter, generated templates, file/folder metadata packaging, executable warning copy. | Vitest + Testing Library with mocked `File` inputs. |
| Contract | Client payloads preserve owner/manifest fields and omit local mutation fields. | Existing `client.harnesses.test.ts`. |

## Migration / Rollout

Additive PR #210 update. Run v49 after v48; backfill every existing harness owner from `created_by`. No data deletion. If rollback is needed, UI can hide owner/format controls while backend keeps inert owner data.

## Open Questions

- [ ] Exact inline content size cap for text files; default proposal is 64 KiB per file for PR #210.
