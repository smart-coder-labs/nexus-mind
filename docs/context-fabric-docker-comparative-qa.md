# Context Fabric v3 — Docker Comparative QA

Date: 2026-08-08

## Scope

This QA compares the restored production backup snapshot with the Context Fabric
Docker runtime. It is **not** a live production-vs-staging benchmark: the source
application endpoint was not provided, so production is represented by the
read-only backup snapshot.

The remote PostgreSQL source was never modified. Its backup was copied into a
temporary local PostgreSQL container, then restored into a clean Docker SQLite
volume.

## Environment

- Docker project: `context-fabric-test`
- Backend: `http://localhost:8080`
- Admin: `http://localhost:3000`
- Backoffice: `http://localhost:3001`
- SQLite volume: `context-fabric-test_nexusmind_data`
- Local backup PostgreSQL: host port `55432`
- Embedding model: `nomic-embed-text-v1.5` (768d)
- Context Fabric migrations: v58, v59, v60

## Restore

- Backup status: complete
- Tables restored: 24
- Rows restored: 89,552
- Memories: 3,263
- Code chunks: 16,414
- Code symbols: 18,576
- Code edges: 31,716

The restored baseline generation was published locally as
`baseline-nomic-768-f32-v1`.

## Functional results

| Area | Result |
| --- | --- |
| Backend health | PASS |
| Admin login/dashboard | PASS |
| Admin backups page | PASS |
| Legacy search | PASS |
| Context assemble | PASS |
| Deterministic generation | PASS |
| Independent verify | PASS; 10/10 claims verified |
| Tampered evidence | PASS; rejected with `evidence_integrity_mismatch` |
| Migrations v58–v60 | PASS locally |
| Nomic embedding initialization | PASS |
| Semantic/hybrid memory search | PASS with partial embedding coverage |
| BQ shadow A5 | PASS; recall 0.30, promotion blocked |
| MRL+BQ shadow A6 | PASS; recall 0.225, promotion blocked |
| Code semantic search | PASS for `design-system` |
| Code graph/AST skeleton | PASS |

## Latency comparison

Ten local runs over restored data:

| Pipeline | p50 | p95 |
| --- | ---: | ---: |
| Legacy search | 8.66 ms | 20.48 ms |
| Context assemble + generate + verify | 75.34 ms | 119.03 ms |

Context Fabric is slower in this comparison because it performs compilation,
provenance checks, generation and independent verification. The measured benefit
is safety and verifiability, not lower latency.

## Embedding and code results

Memory search modes all returned HTTP 200:

- `keyword`: 10 results.
- `semantic`: 10 results.
- `hybrid`: 10 results.

The full memory backfill did not finish within ten minutes. Only a partial set
of 32 memory embeddings was available, so no recall or quality conclusion is
valid for the complete memory corpus.

The isolated synthetic lab shadow run enabled both BQ and MRL:

- A5 BQ recall: `0.30`, quality delta `0.70`.
- A6 MRL+BQ recall: `0.225`, quality delta `0.775`.
- Both remained `status=shadow`, `fallback=baseline`, `promotion=false`.
- These results are diagnostic only: the corpus uses 64d synthetic vectors and
  is not valid 768d NX-Gold promotion evidence.

The `design-system` project was reindexed successfully and semantic code search
returned scored results for component, accessibility and theme queries.

The private `kasymir-app-ui` reindex failed closed with
`PRIVATE_REPO_AUTH_FAILURE`, as expected without a GitHub token. Existing AST
data remained readable.

AST/graph checks returned:

- 300 nodes and 310 edges for a bounded graph query.
- Function and class skeleton nodes were available.
- File/folder/project structural nodes were available.
- Code snippets returned file, language, symbol and line boundaries.

## What improved

- Evidence cannot be replaced by client-provided content.
- Tenant, ACL, generation and freshness checks happen before verification.
- Provenance survives compilation and generation.
- Claims are independently verified instead of trusted from model output.
- Invalidated/tampered evidence fails closed.
- Baseline remains active when embeddings, BQ/MRL or gates are unavailable.
- AST skeletons provide structural code navigation without sending full files to
  a model.
- Semantic code search returns scored, symbol-aware results when embeddings are
  available.

## What regressed or remains unproven

- End-to-end Context Fabric latency is higher than legacy lexical search.
- Full memory semantic recall is unmeasured because backfill is incomplete.
- BQ/MRL recall, memory reduction and quality gates were not measured.
- BQ/MRL shadow measurements were below the SDD promotion threshold in the
  synthetic diagnostic run and therefore did not alter defaults.
- The full NX-Gold long protocol was not run.
- DeepSeek live generation was not tested; only offline transport tests exist.
- Private code reindex requires an authorized GitHub token.
- No live production API comparison was performed.

## Final assessment

The Docker deployment, restore path, UI, baseline Context Fabric path,
independent verification, semantic code search and AST skeleton access work on
the restored dataset. Context Fabric currently improves correctness, provenance,
security and failure behavior. It does not yet demonstrate a quality or latency
improvement over production because complete embeddings, BQ/MRL shadow evidence,
NX-Gold review and a live production endpoint are still missing.
