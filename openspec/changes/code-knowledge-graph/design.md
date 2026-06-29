# Design: Code Knowledge Graph (Slice 1 — Structural)

> Technical architecture for the structural code graph. Reads: proposal
> (`sdd/code-knowledge-graph/proposal`), exploration (`explore.md`). Feeds
> `sdd-tasks`. All decisions verified against the real code at the paths cited.

## Chosen approach (one sentence)

Add two org-isolated SQLite tables (`code_symbols` v41, `code_edges` v42) populated
by a **single-parse** extraction pass that extends `TreeSitterChunker`, persisted
per file inside **one explicit transaction**, and served read-only at
`GET /v1/code/graph` to a lazy-loaded `react-force-graph-2d` tab.

## Architecture at a glance

```
walk_files ──► [PRE-PASS] synthesize Project/Folder/File nodes + contains_* edges   (1 txn, all files, INSERT OR IGNORE)
            │
            └► per CHANGED file:
                 chunk_with_graph(file)  ── ONE tree-sitter parse ──► (RawChunk[], FileGraph)
                                                                          │
                 persist chunks (existing)                               │
                 persist_file_graph(file, FileGraph)  ◄──────────────────┘   (1 txn: delete owned → insert)

GET /v1/code/graph ──► get_graph(project_id, filters, limit, offset) ──► nodes + edges (edges constrained to returned nodes)
                                   │
                                   └► org isolation via code_project ownership (get_code_project filters org_id)

Code.tsx "Graph" tab ──► React.lazy(GraphTab) ──► getCodeGraph() ──► map {from_id,to_id}→{source,target} ──► force-graph-2d
```

Layering is preserved exactly as the codebase already separates it: extraction in
`indexer/`, persistence in `db/queries.rs`, schema in `db/migrations.rs`, transport
in `api/`, presentation in `apps/admin`. No new architectural boundary is introduced —
this is an additive vertical slice over the existing code-index pipeline.

---

## 1. Migrations — `apps/backend/src/db/migrations.rs`

**Verified:** current head is **v40** (`run_v40` sets `PRAGMA user_version = 40`).
New migrations are **v41** and **v42**. The public entry is `run()` → `run_all()`.

> GOTCHA (must handle): `run_v39` is **defined but NOT called** in `run_all` (the
> chain jumps `run_v38 → run_v40`). When appending `run_v41`/`run_v42`, append them
> **after** `run_v40(conn)?;`. Do not assume sequential auto-registration. The
> `run_v39` omission is a pre-existing latent bug — see Risks; flag, do not silently
> "fix" it inside this change unless tasks explicitly scope it.

### v41 — `code_symbols`

```sql
CREATE TABLE IF NOT EXISTS code_symbols (
  id              INTEGER PRIMARY KEY,
  code_project_id INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
  symbol_type     TEXT NOT NULL,     -- Project|Folder|File|Module|Function|Method|Class|Struct|Interface|Type|Enum|External
  name            TEXT NOT NULL,
  qualified_name  TEXT NOT NULL,     -- stable identity, see §3.1
  file_path       TEXT,             -- NULL for shared virtual nodes (Project/Folder/File/External); rel_path for code symbols
  file_hash       TEXT,
  start_line      INTEGER,          -- NULL for virtual nodes
  end_line        INTEGER,          -- NULL for virtual nodes
  language        TEXT,
  created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX        IF NOT EXISTS idx_code_symbols_project      ON code_symbols(code_project_id);
CREATE INDEX        IF NOT EXISTS idx_code_symbols_project_file ON code_symbols(code_project_id, file_path);
CREATE UNIQUE INDEX IF NOT EXISTS idx_code_symbols_qname        ON code_symbols(code_project_id, qualified_name);
PRAGMA user_version = 41;
```

### v42 — `code_edges`

