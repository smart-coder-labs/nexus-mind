# Tasks: Code Knowledge Graph (Slice 1 — Structural)

## Review Workload Forecast

| Field | Value |
|-------|-------|
| Estimated changed lines | 1,200–1,500 |
| 400-line budget risk | High |
| Chained PRs recommended | Yes |
| Suggested split | size:exception required (delivery_strategy = single-pr) |
| Delivery strategy | single-pr |
| Chain strategy | size-exception |

Decision needed before apply: Yes
Chained PRs recommended: Yes
Chain strategy: size-exception
400-line budget risk: High

### Work Units (size:exception record)

| Unit | Primary Files | ~Lines |
|------|--------------|--------|
| WU-A | `db/migrations.rs` | 110 |
| WU-B | `indexer/tree_sitter_chunker.rs` | 400 |
| WU-C/D | `db/queries.rs` + `indexer/mod.rs` | 420 |
| WU-E | `api/code.rs` + `models/types.rs` + `api/router.rs` | 145 |
| WU-F | `apps/admin` package, types, client, GraphTab, Code.tsx | 315 |

---

## Phase 1: Migrations — WU-A

- [ ] 1.1 **[RED]** `apps/backend/src/db/migrations.rs`: write failing tests — fresh `:memory:` DB reaches `user_version=42`; `code_symbols` and `code_edges` have expected columns; re-running `run()` is idempotent (no error, version stays 42). Run `cargo test` — must fail.
- [ ] 1.2 **[GREEN]** Add `run_v41` (creates `code_symbols` + indexes, sets `PRAGMA user_version=41`) and `run_v42` (creates `code_edges` + indexes, sets `PRAGMA user_version=42`), each guarded by `if version >= N { return Ok(()); }`. Append both to `run_all` **after** `run_v40`. **Do not register or modify `run_v39`.**
  - **Accept:** `cargo test` green; idempotent on re-run; `cargo clippy -- -D warnings` clean.

## Phase 2: Extraction — WU-B

- [ ] 2.1 **[RED]** `apps/backend/src/indexer/tree_sitter_chunker.rs`: write failing tests — Rust `struct Foo` + `impl Foo { fn bar }` → Struct symbol, Method symbol, `defines_method` edge from Foo→bar, `defines` edge File→Foo; Go `type_declaration` → Struct vs Interface vs Type by inner child; relative TS import → `imports` edge to resolved File; bare import `requests` → External stub + `imports` edge; unsupported `.yaml` → no symbols. Run `cargo test` — must fail.
- [ ] 2.2 **[GREEN]** Define `SymbolType` enum, `EdgeType` enum, `Persist` enum, `RawSymbol`, `RawEdge`, `FileGraph` structs.
- [ ] 2.3 **[GREEN]** Implement `collect_graph_data`: AST walk for Rust/TS/JS/Python/Go per spec mapping; emit container symbols (`impl_item` → Class, `class_*` → Class) with `defines_method` edges to their methods; import walk per language; resolve relative specifiers against `known_files`; emit External stub (`INSERT OR IGNORE`) for bare/unresolved imports.
- [ ] 2.4 **[GREEN]** Implement `chunk_with_graph`: single `parse_once` call → reuse tree for existing chunk logic AND `collect_graph_data`; returns `(Vec<RawChunk>, Option<FileGraph>)`; existing `chunk()` delegates to the same internal `parse_once` helper so existing tests still pass.
  - **Accept:** all new extraction tests green; existing chunk tests unbroken; `cargo clippy -- -D warnings` clean.

## Phase 3: Persistence + Indexer Wiring — WU-C/D

