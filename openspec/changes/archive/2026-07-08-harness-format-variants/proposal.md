# Proposal: Harness Format Variants

## Intent

Amend PR #210 so Harness Library creation is format-aware, upload-oriented, and owner-accountable. Users create Claude-family harnesses from guided variants instead of generic manifests, without secret-bearing config capture or silent local mutation.

## Scope

### In Scope
- Typed variants: agent Markdown, skill Markdown/file/folder, command Markdown, hook `.sh`, output-style Markdown, Claude Code plugin JSON, theme JSON.
- Direct file/folder upload intent as manifest component metadata/content references.
- First-class user ownership via `owner_user_id`, backfilled from `created_by`.
- Format validation and config-derived examples with no raw secrets, tokens, or private paths.

### Out of Scope
- Team/org/project owner polymorphism or owner-only permissions.
- Backend installation/mutation of local tool files.
- Public marketplace distribution or deep installer research.

## Capabilities

### New Capabilities
- None.

### Modified Capabilities
- `harness-library`: Add owner metadata and typed manifest formats/components for publishable harness versions.
- `harness-config-review`: Clarify that config-derived examples must be user-reviewed/redacted and never store raw secrets or sensitive local paths.
- `harness-install-approval`: Preserve approval-first download/install boundaries for executable hooks/plugins and uploaded file/folder payloads.

## Approach

Keep the current harness/version model. Add `owner_user_id` to `harnesses`, defaulting to the caller and backfilled from `created_by`; keep `created_by` as audit provenance. Store variants in `manifest_json` with `schema_version`, `format`, `targets`, `components`, `provenance`, and `security`. Admin provides guided builders, upload controls, and manifest preview; backend validates formats and rejects unsafe config-derived examples.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | Modified | Add/backfill `owner_user_id`. |
| `apps/backend/src/models/types.rs` | Modified | Owner + manifest DTOs. |
| `apps/backend/src/db/queries.rs` | Modified | Owner mapping, validation. |
| `apps/backend/src/api/harnesses.rs` | Modified | Create/list/publish owner + formats. |
| `apps/admin/src/types.ts` | Modified | Owner + variant/upload types. |
| `apps/admin/src/pages/Harnesses.tsx` | Modified | Builder, upload, preview. |
| `openspec/specs/harness-*` | Modified | Ownership, formats, safety deltas. |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| Executable hooks/plugins are high trust | Med | Approval copy, validation, no backend install. |
| Folder upload varies by browser | Med | Multi-file fallback; preserve relative paths when present. |
| Teams may be needed later | Low | Start with user owner; migrate only with product need. |

## Rollback Plan

Hide the typed builder and stop owner overrides. Keep the additive column inert/backfilled; existing manifests remain under current approval/hash rules.

## Dependencies

- Harness-sharing tables, permissions, admin page, and approval-gated downloads from PR #210.

## Success Criteria

- [ ] Each variant produces a valid preview and publish payload.
- [ ] File/folder upload intent avoids raw local secret storage.
- [ ] Harness list/detail returns and displays owner metadata.
- [ ] Config-derived examples remain redacted and approval-gated.
