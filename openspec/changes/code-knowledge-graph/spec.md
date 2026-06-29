# Code Knowledge Graph Specification (Slice 1 — Structural)

## Purpose

Formal requirements for change `code-knowledge-graph`. All three capabilities are new — this is a full spec, not a delta. Covers: schema migrations, graph extraction, idempotent re-index, graph API, frontend graph tab, and edge cases.

Capabilities: `code-graph-extraction` | `code-graph-api` | `code-graph-visualization`

---

## REQ-1: Schema Migrations (v41 / v42)

The system MUST create `code_symbols` (v41) and `code_edges` (v42) guarded by `PRAGMA user_version` so migrations are idempotent on re-run.

**code_symbols columns:** `id` PK, `code_project_id` FK → `code_projects` ON DELETE CASCADE, `file_path`, `file_hash`, `symbol_type` text, `name`, `qualified_name`, `start_line` nullable, `end_line` nullable, `language`, `created_at`.

**code_edges columns:** `id` PK, `code_project_id` FK → `code_projects` ON DELETE CASCADE, `from_symbol_id` FK → `code_symbols` ON DELETE CASCADE, `to_symbol_id` FK → `code_symbols` ON DELETE CASCADE, `edge_type` text, `file_path`, `created_at`.

**Required indexes:**

| Table | Index |
|-------|-------|
| `code_symbols` | `(code_project_id)` |
| `code_symbols` | `(code_project_id, file_path)` |
| `code_symbols` | `UNIQUE(code_project_id, qualified_name)` |
| `code_edges` | `(code_project_id)` |
| `code_edges` | `(from_symbol_id)` |
| `code_edges` | `(to_symbol_id)` |
| `code_edges` | `(code_project_id, file_path)` |

Org scoping is enforced via FK chain: `code_symbols.code_project_id → code_projects.org_id`.

#### Scenario: Fresh database — migrations applied

- GIVEN a database at user_version < 41
- WHEN the application starts and runs migrations
- THEN `code_symbols` and `code_edges` exist with all specified columns and indexes; `PRAGMA user_version` returns 42

#### Scenario: Already-migrated database — migrations are no-ops

- GIVEN a database already at user_version 42
- WHEN migrations run again
- THEN no error is raised; table structure is unchanged

---

## REQ-2: Graph Extraction on Index

The system MUST emit `code_symbols` and `code_edges` rows during file indexing by traversing the already-parsed tree-sitter AST (no second parse pass). Containers (`impl_item`, `class_definition`) MUST be emitted as `code_symbols` even though they are not emitted as `code_chunks`.

**AST node kind → symbol_type mapping:**

| Language | AST node kind | symbol_type |
|----------|--------------|-------------|
| Rust | `fn_item` | Function |
| Rust | `impl_item` | Class (container) |
| Rust | `struct_item` | Struct |
| Rust | `enum_item` | Enum |
| Rust | `trait_item` | Interface |
| Rust | `type_alias` | Type |
| Rust | `mod_item` | Module |
| TS/JS | `function_declaration`, `arrow_function` | Function |
| TS/JS | `class_declaration` | Class |
| TS/JS | `method_definition` | Method |
| TS/JS | `interface_declaration` | Interface |
| TS/JS | `type_alias_declaration` | Type |
| TS/JS | `enum_declaration` | Enum |
| Python | `function_definition` | Function |
| Python | `class_definition` | Class |
| Go | `func_declaration` (top-level) | Function |
| Go | `method_declaration` | Method |
| Go | `type_declaration` (struct inner child) | Struct |
| Go | `type_declaration` (interface inner child) | Interface |
| Go | `type_declaration` (other inner child) | Type |

Import statements MUST be traversed for all supported languages. Targets that resolve to a project File MUST emit an `imports` edge to that File symbol. Targets that do not resolve MUST create an `external` stub `code_symbols` row (`language = "external"`) and an `imports` edge to it.

Files in unsupported languages MUST be skipped without error; no symbols or edges are emitted for them.

#### Scenario: Rust file — struct and method emitted

- GIVEN a Rust file containing `struct Foo` and `impl Foo { fn bar(&self) }`
- WHEN the file is indexed
- THEN `code_symbols` contains Foo (symbol_type=Struct) and bar (symbol_type=Method); a `defines_method` edge exists from Foo → bar; a `defines` edge exists from the File → Foo

#### Scenario: Import resolved to project File

- GIVEN a TypeScript file that imports from `./utils` where `utils.ts` exists in the project
- WHEN the file is indexed
- THEN an `imports` edge connects the importing File symbol to the `utils.ts` File symbol; no external stub is created

#### Scenario: Unresolved import creates external stub

- GIVEN a Python file that imports `requests` (a package not in the project)
- WHEN the file is indexed
- THEN a `code_symbols` row exists with `symbol_type=external` and `name=requests`; an `imports` edge points to it

#### Scenario: Unsupported language file is skipped

- GIVEN a `.yaml` configuration file in the project
- WHEN the project is indexed
- THEN no `code_symbols` or `code_edges` rows reference that file path; no error is raised

---

## REQ-3: Idempotent Re-index

Per-file symbol/edge deletion and reinsertion MUST be wrapped in a single `BEGIN`/`COMMIT` transaction. On failure, the transaction MUST roll back leaving the previous state intact (no orphan edges).

Virtual Project and Folder nodes MUST use `INSERT OR IGNORE` on `UNIQUE(code_project_id, qualified_name)` so they survive per-file re-indexes and are removed only when the project is deleted (via CASCADE).

