# Proposal: Code Knowledge Graph (Slice 1 — Structural)

## Intent

NexusMind indexes repos but only exposes them through text search. Users cannot *see* how an indexed codebase is structured. This change adds a force-directed graph of repo structure (folders, files, symbols, and their containment/import relationships), turning the index into a navigable map. It is a clear differentiator versus plain semantic search and the foundation for Slice 2 semantic edges. Phase 1 (AST-aware tree-sitter chunking) already ships the parse we reuse — extraction is incremental, not a rewrite.

## Scope

### In Scope
- Node types: Project, Folder, File, Module, Function, Method, Class, Struct, Interface, Type, Enum.
- Edge types: `contains_folder`, `contains_file`, `defines`, `defines_method`, `imports`.
- Two SQLite tables: `code_symbols` (v41), `code_edges` (v42); org-scoped via existing FK chain.
- Graph extraction extending `TreeSitterChunker` (reuse parse, no second pass).
- Resolved-at-index API: `GET /v1/code/graph` with `node_type`/`edge_type` filters, `limit=5000`.
- `react-force-graph-2d` "Graph" tab inside `Code.tsx`.

### Out of Scope (Slice 2)
- Semantic edges: `calls`, `usage`, `inherits` (need cross-file resolution).
- `Field` nodes — **deferred, confirmed**.
- 3D rendering (three.js payload).

## Resolved Decisions
1. **Field nodes** → deferred to Slice 2. Confirmed.
2. **Unresolved imports** → emit `external` stub nodes (gray); import edge still drawn. Confirmed.
3. **Graph UI** → new tab in `apps/admin/src/pages/Code.tsx`, not a route. Confirmed.
4. **Default first-load filter** → File + top-level symbols only; Folder/external hidden via client LOD. Confirmed.

## Capabilities

### New Capabilities
- `code-graph-extraction`: parse-time emission of symbols and structural/import edges, idempotent per file.
- `code-graph-api`: `GET /v1/code/graph` resolved-at-index, org-scoped, filterable.
- `code-graph-visualization`: force-directed graph tab with LOD and type filters.

### Modified Capabilities
- None.

## Approach

Mirror the existing chunk idempotency pattern. A `collect_graph_data(root, content)` pass over the *already-parsed* tree emits symbols + edges; containers (`impl_item`, `class_definition`) are emitted as Class/Struct symbols even though they are not chunks. Import statements (previously skipped) resolve to File nodes by path heuristic; unresolved targets become `external` stubs. Per changed file: `BEGIN` → delete owned symbols/edges → re-insert → `COMMIT`. Virtual Project/Folder nodes use `INSERT OR IGNORE` on `UNIQUE(code_project_id, qualified_name)`. The API is a pure indexed DB read; the UI lazy-loads the tab.

## Affected Areas

| Area | Impact | Description |
|------|--------|-------------|
| `apps/backend/src/db/migrations.rs` | Modified | Add v41 `code_symbols`, v42 `code_edges` |
| `apps/backend/src/indexer/tree_sitter_chunker.rs` | Modified | `collect_graph_data` extraction + import walk |
| `apps/backend/src/indexer/mod.rs` | Modified | Persist symbols/edges per file in one transaction |
| `apps/backend/src/db/queries.rs` | Modified | upsert/insert/delete + `get_graph` queries |
| `apps/backend/src/api/code.rs` | Modified | `GET /v1/code/graph` handler |
| `apps/backend/src/api/router.rs` | Modified | Register graph route |
| `apps/admin/src/pages/Code.tsx` | Modified | Graph tab |
| `apps/admin/src/api/client.ts` | Modified | `getCodeGraph` method |
| `apps/admin/src/types.ts` | Modified | `GraphNode`, `GraphEdge` types |
| `apps/admin/package.json` | Modified | Add `react-force-graph-2d` |

## Risks

| Risk | Likelihood | Mitigation |
|------|------------|------------|
| SQLite single-writer contention during bulk extraction | Med | One transaction per file, not row-by-row autocommit |
| Import path resolution ambiguity (workspace, tsconfig aliases) | High | Best-effort heuristic; unresolved → `external` stub nodes |
| Orphan edges on crash mid-reindex | Med | Wrap delete + reinsert in single `BEGIN`/`COMMIT` |
| Node-count ceiling (~10–20k) in force-graph physics | Med | Server `limit=5000` + client LOD default filter |
| Go `type_declaration` disambiguation (Struct/Interface/Type) | Low | Inspect inner child node kind |
| code_chunks vs code_symbols misalignment (exploded containers) | Med | Graph extractor emits containers explicitly as separate pass |

## Rollback Plan

Frontend: hide the Graph tab (feature-flag/remove tab entry) — no API dependency for existing flows. Backend: route is additive; unregister `GET /v1/code/graph`. Tables v41/v42 are additive and idempotent; leaving them in place is harmless (no reads if route removed). No data migration of existing tables, so reverting the migration head is unnecessary — existing search/index paths are untouched.

## Dependencies

- `react-force-graph-2d` (npm, ~400KB) added to `apps/admin`.
- Existing Phase 1 tree-sitter parse (already shipped).

## Success Criteria

- [ ] Indexing a repo populates `code_symbols` and `code_edges` (v41/v42 applied).
- [ ] `GET /v1/code/graph?project=X` returns org-scoped nodes + edges with filters and limit.
- [ ] Re-indexing a changed file replaces only its owned symbols/edges (no orphans, no duplicates).
- [ ] Unresolved imports appear as gray `external` nodes with an `imports` edge.
- [ ] Graph tab renders File + top-level symbols by default; type filters expand the view.