- [ ] 3.1 **[RED]** `apps/backend/src/db/queries.rs`: write failing tests — `"src/a/b.rs"` path synthesis yields Project + Folders `src`, `src/a` + File + `contains_*` edges; duplicate `persist_structure` call produces no extra rows; `persist_file_graph` delete+reinsert replaces only FileOwned rows; forced mid-transaction error leaves counts unchanged (rollback); `get_graph` with `node_type` filter returns only matching types; returned edges reference only nodes in the result set. Run `cargo test` — must fail.
- [ ] 3.2 **[GREEN]** Add `GraphNodeRow`, `GraphEdgeRow` structs. Implement `upsert_symbol` (Shared → `INSERT OR IGNORE` then `SELECT`; FileOwned → `INSERT` + `last_insert_rowid`), `get_symbol_id`, `insert_edge` (Shared → `INSERT OR IGNORE`; FileOwned → `INSERT`), `delete_symbols_for_file`, `delete_edges_for_file`.
- [ ] 3.3 **[GREEN]** Implement `persist_structure` (one `unchecked_transaction`, all INSERT OR IGNORE for Project/Folder/File/edge synthesis over all rel_paths) and `persist_file_graph` (one `unchecked_transaction`: delete FileOwned symbols → delete FileOwned edges → upsert symbols → insert edges; RAII rollback on Drop). Implement `get_graph` (2-step: SELECT nodes with filters + LIMIT/OFFSET → collect ids → SELECT edges WHERE both endpoints IN ids).
- [ ] 3.4 **[GREEN]** `apps/backend/src/indexer/mod.rs`: (a) after building the eligible `rel_path` set, build `known_files: HashSet<String>` and call `persist_structure` once (one lock); (b) replace `chunker.chunk(...)` with `chunker.chunk_with_graph(..., &known_files)`; (c) for each changed file with `Some(file_graph)`, acquire the lock once and call `persist_file_graph`. Do NOT use row-by-row autocommit for graph rows.
  - **Accept:** all persistence tests green; `cargo build` clean; `cargo clippy -- -D warnings` clean.

## Phase 4: Graph API — WU-E

- [ ] 4.1 **[RED]** `apps/backend/src/api/code.rs`: write failing handler tests (mirroring the existing `code.rs` test pattern) — valid project → 200 with `node_count`/`edge_count`; unknown project → 404 `project_not_indexed`; unauthenticated → 401; cross-org project → 404; `node_type=File,Function` → no Struct nodes in response; empty project → 200 `node_count=0`. Run `cargo test` — must fail.
- [ ] 4.2 **[GREEN]** Add `GraphNodeDto`, `GraphEdgeDto`, `GraphResponse` to `apps/backend/src/models/types.rs`. Implement `get_graph` handler in `api/code.rs`: `require_permission`, `get_code_project` org gate (None → 404), CSV param split, `limit.unwrap_or(5000).clamp(1, 20_000)`, call `db::get_graph`, build response.
- [ ] 4.3 **[GREEN]** Register `.route("/v1/code/graph", get(code::get_graph))` in `apps/backend/src/api/router.rs` after the existing code routes.
  - **Accept:** all handler tests green; `cargo build` clean.

## Phase 5: Frontend — WU-F

- [x] 5.1 Add `"react-force-graph-2d"` to `apps/admin/package.json` dependencies; run `npm install`.
- [x] 5.2 Add `GraphNode`, `GraphEdge`, `CodeGraph` interfaces to `apps/admin/src/types.ts` (matching the API response schema).
- [x] 5.3 Add `getCodeGraph(project, opts)` to `apps/admin/src/client.ts` beside `searchCode`; builds `URLSearchParams` from `node_type`, `edge_type`, `limit`, `offset`.
- [x] 5.4 Create `apps/admin/src/pages/code/GraphTab.tsx`: `useQuery(['code-graph', project, filters], getCodeGraph)`; `useMemo` data mapper `from_id→source`, `to_id→target`; `NODE_COLORS`/`EDGE_COLORS` lookup maps; `nodeCanvasObject` suppresses labels when `zoom ≤ LABEL_ZOOM_THRESHOLD`; default LOD = File + top-level symbols visible, Folder + External hidden; client-side filter toggles; external aggregation → one aggregate node when `externalCount > 150`, expandable; "No graph data" empty state when `node_count=0`.
- [x] 5.5 `apps/admin/src/pages/Code.tsx`: add `'graph'` to `Tab` union; add "Graph" entry to tab list; `const GraphTab = lazy(() => import('./code/GraphTab'))`; render `<Suspense fallback={…}><GraphTab …/></Suspense>` when active tab is `'graph'`.
- [x] 5.6 **[TEST]** Write `apps/admin/src/pages/code/GraphTab.test.tsx` (Vitest): mapper produces `source`/`target` from `from_id`/`to_id`; default LOD excludes Folder and External nodes; external aggregation produces exactly 1 aggregate node above threshold; empty `nodes=[]` renders "No graph data" without JS error.
  - **Accept:** `npm run build && npm run test` green (admin CI gate). ✅ DONE — 21 tests passing, build clean.

---

**Non-goal:** Do NOT register or fix `run_v39` in `run_all`. Pre-existing latent bug (defined but skipped in the `run_v38 → run_v40` chain) — flagged in design, out of scope for this change. v41/v42 append exclusively after `run_v40`.
