# Tasks — SDD Specs (the living specification)

**Change:** `sdd-specs`
**Project:** nexus-mind

TDD is strict (`openspec/config.yaml`): RED before GREEN, everywhere.

---

## 1. Migration v55

- [x] 1.1 RED: `run_v55_creates_spec_tables` — `sdd_specs` + `sdd_spec_revisions` with every column.
- [x] 1.2 RED: `run_v55_one_spec_per_capability_per_project` — `UNIQUE(org_id, project, capability)`
      holds; the same capability in another project is its own contract.
- [x] 1.3 RED: `run_v55_merged_from_change_id_survives_the_change_it_names` — `ON DELETE SET NULL`.
      Deleting the change nulls the column and **leaves the revision standing**.
- [x] 1.4 RED: `run_v55_spec_revisions_cascade_from_the_spec` — a spec's revisions are its own history.
- [x] 1.5 RED: `run_v55_revision_numbers_are_unique_per_spec` — the history cannot fork.
- [x] 1.6 RED: `run_v55_source_defaults_to_agent`.
- [x] 1.7 RED: `run_v55_creates_specs_fts_virtual_table` — content indexed, `spec_id` UNINDEXED.
- [x] 1.8 RED: `run_v55_creates_indexes`, `run_v55_is_idempotent`.
- [x] 1.9 GREEN: `run_v55`, registered in `run_all`.
- [x] 1.10 Bump every migration-version assertion 54 → 55 (25 in `migrations.rs`, 1 in
      `tests/integration_test.rs`).

## 2. Store (`db/queries.rs`)

- [x] 2.1 RED: `upsert_sdd_spec_creates_spec_and_revision_1`.
- [x] 2.2 RED: `upsert_sdd_spec_does_not_create_a_synthetic_change` — **the modelling decision,
      asserted**. A spec belongs to the project; saving one conjures no change to hang off.
- [x] 2.3 RED: `upsert_sdd_spec_creates_no_revision_when_hash_unchanged` — no revision, no FTS write,
      no `updated_at` bump.
- [x] 2.4 RED: `upsert_sdd_spec_appends_revision_2_on_changed_content`.
- [x] 2.5 RED: `upsert_sdd_spec_revert_to_earlier_content_appends_revision_3` — A → B → A. A revert is
      an event.
- [x] 2.6 RED: `upsert_sdd_spec_rejects_content_over_1mb_atomically` — no spec row, no revision row,
      and a pre-existing contract is untouched.
- [x] 2.7 RED: `upsert_sdd_spec_accepts_content_just_under_the_cap`.
- [x] 2.8 RED: `upsert_sdd_spec_replaces_fts_row_on_new_revision` — one row per spec; a struck term
      stops matching.
- [x] 2.9 RED: `upsert_sdd_spec_org_isolation`.
- [x] 2.10 RED: `upsert_sdd_spec_records_which_change_merged_into_the_contract` — both directions.
- [x] 2.11 RED: `upsert_sdd_spec_without_a_change_name_has_null_provenance` — a legitimate state.
- [x] 2.12 RED: `upsert_sdd_spec_rejects_an_unknown_change_name_atomically`.
- [x] 2.13 RED: `upsert_sdd_spec_will_not_merge_from_another_orgs_change`.
- [x] 2.14 RED: `list_sdd_specs_for_change_reports_every_spec_the_change_touched` (+ the empty case).
- [x] 2.15 RED: the reads — `get_sdd_spec`, `get_sdd_spec_by_capability` (None for unknown),
      `list_sdd_specs` (metadata + provenance, org-isolated), `list_sdd_spec_revisions`
      (metadata-only, newest first), `get_sdd_spec_revision` (full content, org-scoped).
- [x] 2.16 RED: `search_sdd_specs_returns_snippets_scoped_to_org`, `..._sanitizes_fts_query_syntax`.
- [x] 2.17 RED: `search_sdd_all_covers_specs_and_artifacts_and_labels_each_hit` — the contract outranks
      the drafts — and `search_sdd_all_honours_the_limit_across_both_trees`.
