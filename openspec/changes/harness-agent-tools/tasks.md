# Tasks: Harness Agent Tools

> **Prod-data migration note:** Phase 0 removes `opencode` as a valid harness
> manifest target in favor of `cursor`. No data migration runs in code. See
> [`MIGRATION_NOTE.md`](./MIGRATION_NOTE.md) for the required operational
> `UPDATE`/archival steps and row-count confirmation before rollout.

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | ~950-1250 total (backend+admin ~120-160, MCP read ~180-220, MCP install core ~350-450, MCP create/upload ~200-280, optional Phase 4 ~100-140) |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | PR 1 (Phase 0+1) → PR 2 (Phase 2) → PR 3 (Phase 3) → PR 4 (Phase 4, optional) |
| Delivery strategy | ask-on-risk |
| Chain strategy | pending |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: pending
400-line budget risk: High

### Suggested Work Units

| Unit | Goal | Likely PR | Notes |
|------|------|-----------|-------|
| 1 | Phase 0 backend swap + Phase 1 MCP read tools | PR 1 | ~300-380 lines; base = main (or tracker branch); independent, cheapest slice, ships alone |
| 2 | Phase 2 MCP install core (materializer, resolver, plan/apply) | PR 2 | ~350-450 lines; highest risk; base = PR 1 branch or main depending on chain strategy |
| 3 | Phase 3 MCP create/upload (secret-scan, build, create/publish) | PR 3 | ~200-280 lines; base = PR 2 branch or main |
| 4 | Phase 4 create_harness_config_review (optional) | PR 4 | ~100-140 lines; independent of PR 3 internals, can ship last or be dropped |

## Phase 0: Backend Target Swap (opencode -> cursor)

- [x] 0.1 RED: add/update Rust test in `apps/backend/src/models/types.rs` asserting `cursor` passes `validate_typed_harness_manifest` valid-targets check and `opencode` fails with `missing_targets`-style error (Spec: harness-library "Accept cursor as a valid target" / "Reject opencode as a target").
- [x] 0.2 GREEN: in `apps/backend/src/models/types.rs` (~line 1661), change `matches!(v.as_str(), Some("claude" | "codex" | "opencode"))` -> `Some("claude" | "codex" | "cursor")`.
- [x] 0.3 Update any existing manifest fixtures/tests in `types.rs` that assert `opencode` validity to use `cursor` instead. (No prior fixture asserted `opencode` validity; new test covers both cursor-accept and opencode-reject.)
- [x] 0.4 `apps/admin/src/pages/Harnesses.tsx:940` — replace `<option value="opencode">OpenCode</option>` with `<option value="cursor">Cursor</option>`.
- [x] 0.5 `apps/admin/src/pages/Harnesses.tsx:546` — update approval copy "...Claude, Codex, OpenCode, shell..." -> "...Claude, Codex, Cursor, shell...".
- [x] 0.6 `apps/admin/src/types.ts:676` — change `targets: Array<'claude' | 'codex' | 'opencode'>` -> `'cursor'`.
- [x] 0.7 Admin test: Vitest + Testing Library assertion that the `cursor` option renders in `Harnesses.tsx` and approval copy reads "Cursor" (Spec: harness-library scenarios).
- [x] 0.8 Document prod-data migration note: flag any `harness_versions` rows with `manifest_json.targets` containing `opencode` as needing an operational `UPDATE` to `cursor` (no code in this change) — added as a code comment near the validation change referencing this task, plus standalone `MIGRATION_NOTE.md`.

## Phase 1: MCP Read Tools (client + 4 tools)

- [x] 1.1 RED: Vitest unit tests in `../nexusmind-mcp/src/client.test.ts` (or existing test file) for `listHarnesses`, `recommendHarnesses`, `getHarnessVersion`, `listHarnessConfigReviews` — assert correct verb/path/query params and typed response shape.
- [x] 1.2 GREEN: add `listHarnesses`, `recommendHarnesses`, `getHarnessVersion`, `listHarnessConfigReviews` methods to `../nexusmind-mcp/src/client.ts` per design's client-methods table, using existing `request<T>()` helper.
- [x] 1.3 Add new TS types (`Harness`, `HarnessVersion`, `HarnessRecommendation`, `HarnessConfigReview`) to `../nexusmind-mcp/src/client.ts` (or a types module) mirroring backend DTOs.
- [x] 1.4 RED: test asserting `recommend_harnesses` and `list_harnesses` never call a download/manifest-content endpoint (Spec: "Permissioned Read Tools Are Metadata-Only").
- [x] 1.5 GREEN: register `recommend_harnesses` tool in `../nexusmind-mcp/src/index.ts` with zod shape `{ target: z.enum(['claude','codex','cursor']).optional() }`, returning metadata-only text list.
- [x] 1.6 GREEN: register `list_harnesses` tool with zod shape `{ target: z.enum(['claude','codex','cursor']).optional(), owner_user_id: z.string().optional() }`.
- [x] 1.7 GREEN: register `get_harness_version` tool with zod shape `{ harness_id: z.string(), version: z.string() }`, returning manifest preview metadata only (no writes).
- [x] 1.8 GREEN: register `list_harness_config_reviews` tool with zod shape `{ status: z.string().optional() }`.
- [x] 1.9 Test: permission-denied path — `harness:read` missing causes `list_harness_config_reviews` to deny and return no metadata (Spec: "List config reviews requires permission").
- [x] 1.10 Manual/integration check: confirm all 4 read tools follow existing `{ content: [{ type: 'text', text }], isError? }` handler shape used by `store_memory`/`search_memories`.