```sql
CREATE TABLE IF NOT EXISTS code_edges (
  id              INTEGER PRIMARY KEY,
  code_project_id INTEGER NOT NULL REFERENCES code_projects(id) ON DELETE CASCADE,
  from_symbol_id  INTEGER NOT NULL REFERENCES code_symbols(id) ON DELETE CASCADE,
  to_symbol_id    INTEGER NOT NULL REFERENCES code_symbols(id) ON DELETE CASCADE,
  edge_type       TEXT NOT NULL,    -- contains_folder|contains_file|defines|defines_method|imports
  file_path       TEXT,             -- rel_path for file-owned edges; NULL for shared structural edges
  created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX        IF NOT EXISTS idx_code_edges_project      ON code_edges(code_project_id);
CREATE INDEX        IF NOT EXISTS idx_code_edges_from         ON code_edges(from_symbol_id);
CREATE INDEX        IF NOT EXISTS idx_code_edges_to           ON code_edges(to_symbol_id);
CREATE INDEX        IF NOT EXISTS idx_code_edges_project_file ON code_edges(code_project_id, file_path);
CREATE UNIQUE INDEX IF NOT EXISTS idx_code_edges_unique
  ON code_edges(code_project_id, from_symbol_id, to_symbol_id, edge_type);
PRAGMA user_version = 42;
```

Idempotency guard matches existing style exactly:

```rust
pub fn run_v41(conn: &Connection) -> Result<()> {
    let version: i32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version >= 41 { return Ok(()); }
    conn.execute_batch(/* DDL above, ends with PRAGMA user_version = 41; */)?;
    Ok(())
}
// run_v42 identical shape, guard `>= 42`.
```

> **Decision — org isolation by ownership, not a redundant `org_id` column.**
> Both tables hang off `code_project_id`. `code_projects` already carries `org_id`,
> and every read path resolves the project through `get_code_project(org_id, name)`
> first (which filters by org). Adding `org_id` to the leaf tables would denormalize
> without an access pattern that needs it (db-design §4/§5: no FK-less copy without a
> read that demands it). `ON DELETE CASCADE` guarantees no orphan symbols/edges when a
> project is deleted — an invariant enforced by the DB, not the app (db-design §10).

> **Decision — `UNIQUE(code_project_id, from_symbol_id, to_symbol_id, edge_type)`.**
> Lets shared structural edges (`contains_folder`/`contains_file`) be created with
> `INSERT OR IGNORE` and survive per-file reindex without duplicating. Without it,
> the pre-pass would multiply folder edges on every index run.

---

## 2. Extraction — `apps/backend/src/indexer/tree_sitter_chunker.rs`

**Constraint honored:** ONE tree-sitter parse per file. The current `chunk()` parses
internally and drops the tree. We add a sibling entry point that parses once and
returns BOTH outputs; the indexer switches to it.

### 2.1 New public surface

```rust
pub struct RawSymbol {
    pub symbol_type: SymbolType,     // enum incl. virtual + External
    pub name: String,
    pub qualified_name: String,
    pub start_line: Option<i64>,     // None for virtual nodes
    pub end_line: Option<i64>,
    pub language: Option<String>,
    pub persist: Persist,            // Shared (INSERT OR IGNORE) | FileOwned (delete+reinsert)
}

pub struct RawEdge {
    pub from_qname: String,
    pub to_qname: String,
    pub edge_type: EdgeType,         // ContainsFolder|ContainsFile|Defines|DefinesMethod|Imports
    pub persist: Persist,
}

pub struct FileGraph { pub symbols: Vec<RawSymbol>, pub edges: Vec<RawEdge> }

impl TreeSitterChunker {
    /// Single parse → chunks (unchanged semantics) + code-level graph data.
    /// Returns FileGraph = None for unsupported langs / parse failure
    /// (File/Folder nodes are still synthesized from the path elsewhere).
    pub fn chunk_with_graph(
        &self, file_path: &str, file_hash: &str, language: Option<&str>,
        content: &str, known_files: &HashSet<String>,
    ) -> (Vec<RawChunk>, Option<FileGraph>);

    /// Walks the ALREADY-PARSED root. No second parse. Pure (testable in isolation
    /// by passing a tree root built in the test).
    fn collect_graph_data(
        &self, root: Node, file_path: &str, file_hash: &str,
        language: &str, content: &str, known_files: &HashSet<String>,
    ) -> FileGraph;
}
```

`chunk()` stays as the trait method (delegates to a private `parse_once` helper so the
parse is shared); existing chunk tests keep passing unchanged.

### 2.2 Node-kind → symbol_type mapping (reuses the chunker's existing classifiers)