- [x] 2.18 RED: `search_sdd_specs_by_query_matches_capability_and_title_for_global_search`.
- [x] 2.19 RED: `no_store_function_mutates_a_spec_revision` — the source-scan. **Needles built at
      runtime with `format!`; the prohibited statements are not spelled out here or in the comment
      that explains the prohibition, because the scan reads its own source.**
- [x] 2.20 GREEN: the `// ── SDD specs ──` section.

## 3. API (`api/sdd.rs`, `api/router.rs`, `api/search.rs`)

- [x] 3.1 RED: `put_sdd_spec_returns_200_and_created_revision_true_on_first_save` — **200, never 201.**
- [x] 3.2 RED: `put_sdd_spec_second_identical_save_returns_created_revision_false`.
- [x] 3.3 RED: `put_sdd_spec_over_1mb_returns_422_and_creates_nothing` — a 422 from our guard, not a
      413 from Axum's body limit.
- [x] 3.4 RED: `put_sdd_spec_honours_the_source_field_and_rejects_unknown_values`.
- [x] 3.5 RED: `put_sdd_spec_records_merged_from_change_and_the_change_reports_it_back`.
- [x] 3.6 RED: `put_sdd_spec_with_an_unknown_change_name_returns_404_and_creates_nothing`.
- [x] 3.7 RED: `get_sdd_spec_by_natural_key_returns_full_content`; `..._404s_for_an_unknown_capability`
      — never a 200 with empty content.
- [x] 3.8 RED: `list_sdd_specs_returns_metadata_only_never_content`.
- [x] 3.9 RED: `get_sdd_spec_from_another_org_returns_404_not_403` — a 403 would confirm the id exists.
- [x] 3.10 RED: `spec_revisions_list_is_metadata_only_and_the_read_is_full_content`.
- [x] 3.11 RED: `no_endpoint_mutates_or_deletes_a_spec_revision` — PUT/PATCH/DELETE on a revision → 405.
- [x] 3.12 RED: `the_specs_collection_route_is_not_swallowed_by_the_id_route`.
- [x] 3.13 RED: the permission matrix — `put_sdd_spec_denied_without_sdd_write`,
      `get_sdd_specs_denied_without_sdd_read`, `spec_endpoints_enforce_the_read_write_split`,
      `spec_routes_require_authentication`.
- [x] 3.14 RED: `search_covers_specs_as_well_as_change_artifacts` — every hit carries `hit_type`.
- [x] 3.15 RED: `list_change_specs_returns_404_for_an_unknown_change`; the empty case is a 200.
- [x] 3.16 RED: `global_search_returns_the_sdd_specs_facet`; and
      `global_search_without_sdd_read_returns_an_empty_specs_facet_not_403` — **A4**.
- [x] 3.17 GREEN: the handlers, the routes (static collection before `:id`), the facet.

## 4. Importer (`bin/import_sdd.rs`)

- [x] 4.1 RED: `discover_specs_finds_one_spec_per_capability_directory` — only `spec.md`, only where it
      exists.
- [x] 4.2 RED: `discover_specs_tolerates_a_repo_with_no_specs_tree`.
- [x] 4.3 RED: `spec_title_takes_the_first_h1_and_nothing_else`.
- [x] 4.4 RED: `import_specs_creates_a_living_spec_per_capability` — `source='import'`, `git_path` set,
      no change created, no invented provenance.
- [x] 4.5 RED: `import_specs_is_idempotent_a_second_run_creates_no_revision`.
- [x] 4.6 RED: `import_specs_dry_run_predicts_the_real_run_and_writes_nothing`.
- [x] 4.7 RED: `a_delta_spec_and_the_living_spec_for_one_capability_are_two_documents` — the two trees
      do not collide.
- [x] 4.8 RED: `import_specs_walks_this_repos_own_openspec_specs_tree` — the three specs that exist on
      disk right now.
- [x] 4.9 RED: `plan_walks_the_specs_tree_by_default_and_skip_specs_takes_it_out`.
- [x] 4.10 RED (API sink, real router over a real socket):
      `api_import_specs_creates_zero_revisions_on_a_second_identical_run` — including that `source` and
      `git_path` survive the trip over HTTP — and
      `api_import_specs_surfaces_an_oversized_spec_as_the_servers_422`.