#### Scenario: Unchanged file — graph rows are stable

- GIVEN a file already indexed with symbols S1, S2 and edge E1
- WHEN the file is reindexed without content modification (same hash)
- THEN row counts for that file_path are identical; no duplicate rows exist

#### Scenario: Changed file — old symbols replaced atomically

- GIVEN a Rust file indexed with function `foo`; file is edited to rename `foo` → `bar`
- WHEN the file is reindexed
- THEN only `bar` exists for that file_path; no `foo` orphan remains; the swap is atomic

#### Scenario: Crash mid-transaction leaves no orphan edges

- GIVEN a file reindex begins (deletes completed, reinsert not finished) and the process is killed
- WHEN the database is reopened
- THEN no `code_edges` row has a `from_symbol_id` or `to_symbol_id` that does not exist in `code_symbols`

#### Scenario: Virtual Folder node survives sibling file reindex

- GIVEN project P has folder `src/` and two files beneath it; one file is reindexed
- WHEN reindex completes
- THEN the `src/` Folder symbol row still exists with the same id; no duplicate is created

---

## REQ-4: Graph API

The system MUST expose `GET /v1/code/graph` using the same authentication and permission middleware as existing code routes.

**Request parameters:**

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `project` | string | yes | — | Project name (caller's org) |
| `node_type` | csv | no | all types | Filter to listed symbol_types |
| `edge_type` | csv | no | all types | Filter to listed edge_types |
| `limit` | int | no | 5000 | Max nodes returned |
| `offset` | int | no | 0 | Pagination offset |

**Response envelope (200):**
```json
{
  "project": "<name>",
  "node_count": <int>,
  "edge_count": <int>,
  "nodes": [{ "id", "type", "name", "qualified_name", "file_path", "start_line", "end_line", "language" }],
  "edges": [{ "id", "from_id", "to_id", "type" }]
}
```

Org isolation MUST be enforced at the query level: a caller from org A MUST NOT receive any symbols or edges belonging to org B's projects, even when both orgs have a project with the same name.

#### Scenario: Valid project returns graph

- GIVEN org A has project P with 10 symbols and 8 edges indexed
- WHEN org A's key calls `GET /v1/code/graph?project=P`
- THEN status 200; `node_count=10`, `edge_count=8`; all nodes have `code_project_id` belonging to org A

#### Scenario: node_type filter narrows results

- GIVEN project P has File, Function, and Struct symbols
- WHEN `GET /v1/code/graph?project=P&node_type=File,Function`
- THEN response nodes contain only `type=File` or `type=Function`; no Struct nodes present

#### Scenario: Org isolation enforced

- GIVEN org A and org B each have a project named "app" with distinct symbols
- WHEN org A's key calls `GET /v1/code/graph?project=app`
- THEN response contains only org A's nodes and edges; org B's data is absent

#### Scenario: Unknown project returns 404

- GIVEN no project named "ghost" exists in the caller's org
- WHEN `GET /v1/code/graph?project=ghost`
- THEN response status is 404

#### Scenario: Unauthenticated request rejected

- GIVEN no valid API key is provided
- WHEN `GET /v1/code/graph?project=P`
- THEN response status is 401

---

## REQ-5: Frontend Graph Tab

The system MUST add a "Graph" tab to `apps/admin/src/pages/Code.tsx` rendered with `react-force-graph-2d`. The tab component MUST be lazy-loaded (`React.lazy`) so the library (~400 KB) does not affect initial page bundle. The tab MUST call `GET /v1/code/graph` via the typed `getCodeGraph` client method using TanStack Query.

**Default first-load state:** only File nodes and their direct top-level symbol children are visible. Folder and external nodes are hidden. The user MUST be able to toggle visibility per node_type and per edge_type via filter controls. Node labels MUST be suppressed below a configurable zoom threshold (LOD).

#### Scenario: Graph tab renders on project selection

- GIVEN an indexed project is selected in the UI
- WHEN the user clicks the "Graph" tab for the first time
- THEN the force-directed canvas renders; File nodes and top-level symbols are visible; Folder nodes are not visible; no full-page reload occurs

#### Scenario: Node-type filter expands the view

- GIVEN the Graph tab is active with default filters
- WHEN the user enables the Folder node-type filter
- THEN Folder nodes appear in the running simulation without page reload

#### Scenario: LOD suppresses labels at low zoom

- GIVEN a project with 3000+ nodes is rendered
- WHEN the graph canvas is zoomed out below the LOD threshold
- THEN node labels are not drawn; simulation frame rate remains acceptable

---

## REQ-6: Edge Cases

#### Scenario: Empty project — graph API returns empty envelope

- GIVEN a project exists but has no indexed files
- WHEN `GET /v1/code/graph?project=P` is called
- THEN status 200; response is `{ "node_count": 0, "edge_count": 0, "nodes": [], "edges": [] }`

#### Scenario: Duplicate virtual nodes not created

- GIVEN two files in the same folder are indexed in the same batch
- WHEN indexing completes
- THEN exactly one Folder symbol row exists for that folder path (INSERT OR IGNORE prevents duplicates)

#### Scenario: Empty project renders empty graph tab

- GIVEN the Graph tab is open for a project with no symbols
- WHEN the API response returns an empty nodes/edges array
- THEN the canvas renders with a "No graph data" message; no JavaScript error occurs