| Language | tree-sitter kind | symbol_type |
|----------|------------------|-------------|
| Rust | `function_item` (top-level) | Function |
| Rust | `function_item` (in `impl_item`) | Method |
| Rust | `struct_item` | Struct |
| Rust | `enum_item` | Enum |
| Rust | `trait_item` | Interface |
| Rust | `type_item` | Type |
| Rust | `mod_item` | Module |
| Rust | `impl_item` | Class *(container, emitted as symbol — see below)* |
| Python | `function_definition` (top) / `decorated_definition` | Function |
| Python | `function_definition` (in `class_definition`) | Method |
| Python | `class_definition` | Class |
| TS/JS | `function_declaration`/`generator_function_declaration` | Function |
| TS/JS | `method_definition` | Method |
| TS/JS | `class_declaration`/`abstract_class_declaration` | Class |
| TS | `interface_declaration` | Interface |
| TS | `type_alias_declaration` | Type |
| TS | `enum_declaration` | Enum |
| Go | `function_declaration` | Function |
| Go | `method_declaration` | Method |
| Go | `type_declaration` | Struct \| Interface \| Type *(disambiguate by inner child)* |

**Container handling.** `is_container()` kinds (`impl_item`, `class_definition`,
`class_declaration`, `abstract_class_declaration`) are emitted as a **symbol** (Class
for classes, the implemented-type name as Class for Rust `impl`), even though the
existing chunker does NOT emit them as chunks. Their methods (via `collect_methods()`,
already present) are emitted as Method symbols with a `defines_method` edge from the
container symbol. This is the "explicit container pass" the proposal calls for — it
resolves the chunks-vs-symbols misalignment because graph extraction has its own
output shape.

**Go `type_declaration` disambiguation.** Inspect the inner `type_spec` child:
`struct_type` → Struct, `interface_type` → Interface, else Type. (Cheap, one extra
child read — reuses the parsed node, no reparse.)

### 2.3 Virtual Project/Folder/File synthesis (path-based, parse-independent)

Synthesized from `rel_path`, NOT from the AST, so they exist for EVERY file regardless
of language. To avoid the import-ordering hazard (see §3.2), this synthesis runs in a
**pre-pass over all walked files** (§4), not inside `collect_graph_data`. Given
`rel_path = "src/a/b.rs"`:

- Project node: `symbol_type=Project`, `qualified_name="<project-root>"`, `Shared`.
- Folder nodes: `src`, `src/a` (`qualified_name = folder path`), `Shared`.
- File node: `qualified_name = "src/a/b.rs"`, `Shared`, `file_path = NULL`.
- Edges (`Shared`, `INSERT OR IGNORE`): `contains_folder` Project→`src`, `src`→`src/a`;
  `contains_file` `src/a`→File.

> **Decision — File nodes are SHARED (not file-owned), code symbols are FileOwned.**
> Incoming `imports` edges point at a target File node. If File nodes were deleted on
> every reindex, those edges would CASCADE-delete and the target id would churn. Making
> File/Folder/Project/External nodes shared and stable (identity = `qualified_name`)
> means only Function/Method/Class/... symbols + their `defines`/`imports` edges are
> rewritten per file. Stable endpoints = no ordering problem, no edge loss.

### 2.4 Import edges + resolution heuristic

Import statements (currently skipped by the chunker) are walked per language:

- Rust `use_declaration`, TS/JS `import_statement`, Python `import_statement` /
  `import_from_statement`, Go `import_declaration`.

Resolution → a `to_qname`:

1. Extract the raw module spec string.
2. **Relative specifiers** (`./`, `../`, Rust `crate::`/`super::`/`self::`, Python
   leading-dot): resolve against the importer's directory, try candidate file paths
   with known extensions / `index.*` / `mod.rs`. If a candidate ∈ `known_files`
   (the walker's set of rel_paths) → `imports` edge to that File node.
3. **Unresolved or bare specifiers** (`react`, `os`, Go module paths): emit an
   `External` stub node `qualified_name = "external:<spec>"`, `Shared`
   (`INSERT OR IGNORE`), and still draw the `imports` edge. Go imports are
   predominantly external in Slice 1 (acceptable, documented limitation).

`imports` edges are `FileOwned` (from = importer File, `file_path = rel_path`), so they
are rewritten when the importer changes.

---

## 3. Persistence — `apps/backend/src/db/queries.rs` + `indexer/mod.rs`

### 3.1 `qualified_name` identity rules (uniqueness invariant)

- Project: `"<project-root>"` (single per project).
- Folder: the folder rel path (`"src/a"`).
- File: the file rel path (`"src/a/b.rs"`).
- External: `"external:<spec>"`.
- Code symbol: `"{rel_path}::{name}#{start_line}"` — `#start_line` guarantees per-file
  uniqueness for overloads / same-named nested defs (avoids `UNIQUE` violation on the
  plain `INSERT` used for file-owned symbols). Method: include container —
  `"{rel_path}::{Container}::{name}#{start_line}"`.

### 3.2 New query functions

```rust
// virtual-node-safe upsert; returns the row id either way.
pub fn upsert_symbol(conn, code_project_id, sym: &RawSymbol) -> Result<i64>;
//   Shared    → INSERT OR IGNORE ... ; then SELECT id WHERE (project, qualified_name)
//   FileOwned → INSERT ...           ; conn.last_insert_rowid()

pub fn get_symbol_id(conn, code_project_id, qualified_name) -> Result<Option<i64>>;
pub fn insert_edge(conn, code_project_id, from_id, to_id, edge_type, file_path: Option<&str>) -> Result<()>;
//   Shared edge → INSERT OR IGNORE (dedup via UNIQUE);  FileOwned → INSERT

pub fn delete_symbols_for_file(conn, code_project_id, file_path) -> Result<()>;
//   DELETE FROM code_symbols WHERE code_project_id=? AND file_path=?   (virtual nodes have file_path NULL → untouched)
pub fn delete_edges_for_file(conn, code_project_id, file_path) -> Result<()>;
//   DELETE FROM code_edges   WHERE code_project_id=? AND file_path=?

/// Structural pre-pass for ALL files — ONE transaction, all INSERT OR IGNORE.
pub fn persist_structure(conn, code_project_id, rel_paths: &[String]) -> Result<()>;

/// Per-file code symbols + defines/imports edges — ONE transaction.
pub fn persist_file_graph(conn, code_project_id, file_path, graph: &FileGraph) -> Result<()>;

pub fn get_graph(conn, code_project_id, node_types: &[String], edge_types: &[String],
                 limit: i64, offset: i64) -> Result<(Vec<GraphNodeRow>, Vec<GraphEdgeRow>)>;
```

### 3.3 Transaction mandate (NON-NEGOTIABLE for sdd-apply)

`persist_structure` and `persist_file_graph` MUST each run inside **one** explicit
transaction. Use `rusqlite`'s `conn.unchecked_transaction()` — it borrows `&Connection`
(matching every other query signature in this file), the `Arc<Mutex<Connection>>`
already serializes writers, and `Transaction`'s `Drop` rolls back automatically if
`commit()` is not reached (orphan-edge safety on mid-reindex panic/error).

```rust
pub fn persist_file_graph(conn: &Connection, pid: i64, file_path: &str, g: &FileGraph) -> Result<()> {
    let tx = conn.unchecked_transaction()?;          // BEGIN
    delete_symbols_for_file(&tx, pid, file_path)?;
    delete_edges_for_file(&tx, pid, file_path)?;
    for s in &g.symbols { upsert_symbol(&tx, pid, s)?; }
    for e in &g.edges {
        let from = get_symbol_id(&tx, pid, &e.from_qname)?;
        let to   = get_symbol_id(&tx, pid, &e.to_qname)?;
        if let (Some(f), Some(t)) = (from, to) {
            insert_edge(&tx, pid, f, t, e.edge_type.as_str(), e.persist.file_path(file_path))?;
        }
    }
    tx.commit()?;                                    // COMMIT
    Ok(())
}
```

> **sdd-apply MUST NOT** regress this to row-by-row autocommit (a `db.lock()` +
> single `insert_*` per row, like the existing chunk loop). That pattern is correct for
> chunks but would (a) defeat the orphan-edge guarantee and (b) hammer the single
> writer. One lock acquisition wraps the whole per-file transaction.

### 3.4 `get_graph` shape (avoids dangling edges)

Mirror the existing two-step `get_chunks_by_ids` pattern:

1. `SELECT ... FROM code_symbols WHERE code_project_id=? [AND symbol_type IN (...)]
   ORDER BY id LIMIT ? OFFSET ?` → collect node rows + their ids.
2. `SELECT ... FROM code_edges WHERE code_project_id=? [AND edge_type IN (...)]
   AND from_symbol_id IN (ids) AND to_symbol_id IN (ids)` → only edges whose BOTH
   endpoints are in the returned node set. Prevents the frontend from receiving edges
   that reference filtered-out / paged-out nodes.

### 3.5 Wiring into `index_project` (`indexer/mod.rs`)

Verified current loop: lock-per-op, SHA-256 skip, delete+reinsert chunks per changed
file. Changes:

1. After computing the eligible `rel_path` set (post exclude-pattern filter), build
   `known_files: HashSet<String>` and call `persist_structure(conn, pid, &all_rel_paths)`
   ONCE (one lock, one txn) — creates the full File/Folder tree even for unchanged/
   unparsed files, so the graph shows the whole repo immediately and import targets
   always resolve.
2. Replace `chunker.chunk(...)` with `chunker.chunk_with_graph(..., &known_files)`.
   Keep chunk persistence exactly as-is.
3. For each changed file with `Some(file_graph)`, after chunk persistence acquire the
   lock once and call `persist_file_graph(conn, pid, &rel_path, &file_graph)`.

---

## 4. API — `apps/backend/src/api/code.rs` + `router.rs`

### 4.1 Handler

```rust
#[derive(Deserialize)]
pub struct GraphParams {
    pub project: String,
    pub node_type: Option<String>,   // CSV: "File,Function"
    pub edge_type: Option<String>,   // CSV: "defines,imports"
    pub limit: Option<i64>,          // default 5000, hard cap 20000
    pub offset: Option<i64>,         // default 0
}

pub async fn get_graph(
    State(store): State<SqliteStore>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<GraphParams>,
) -> Result<Json<GraphResponse>, (StatusCode, Json<ApiError>)>;
```

Flow (mirrors `get_status`/`post_search`):
1. `require_permission(&conn, &auth, None, "memory:search")`.
2. `get_code_project(&auth.org_id, &params.project, &conn)` → 404
   `project_not_indexed` if `None`. **This is the org-isolation gate** — a project
   from another org returns `None` → 404, so its symbols/edges are never reachable.
3. Parse `code_project_id`; split CSV filters; `limit = limit.unwrap_or(5000).clamp(1, 20000)`.
4. `get_graph(...)` → build DTOs.

### 4.2 Response DTOs (`models/types.rs`)

```rust
pub struct GraphResponse { project: String, node_count: i64, edge_count: i64,
                           nodes: Vec<GraphNodeDto>, edges: Vec<GraphEdgeDto> }
pub struct GraphNodeDto { id: i64, r#type: String, name: String, qualified_name: String,
                          file_path: Option<String>, start_line: Option<i64>,
                          end_line: Option<i64>, language: Option<String> }
pub struct GraphEdgeDto { id: i64, from_id: i64, to_id: i64, r#type: String }
```

### 4.3 Route registration (`router.rs`, after line 104)

```rust
.route("/v1/code/graph", get(code::get_graph))
```

Additive only — rollback = delete this line (proposal rollback plan).

---

## 5. Frontend — `apps/admin`

### 5.1 Dependency

`apps/admin/package.json` → add `react-force-graph-2d` (~400KB; 2D avoids the three.js
payload — 3D is Slice 2). The weight justifies lazy loading (frontend-patterns §Code
Splitting).

### 5.2 Types (`types.ts`)

```ts
export interface GraphNode { id: number; type: string; name: string; qualified_name: string;
  file_path: string | null; start_line?: number | null; end_line?: number | null; language?: string | null }
export interface GraphEdge { id: number; from_id: number; to_id: number; type: string }
export interface CodeGraph { project: string; node_count: number; edge_count: number;
  nodes: GraphNode[]; edges: GraphEdge[] }
```

### 5.3 Client (`client.ts`, beside `searchCode`)

```ts
getCodeGraph(project: string, opts: { node_type?: string; edge_type?: string; limit?: number; offset?: number } = {}): Promise<CodeGraph> {
  const qs = new URLSearchParams({ project });
  if (opts.node_type) qs.set('node_type', opts.node_type);
  if (opts.edge_type) qs.set('edge_type', opts.edge_type);
  if (opts.limit != null) qs.set('limit', String(opts.limit));
  if (opts.offset != null) qs.set('offset', String(opts.offset));
  return this.request(`/v1/code/graph?${qs}`);
}
```

### 5.4 Tab + lazy component

- `Code.tsx`: extend `type Tab` with `'graph'`, add a `TABS` entry, and render
  `<Suspense fallback={…}><GraphTab projects={projects} /></Suspense>` when active.
- New file `apps/admin/src/pages/code/GraphTab.tsx`, imported via
  `const GraphTab = lazy(() => import('./code/GraphTab'))`. Inside, dynamically render
  `react-force-graph-2d`. TanStack Query: `useQuery(['code-graph', project, filters], () => client.getCodeGraph(project, filters))`.

### 5.5 Data mapping (the unit-tested seam)

API returns `{from_id,to_id}`; force-graph wants `links:[{source,target}]`. Pure mapper,
memoized:

```ts
const data = useMemo(() => ({
  nodes: graph.nodes.map(n => ({ id: n.id, type: n.type, name: n.name, fp: n.file_path })),
  links: graph.edges.map(e => ({ source: e.from_id, target: e.to_id, type: e.type })),
}), [graph]);
```

### 5.6 Visual encoding + LOD

- **Node color by type** — a `NODE_COLORS: Record<string,string>` map (Project, Folder,
  File, Module, Function, Method, Class, Struct, Interface, Type, Enum, **External=gray
  `#6b7280`**). Apply via `nodeColor`.
- **Edge color by type** — `EDGE_COLORS` map (contains_* muted, defines/defines_method
  accent, imports distinct). Apply via `linkColor`.
- **Labels above zoom threshold** — render labels in `nodeCanvasObject` only when the
  current zoom `k > LABEL_ZOOM_THRESHOLD` (e.g. `1.5`); otherwise dots only. Avoids
  label soup at full-graph zoom.
- **Default LOD filter** — first load shows **File + top-level symbols**
  (Function/Class/Struct/Interface/Type/Enum); **Folder and External hidden**. Filter
  client-side from the fetched set so toggles are instant; server `limit` is the ceiling.
- **Filter controls** — node-type and edge-type pill/checkbox toggles (a small
  `useState<Set<string>>`), driving the client-side filter.

### 5.7 RESOLVED open detail — external-node volume control

> **Decision — collapse externals into a single aggregate node past a threshold.**
> Bare-specifier imports (`react`, `os`, every Go std import) can flood the canvas.
> Rule: External nodes are **hidden by default** (§5.6). When the user enables them, if
> `externalCount > EXTERNAL_COLLAPSE_THRESHOLD` (recommend **150**), render ONE synthetic
> aggregate node labeled `External dependencies (N)` that absorbs all `imports` edges
> to externals (re-pointed to the aggregate); clicking it expands to the real stubs
> (toggle `expandExternals`). Below the threshold, render them individually. This caps
> physics cost and visual noise without dropping data, and stays entirely client-side
> (no API/schema change). Rejected alternatives: (a) drop externals server-side — loses
> the "this file pulls in N deps" signal; (b) hard per-request cap — arbitrary
> truncation, non-deterministic which deps show.

---

## 6. Testing strategy

**Strict TDD is ACTIVE for backend.** Write the failing test first, then implement.
Backend test command: `cargo test --manifest-path apps/backend/Cargo.toml`.

### Backend (Rust) — order

1. **Migrations** (`migrations.rs` tests): `run()` brings a `:memory:` DB to
   `user_version >= 42`; `code_symbols` and `code_edges` exist with expected columns;
   re-running `run()` is idempotent (no error, version stays 42).
2. **Path synthesis** (`persist_structure` / helper): `"src/a/b.rs"` yields Project +
   Folders `src`, `src/a` + File + correct `contains_*` edges; duplicate calls do not
   multiply nodes/edges (INSERT OR IGNORE).
3. **Extraction per language** (`collect_graph_data`): rust / ts / tsx / js / py / go
   each emit the expected `symbol_type`s; container emitted as Class + `defines_method`
   to its methods; Go `type_declaration` → Struct vs Interface vs Type; relative import
   → `imports` edge to resolved File; bare import → External stub + edge.
4. **Idempotent reindex** (`persist_file_graph`): persisting the same file twice keeps
   symbol/edge counts stable (no duplicates); changing a file replaces only its
   FileOwned rows; shared File/Folder nodes survive.
5. **Transaction rollback**: force an error mid-`persist_file_graph` (e.g. a bad edge)
   → assert no partial symbols/edges were committed (counts unchanged from before).
6. **`get_graph` filters**: `node_type` returns only those types; `edge_type` filters
   edges; returned edges never reference a node outside the returned set; `limit`/
   `offset` paginate.
7. **Org isolation** (handler test, like the existing `code.rs` suite): a graph request
   for a project owned by another org returns 404 `project_not_indexed`; unauthenticated
   returns 401.

### Admin (Vitest)

- **GraphTab data mapping**: API payload → `{nodes, links}` with `from_id→source`,
  `to_id→target`; default LOD filter excludes Folder + External; `NODE_COLORS`/
  `EDGE_COLORS` return expected values; external aggregation kicks in above
  `EXTERNAL_COLLAPSE_THRESHOLD` and collapses to one node.

---

## 7. Sequencing

1. **Migrations** v41 + v42; register `run_v41`/`run_v42` in `run_all` after `run_v40`.
2. **queries.rs**: structs + `upsert_symbol`, `get_symbol_id`, `insert_edge`,
   `delete_symbols_for_file`, `delete_edges_for_file`, `persist_structure`,
   `persist_file_graph`, `get_graph`.
3. **tree_sitter_chunker.rs**: `RawSymbol`/`RawEdge`/`FileGraph` + `collect_graph_data`
   + `chunk_with_graph` (single parse) + import walk + mapping.
4. **indexer/mod.rs**: structural pre-pass + per-file graph persist wiring.
5. **api/code.rs** + **models/types.rs**: `get_graph` handler + DTOs.
6. **api/router.rs**: register `GET /v1/code/graph`.
7. **types.ts** + **client.ts**: types + `getCodeGraph`.
8. **package.json** + **GraphTab.tsx** + **Code.tsx** tab.

Backend (1→6) lands independently and is shippable behind no frontend; the tab (7→8) is
purely additive. Matches the proposal's rollback story.

---

## 8. Risks & mitigations

| Risk | Mitigation |
|------|------------|
| SQLite single-writer contention | One transaction per file (`unchecked_transaction`); structural pre-pass also one txn; keep txns short, no I/O inside |
| Orphan edges on crash mid-reindex | RAII rollback via `Transaction::Drop`; FK `ON DELETE CASCADE` |
| Node-count ceiling (~10–20k force-graph) | Server `limit` default 5000 / cap 20000; client default LOD (File + top symbols); external aggregation |
| Import resolution false-externals (aliases, workspaces, Go modules) | Best-effort heuristic + `known_files` set; unresolved → External stub (data preserved); documented Slice-1 limitation |
| `run_v39` not registered in `run_all` (pre-existing) | FLAGGED — append v41/v42 after `run_v40`; do not assume auto-chaining. Decide separately whether to also register v39 |
| Second parse cost | `chunk_with_graph` parses once for both chunks and graph |
| Stale nodes for deleted files / emptied folders | Accepted for Slice 1 (matches existing chunk behavior, which also keeps deleted-file chunks); client LOD hides folders by default |
| `qualified_name` collisions (overloads/nested) | `#start_line` (and container for methods) in the qualified_name |

## 9. ADR summary

1. Two tables, org-isolation by `code_project_id` ownership (no redundant `org_id`). Rejected: `org_id` on leaf tables (denormalization without an access pattern).
2. File/Folder/Project/External = shared stable nodes; only code symbols are file-owned. Rejected: file-owned File nodes (id churn + import-edge loss).
3. Single tree-sitter parse via `chunk_with_graph`. Rejected: separate `collect_graph_data` reparse.
4. One explicit transaction per file via `unchecked_transaction`. Rejected: row-by-row autocommit (orphan risk, writer pressure).
5. `get_graph` constrains edges to the returned node set. Rejected: returning all edges (dangling-edge bugs in the renderer).
6. External nodes hidden by default, collapsed into one aggregate past 150. Rejected: server-side drop / hard cap.