- [x] 4.11 GREEN: `discover_specs`, `spec_title`, `import_specs`, `Sink::save_spec`,
      `Sink::latest_spec_hash`, `--skip-specs`, `specs_created` in the stats.

## 5. Admin (`apps/admin`)

- [x] 5.1 Types: `SddSpec`, `SddSpecDetail`, `SddSpecMerge`, `SddSpecRevision`,
      `SddSpecRevisionMeta`, `SddSpecSummary`, `SddSearchResult`; `GlobalSearchResult.sdd_specs`.
- [x] 5.2 Client: `listSddSpecs`, `getSddSpec`, `listSddSpecRevisions`, `getSddSpecRevision`,
      `getSddChangeSpecs`, `searchSdd`. **No spec-save method — read-only over content (A7).**
- [x] 5.3 Extract `DocumentView` from `ChangeDetail` (Raw/Preview + revision selector + panel) and
      re-point `ChangeDetail` at it. **Extracted, not forked** (D12).
- [x] 5.4 RED: `sdd_specs_tab_lists_one_row_per_capability_with_its_revision_and_last_merge`.
- [x] 5.5 RED: `sdd_specs_list_never_asks_for_content`; skeleton; empty state; project filter.
- [x] 5.6 RED: `sdd_specs_denied_without_sdd_read_redirects_and_never_calls_the_api` — an ungated 403
      would redirect the whole app to `/401` (D13). And `..._readable_with_sdd_read_alone`.
- [x] 5.7 RED: the drawer — markdown by default, Raw toggle, revision selector fetching an older
      revision, `spec_drawer_revision_labels_name_the_change_that_merged_each_one`, the provenance
      section, `spec_drawer_is_read_only_over_content`, `?spec=<id>` deep link.
- [x] 5.8 RED: `change_drawer_lists_the_specs_this_change_merged_into` (+ the empty case).
- [x] 5.9 GREEN: `SpecDetail`, the Specs tab on `/sdd`, the "Specs Merged" section on `ChangeDetail`.

## 6. MCP (`nexusmind-mcp`, branch `feat/sdd-specs-mcp`)

- [x] 6.1 Client: `saveSddSpec`, `getSddSpec`, `getSddSpecByCapability`, `listSddSpecs`,
      `listSddSpecRevisions`, `getSddSpecRevision`, `listSddSpecsForChange`; `SddSearchHit` gains
      `hit_type` and the tree-specific fields become optional.
- [x] 6.2 RED: `src/sdd-spec-client.test.ts` — the wire shapes, the flattened detail, the 404's status
      surviving so the tool can branch on it.
- [x] 6.3 RED: `src/sdd-spec-tools.test.ts` — the three tools against a **stateful** fake backend:
      idempotency, the revision append, the merged-from provenance, the atomic refusal of an unknown
      change name, the atomic 1 MB rejection, the permission denials, the FULL document on read (a
      200-line contract, first and last line both present), not-found ≠ empty, metadata-only lists.
- [x] 6.4 GREEN: `save_sdd_spec`, `get_sdd_spec`, `list_sdd_specs` — zod raw shapes, `.describe()` on
      every field, the try/catch text-return contract, `formatSavedSddSpec` / `formatSddSpecList`.
- [x] 6.5 The search formatter labels each hit `[spec]` or `[artifact]`, so an agent cannot quote a
      proposal as though it were the contract.
- [x] 6.6 **Register both new test files in `package.json`'s `test` script** — they do not run
      otherwise. Update the tool-count test: 7 → 10.
- [x] 6.7 Bump to 0.10.0.

## 7. Gates

- [x] `cargo test --manifest-path apps/backend/Cargo.toml`
- [x] `cargo clippy --manifest-path apps/backend/Cargo.toml -- -D warnings`
- [x] `cd apps/admin && npm run test && npx tsc -b && npm run build`
- [x] `cd nexusmind-mcp && npm test`
- [x] **Not** `cargo fmt` — `main` is not fmt-clean and it would reformat ~50 unrelated files.