## Phase 2: MCP Install Core (highest risk — materializer, resolver, plan/apply)

- [x] 2.1 RED: unit tests for `harness/matrix.ts` resolver — correct destination per (format, tool, scope) combination from the applicability matrix; unsupported pairs (e.g. `skill`+`codex`, `hook`+`cursor`) refuse with a clear reason. (Implemented as `harness/resolver.ts` / `resolver.test.ts` — see task 2.2 note.)
- [x] 2.2 GREEN: implement `../nexusmind-mcp/src/harness/matrix.ts` — pure-function format->tool applicability matrix + per-tool destination resolver (no I/O), covering all 7 formats x 3 tools x 2 scopes per design table. (Module named `harness/resolver.ts` instead of `matrix.ts` — same responsibility, no other deviation; 26 unit tests cover all matrix cells including every unsupported pair.)
- [x] 2.3 RED: unit tests for `harness/materialize.ts` path-traversal defense — reject absolute paths, `..` segments, and any resolved path escaping tool root; assert refusal on poisoned manifest with no partial writes.
- [x] 2.4 RED: unit tests for `harness/materialize.ts` sha256 verification — mismatch between recomputed content hash and manifest component sha256 aborts before write.
- [x] 2.5 RED: unit tests for `harness/materialize.ts` atomicity — temp-file + rename pattern, `mkdir -p` parent dirs, `chmod 0o755` when `executable: true` else `0o644`.
- [x] 2.6 RED: unit test for `harness/materialize.ts` settings.json merge (hook/plugin registration) — idempotent read-modify-write merge via same temp+rename atomicity.
- [x] 2.7 GREEN: implement `../nexusmind-mcp/src/harness/materialize.ts` exporting `applyPlan(diff: DiffEntry[]): { written[], skipped[], errors[] }` satisfying tasks 2.3-2.6.
- [x] 2.8 RED: unit test for `harness/plan.ts` diff actions — no existing file -> `create`; identical sha256 -> `skip`; differing sha256 -> `overwrite`; assert zero write/fs-mutation calls anywhere in the plan path (mocked fs reads only).
- [x] 2.9 GREEN: implement `../nexusmind-mcp/src/harness/plan.ts` exporting `planInstall(manifest, tool, scope)` -> `DiffEntry[]`, importing only read/hash utilities (never `fs.writeFile`), per design's `DiffEntry` contract.
- [x] 2.10 Add `downloadHarnessVersion`, `approveHarnessInstall`, `recordHarnessInstallResult` methods to `../nexusmind-mcp/src/client.ts`.
- [x] 2.11 RED: integration test for `plan_harness_install` tool — full diff returned, executable/plugin warnings flagged with `requires_acknowledgement: true`, unsupported format/tool pair refused (no partial diff) (Spec: "Plan Install Produces a Diff and Writes Nothing", "Executable component flagged in plan", "Unsupported format-to-tool pair refused").
- [x] 2.12 GREEN: register `plan_harness_install` tool in `../nexusmind-mcp/src/index.ts` with zod shape per design; calls `getHarnessVersion` (preview, no approval), validates via matrix, resolves destinations, computes diff via `plan.ts`, returns `{ manifest_hash, format, requires_acknowledgement, warnings[], diff }`.
- [x] 2.13 RED: integration test for `apply_harness_install` — hash-mismatch between plan and fresh download returns `hash_mismatch` result with no write and no `record_install_result` call (Spec: "Manifest hash mismatch blocks apply").
- [x] 2.14 RED: integration test for `apply_harness_install` — executable format without `warning_acknowledged` refuses to write any file and does not call `record_install_result` (Spec: "Executable format requires explicit acknowledgement").
- [x] 2.15 RED: integration test for `apply_harness_install` — call without prior plan/manifest_hash is rejected, no write, no record (Spec: "Apply refuses without prior plan confirmation"). (manifest_hash is a required zod field; a call missing it returns `isError: true` before any client/backend call is made — see `harness-tools.test.ts`.)
- [x] 2.16 RED: integration test for `apply_harness_install` — happy path: approve_install called with manifest hash, files materialized exactly as diffed, `record_install_result` called after write, never transmitting raw file contents (Spec: "Apply after confirmation writes and records").
- [x] 2.17 GREEN: register `apply_harness_install` tool in `../nexusmind-mcp/src/index.ts` implementing steps from design section 1 (`apply_harness_install`) — approve -> re-download -> hash check -> materialize -> record; return `{ approval_id, manifest_hash, result_status, written[], skipped[], errors? }`.
- [x] 2.18 Verify integration: run full plan -> apply round trip against a mocked backend contract confirming no write reachability from the plan module (static import check or test double asserting `materialize.ts` is never imported by `plan.ts`). (`plan.test.ts` includes a static-source-check test asserting `plan.ts` never imports `materialize.js` and contains no `writeFile`/`rename`/`mkdir` calls, plus a runtime test asserting `planInstall` leaves the destination dir absent on disk.)

