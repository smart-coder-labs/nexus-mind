# Exploration: code-knowledge-graph (Slice 1 — Structural)

> Replicate, inside NexusMind, a code knowledge graph like DeusData/codebase-memory-mcp:
> a force-directed graph UI showing repo structure. Slice 1 = STRUCTURAL ONLY.
> Engram: `sdd/code-knowledge-graph/explore` (obs 707).

## Scope

**In (Slice 1):** node types Project / Folder / File / Module / Function / Method /
Class / Struct / Interface / Type / Enum; edge types `contains_folder` /
`contains_file` / `defines` / `defines_method` / `imports`.

**Out (Slice 2+):** semantically-resolved edges `calls` / `usage` / `inherits`
(need cross-file resolution, akin to their C "Hybrid LSP"); `Field` nodes.

## Current state (Phase 1 shipped)

- `src/indexer/tree_sitter_chunker.rs` — `TreeSitterChunker` already parses ASTs and
  classifies `is_definition` / `is_container` / `is_method` for rust/ts/js/py/go.
  Import nodes are currently SKIPPED. Graph extraction must REUSE this parse (no 2nd parse).
- `src/indexer/mod.rs` — `index_project`: walk → chunk → embed → persist; per-file
  SHA-256 change detection, delete+reinsert on change (idempotency pattern to mirror).
- `src/db/migrations.rs` — linear `PRAGMA user_version` guards, currently **v40**.
- `src/api/code.rs` — `/v1/code/index`, `/search`, `/status/:project`, `/context`;
  single-writer `Arc<Mutex<Connection>>`.
- `apps/admin/src/pages/Code.tsx` — Repositories + Search tabs (TanStack Query v5),
  no graph lib in `package.json`.

## Decisions / recommendations

### Data model — TWO new tables (resolved-at-index)

```sql
-- v41
CREATE TABLE code_symbols (
    id, code_project_id FK code_projects ON DELETE CASCADE,
    file_path, file_hash,
    symbol_type,        -- Project|Folder|File|Module|Function|Method|Class|Struct|Interface|Type|Enum
    name, qualified_name,  -- e.g. "src/auth.rs::validate_token"
    start_line, end_line,  -- NULL for Project/Folder/File
    language, created_at
);
-- indexes: (code_project_id), (code_project_id, file_path),
--          UNIQUE(code_project_id, qualified_name)

-- v42
CREATE TABLE code_edges (
    id, code_project_id FK code_projects ON DELETE CASCADE,
    from_symbol_id FK code_symbols ON DELETE CASCADE,
    to_symbol_id   FK code_symbols ON DELETE CASCADE,
    edge_type,          -- contains_folder|contains_file|defines|defines_method|imports
    file_path,          -- owning file (for delete-on-reindex)
    created_at
);
-- indexes: (code_project_id), (from_symbol_id), (to_symbol_id), (code_project_id, file_path)
```

- **Idempotency:** mirror chunks — `delete_symbols_for_file` + `delete_edges_for_file`
  then re-insert per changed file. Project/Folder are virtual nodes → `INSERT OR IGNORE`
  on `UNIQUE(code_project_id, qualified_name)`; survive per-file reindex, CASCADE on project delete.
- **Transaction safety:** wrap per-file delete+reinsert in `BEGIN`/`COMMIT` to avoid orphan edges.

### Extraction — reuse the existing parse

Map tree-sitter node kinds → graph node types (all already parsed by the chunker;
containers `impl_item`/`class_definition` must be emitted as Struct/Class symbols even
though they are NOT emitted as `code_chunks`). New traversal for `imports`:
- Rust `use_declaration`, TS/JS `import_statement`, Python `import(_from)_statement`,
  Go `import_declaration` → resolve target to a File node by path heuristic; unresolved
  → `external` stub node (gray), import edge still drawn.
- Go `type_declaration` needs one extra child inspection (Struct vs Interface vs Type).

### API — resolved-at-index

```
GET /v1/code/graph?project={name}[&node_type=File,Function][&edge_type=defines,imports][&limit=5000][&offset=0]
→ { project, node_count, edge_count, nodes:[{id,type,name,qualified_name,file_path,start_line,end_line,language}], edges:[{id,from_id,to_id,type}] }
```
Permission: same as existing code routes. Org-scoped via the `code_project_id → code_projects.org_id` FK chain.

### Frontend — `react-force-graph-2d`

Same library family as the reference. 2D avoids the three.js payload (3D → Slice 2).
New "Graph" tab inside `Code.tsx`. LOD strategy for 20k+ nodes: default filter shows
File + top-level symbols, `nodeVisibility` controls simulation membership, labels only
above a zoom threshold, lazy-load the tab with `React.lazy`.

## Risks

1. SQLite single-writer contention during bulk extraction → batch per-file in one transaction.
2. Import resolution ambiguity (workspace paths, tsconfig aliases, namespace packages) → `external` stubs.
3. Virtual Folder node lifecycle → `INSERT OR IGNORE` + per-file scoped deletes.
4. Node-count ceiling ~10–20k in force-graph physics → server `limit` + client LOD filters.
5. Go `type_declaration` disambiguation.
6. code_chunks vs code_symbols misalignment — graph extractor is a SEPARATE pass with different output.

## Open questions for proposal

1. `Field` nodes deferred to Slice 2 — confirm.
2. `external` stub nodes for unresolved imports: include (gray) vs skip — recommend include.
3. Graph UI as a new tab in `Code.tsx` vs separate route — recommend tab.
4. Default node-type filter on first load — recommend File + top-level symbols.
