# Migration Note: `opencode` -> `cursor` harness target

## What changed

`apps/backend/src/models/types.rs::validate_typed_harness_manifest` no longer
accepts `opencode` as a valid `targets` entry. Valid targets are now
`claude | codex | cursor`.

## Required operational action before/with rollout

This change does **not** run any data migration in code. Before or alongside
deploying this change, an operator MUST:

1. Query the `harness_versions` table (or equivalent store) for rows where
   `manifest_json.targets` contains `"opencode"`.

   Ready-to-run detection SQL (find affected prod rows):

   ```sql
   SELECT id, harness_id, version
   FROM harness_versions
   WHERE manifest_json LIKE '%"opencode"%';
   ```

   Each match's `targets` array MUST be updated (`opencode` -> `cursor`) or the
   version archived before rollout — see step 2.
2. For each affected row, either:
   - `UPDATE` the `targets` array to replace `opencode` with `cursor`, or
   - Archive/deprecate the row if `cursor` is not a valid replacement for
     that specific harness.
3. Confirm the affected row count against production data before rollout,
   and re-confirm after the operational update completes.

## Why this matters

- Validation (`validate_typed_harness_manifest`) only runs on **publish**,
  not on read. Existing persisted rows with `opencode` are not immediately
  broken, but:
  - Any `target=cursor` filtered read (`list_harnesses`, recommendations)
    will NOT match those rows.
  - Any attempt to **republish** or revalidate those rows will fail with
    `missing_targets`.
- If this backend change is rolled back, `opencode` becomes valid again and
  any `cursor`-only rows created in the interim would need a reverse
  `UPDATE` back to `opencode` (or manual reconciliation).

## Scope note

`apps/landing/*.astro` marketing copy referencing "OpenCode" (4 refs) is
explicitly out of scope for this change and is tracked separately.