## Phase 3: MCP Create/Upload

- [x] 3.1 RED: unit tests for `harness/secretscan.ts` — detects API-key/token patterns, private-key blocks, `.env`-style assignments, and local-path leakage; refuses with category+filename list, never the matched value.
- [x] 3.2 GREEN: implement `../nexusmind-mcp/src/harness/secretscan.ts` per design section 6 scanner checks. (Implemented as `harness/secret-scan.ts` — same responsibility, no other deviation.)
- [x] 3.3 RED: unit tests for `harness/build.ts` — valid manifest assembly (schema 1.1, sha256, size_bytes, components matching format template); refuses files >64KiB with no silent truncation; refuses entirely on any secret-scan hit with no partial manifest returned.
- [x] 3.4 GREEN: implement `../nexusmind-mcp/src/harness/build.ts` exporting `buildManifestFromPath(path, format, targets)` per design section 6 steps 1-6. (Implemented as `harness/build-manifest.ts` — same responsibility, no other deviation.)
- [x] 3.5 GREEN: register `build_harness_manifest_from_path` tool in `../nexusmind-mcp/src/index.ts`, gated by `harness:write`, wiring secretscan + build modules (Spec: "Build Manifest From Local Path" scenarios).
- [x] 3.6 Add `createHarness`, `publishHarnessVersion` methods to `../nexusmind-mcp/src/client.ts`.
- [x] 3.7 RED: integration test — `create_harness` and `publish_harness_version` deny calls when `harness:write` is absent, with no data created/published (Spec: "Deny create or publish without write permission").
- [x] 3.8 GREEN: register `create_harness` tool (zod shape per design) as thin wrapper over `createHarness` client method.
- [x] 3.9 GREEN: register `publish_harness_version` tool (zod shape per design) as thin wrapper over `publishHarnessVersion`, accepting a manifest from `build_harness_manifest_from_path`.
- [x] 3.10 Cross-check test: manifest produced by `build_harness_manifest_from_path` passes backend `validate_typed_harness_manifest` fixture/contract without 422 (aligns Rust validator expectations with TS builder output).

## Phase 4: Optional — Config Review Upload

- [x] 4.1 RED: unit test for local redaction/preview logic backing `create_harness_config_review` — produces `redacted_config`, `redaction_report`, `content_hash` without raw secret values.
- [x] 4.2 GREEN: implement local redaction/preview helper (reuse `harness/secretscan.ts` categories) in `../nexusmind-mcp/src/harness/` for config-review use. (Implemented as `harness/config-review.ts`, reusing `harness/secret-scan.ts` categories — same responsibility, no other deviation.)
- [x] 4.3 Add `createHarnessConfigReview` method to `../nexusmind-mcp/src/client.ts`.
- [x] 4.4 RED: integration test — upload without a local redaction report is rejected; upload containing unredacted secret indicators is rejected exactly as a non-agent caller would be (Spec: harness-config-review "Agent-session config review requires local preview before upload", "Agent-session upload still enforces raw-content rejection").
- [x] 4.5 GREEN: register `create_harness_config_review` tool in `../nexusmind-mcp/src/index.ts` with zod shape `{ source_tool, config_path }`, performing local preview then upload only on confirm.

## Phase 5: Documentation & Wrap-Up

- [x] 5.1 Update `../nexusmind-mcp` README/tool list (if present) to document the 10 new harness tools and their permission gates.
- [x] 5.2 Add a code comment in `apps/backend/src/models/types.rs` near the target-swap referencing the prod-data migration note (task 0.8) for future maintainers.
- [x] 5.3 Confirm `apps/landing/*.astro` `opencode` references remain untouched (out of scope) — no action, verification only. (Verified: zero `opencode` references found in `apps/landing/*.astro` — the design's "4 refs" claim was stale; nothing to leave untouched.)
